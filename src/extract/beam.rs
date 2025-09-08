//! Beam extraction implementation.
use super::*;
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::time::Instant;

pub struct BeamExtractor {
    pub beam: usize,
}

impl Extractor for BeamExtractor {
    fn extract(&self, egraph: &EGraph, roots: &[ClassId]) -> ExtractionResult {
        let start = Instant::now();
        let mut extractor = BeamExtract {
            egraph,
            beam: self.beam,
            memo: HashMap::default(),
            reachable: HashSet::new(),
            parents: HashMap::new(),
        };
        extractor.reachability(roots);
        extractor.compute_parents();
        extractor.iterate();
        let solution = extractor
            .extract_multiple(roots)
            .into_iter()
            .next()
            .unwrap();
        let mut choices = IndexMap::new();
        for (cid, (nid, _)) in solution.choices {
            choices.insert(cid, nid);
        }
        let duration = start.elapsed();
        let result = ExtractionResult { choices };
        let cost = result.dag_cost(egraph, roots);
        log::info!(
            "Beam extraction (beam={}) found cost {} in {:?}",
            self.beam,
            cost,
            duration
        );
        result
    }
}

/// A simple data structure to keep the top-k unique elements seen so far.
struct TopK<T: Ord> {
    k: usize,
    data: Vec<T>,
}

impl<T: Ord> TopK<T> {
    fn new(k: usize) -> Self {
        Self {
            k,
            data: Vec::with_capacity(k),
        }
    }

    fn push(&mut self, item: T) {
        match self.data.binary_search(&item) {
            Ok(_) => {} // Duplicate
            Err(index) if index < self.k => {
                if self.data.len() == self.k {
                    self.data.pop();
                }
                self.data.insert(index, item);
            }
            Err(_) => {} // Too large
        }
    }

    /// Consume and return the top-k elements as a sorted `Vec`.
    fn into_inner(self) -> Vec<T> {
        self.data
    }
}

/// A valid partial solution.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Partial {
    cost: NotNan<f64>,
    // Node and level (for cycle detection)
    choices: IndexMap<ClassId, (NodeId, u32)>,
}

impl PartialOrd for Partial {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Partial {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        match self.cost.cmp(&other.cost) {
            Ordering::Equal => self.choices.iter().cmp(other.choices.iter()),
            ord => ord,
        }
    }
}

impl Partial {
    #[track_caller]
    fn test(&self, egraph: &EGraph, roots: &[ClassId]) {
        // Test cost
        let mut cost = NotNan::new(0.0).unwrap();
        for (_, (nid, _)) in &self.choices {
            cost += egraph[nid].cost;
        }
        assert_eq!(cost, self.cost, "Cost mismatch");

        // Test roots
        for root in roots {
            assert!(
                self.choices.contains_key(root),
                "Missing root choice: {root}"
            );
        }

        // Test children
        for (cid, (nid, _)) in &self.choices {
            let node = &egraph[nid];
            for child in &node.children {
                let child_cid = egraph.nid_to_cid(child);
                assert!(
                    self.choices.contains_key(child_cid),
                    "Missing child choice: {child_cid} (from {cid})"
                );
            }
        }

        // Test no cycles
        for (cid, (nid, level)) in &self.choices {
            let node = &egraph[nid];
            for child in &node.children {
                let child_cid = egraph.nid_to_cid(child);
                let child_level = self.choices[child_cid].1;
                assert!(
                    child_level < *level,
                    "Cycle detected: {cid} (level {level}) -> {child_cid} (level {child_level})"
                );
            }
        }
    }

    /// Merge two partial solutions. Returns `None` if they have conflicting choices.
    fn merge(&self, other: &Self, egraph: &EGraph) -> Option<Self> {
        // Check compatibility
        for (cid, (nid, _)) in &self.choices {
            if let Some(other_nid) = other.choices.get(cid) {
                if nid != &other_nid.0 {
                    return None;
                }
            }
        }

        // Merge
        let mut result = self.clone();
        for (cid, nid) in &other.choices {
            if result.choices.contains_key(cid) {
                continue;
            }
            result.choices.insert(cid.clone(), nid.clone());
            result.cost += egraph[&nid.0].cost;
        }

        // result.test(egraph, &[]);
        Some(result)
    }
}

struct BeamExtract<'a> {
    egraph: &'a EGraph,
    beam: usize,
    memo: HashMap<ClassId, Vec<Partial>>,
    reachable: HashSet<ClassId>,
    parents: HashMap<ClassId, Vec<ClassId>>,
}

impl<'a> BeamExtract<'a> {
    fn reachability(&mut self, roots: &[ClassId]) {
        let mut worklist = roots.to_vec();
        while let Some(cid) = worklist.pop() {
            if self.reachable.insert(cid.clone()) {
                for nid in &self.egraph[&cid].nodes {
                    for child in &self.egraph[nid].children {
                        let child_cid = self.egraph.nid_to_cid(child);
                        worklist.push(child_cid.clone());
                    }
                }
            }
        }
    }

    fn compute_parents(&mut self) {
        let mut parents: HashMap<ClassId, HashSet<ClassId>> = self
            .egraph
            .classes()
            .keys()
            .map(|cid| (cid.clone(), HashSet::new()))
            .collect();
        for node in self.egraph.nodes.values() {
            for child in &node.children {
                let child_cid = self.egraph.nid_to_cid(child);
                parents
                    .entry(child_cid.clone())
                    .or_default()
                    .insert(node.eclass.clone());
            }
        }
        self.parents = parents
            .into_iter()
            .map(|(cid, pset)| (cid, pset.into_iter().collect()))
            .collect();
    }

    fn iterate(&mut self) {
        let mut worklist: HashSet<ClassId> = self.reachable.iter().cloned().collect();
        while let Some(cid) = worklist.iter().next().cloned() {
            worklist.remove(&cid);
            let before = self.memo.get(&cid).map_or(0, |v| v.len());
            self.ensure_class(cid.clone());
            let after = self.memo.get(&cid).map_or(0, |v| v.len());
            if after > before {
                worklist.extend(self.parents.get(&cid).unwrap().iter().cloned());
            }
        }
    }

    fn extract_multiple(&mut self, roots: &[ClassId]) -> Vec<Partial> {
        // Build up results one root at a time.
        let mut all = TopK::new(self.beam);
        all.push(Partial {
            cost: NotNan::new(0.0).unwrap(),
            choices: IndexMap::new(),
        });
        let mut partial_roots = Vec::new();
        for root in roots {
            partial_roots.push(root.clone());
            let Some(candidates) = self.memo.get(root) else {
                // No solutions for this root.
                return Vec::new();
            };
            let mut new_all = TopK::new(self.beam);
            for partial in &all.data {
                for candidate in candidates {
                    if let Some(merged) = partial.merge(candidate, self.egraph) {
                        // merged.test(self.egraph, partial_roots.as_slice());

                        new_all.push(merged);
                    }
                }
            }
            all = new_all;
        }
        all.into_inner()
    }

    fn ensure_class(&mut self, cid: ClassId) {
        // Combine all e-nodes with all candidates for their children.
        let mut all = TopK::new(self.beam);
        for nid in &self.egraph[&cid].nodes {
            let node = &self.egraph[nid];
            let child_classes = node
                .children
                .iter()
                .map(|nid| self.egraph.nid_to_cid(nid).clone())
                .collect::<Vec<_>>();
            let candidates = self.extract_multiple(child_classes.as_slice());
            for mut candidate in candidates {
                if candidate.choices.contains_key(&cid) {
                    continue; // Cycle
                }
                let max_level = candidate
                    .choices
                    .values()
                    .map(|(_, level)| *level)
                    .max()
                    .unwrap_or(0);
                candidate
                    .choices
                    .insert(cid.clone(), (nid.clone(), max_level + 1));
                candidate.cost += node.cost;
                // candidate.test(self.egraph, &[cid.clone()]);
                all.push(candidate);
            }
        }
        self.memo.insert(cid, all.into_inner());
    }
}
