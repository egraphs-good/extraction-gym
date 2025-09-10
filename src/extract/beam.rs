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
            parents: HashMap::new(),
        };
        extractor.compute_parents();
        extractor.iterate();
        let mut choices = IndexMap::new();
        for (cid, candidates) in extractor.memo {
            if let Some(best) = candidates.best() {
                choices.insert(cid.clone(), best.choices[&cid].0.clone());
            }
        }
        let duration = start.elapsed();
        let result = ExtractionResult { choices };

        result.check(egraph);

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

/// A valid partial solution.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Candidate {
    choices: HashMap<ClassId, (NodeId, Cost)>,
    cost: Cost,
}

impl PartialOrd for Candidate {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Candidate {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // First by cost, then by choices (to ensure uniqueness)
        match self.cost.cmp(&other.cost) {
            Ordering::Equal => self.choices.iter().cmp(other.choices.iter()),
            ord => ord,
        }
    }
}

struct BeamExtract<'a> {
    egraph: &'a EGraph,
    beam: usize,
    memo: HashMap<ClassId, TopK<Candidate>>,
    parents: HashMap<ClassId, Vec<NodeId>>,
}

impl<'a> BeamExtract<'a> {
    fn compute_parents(&mut self) {
        let mut parents: HashMap<ClassId, HashSet<NodeId>> = self
            .egraph
            .classes()
            .keys()
            .map(|cid| (cid.clone(), HashSet::new()))
            .collect();
        for (nid, node) in &self.egraph.nodes {
            for cid in &node.children {
                let child = &self.egraph[cid];
                parents
                    .entry(child.eclass.clone())
                    .or_default()
                    .insert(nid.clone());
            }
        }
        self.parents = parents
            .into_iter()
            .map(|(cid, pset)| (cid, pset.into_iter().collect()))
            .collect();
    }

    fn iterate(&mut self) {
        let mut worklist: HashSet<NodeId> = self.egraph.nodes.keys().cloned().collect();
        while let Some(nid) = worklist.iter().next().cloned() {
            worklist.remove(&nid);
            if self.recompute_node(&nid) {
                let cid = self.egraph.nid_to_cid(&nid);
                worklist.extend(self.parents.get(cid).unwrap().iter().cloned());
            }
        }
    }

    fn recompute_node(&mut self, nid: &NodeId) -> bool {
        let node = &self.egraph[nid];

        // Check if all children are ready
        let ready = node
            .children
            .iter()
            .all(|child_nid| self.memo.contains_key(&self.egraph[child_nid].eclass));
        if !ready {
            return false;
        }

        let cutoff = self
            .memo
            .get(&node.eclass)
            .and_then(|topk| topk.cutoff().map(|c| c.cost))
            .unwrap_or(INFINITY);

        // Generate candidates by combining top-k from children
        let Some(candidate) = self.generate_candidate(nid, node, cutoff) else {
            return false;
        };

        // Insert candidate into memo, return true if changed
        self.memo
            .entry(node.eclass.clone())
            .or_insert_with(|| TopK::new(self.beam))
            .consider(candidate)
    }

    fn generate_candidate(&mut self, nid: &NodeId, node: &Node, cutoff: Cost) -> Option<Candidate> {
        if node.children.is_empty() {
            return Some(Candidate {
                choices: HashMap::from([(node.eclass.clone(), (nid.clone(), node.cost))]),
                cost: node.cost,
            });
        }

        // Get unique classes of children.
        let mut childrens_classes = node
            .children
            .iter()
            .map(|c| self.egraph[c].eclass.clone())
            .collect::<Vec<ClassId>>();
        childrens_classes.sort();
        childrens_classes.dedup();
        if childrens_classes.contains(&node.eclass) {
            return None;
        }

        // Early exit if single child and can't be cheaper
        let first_cost = self.memo[&childrens_classes[0]].best().unwrap().cost;
        if node.cost + first_cost > cutoff {
            return None;
        }

        // Clone the biggest set and insert the others into it.
        let id_of_biggest = childrens_classes
            .iter()
            .max_by_key(|s| self.memo[s].best().unwrap().choices.len())
            .unwrap();
        let choices = &self.memo[id_of_biggest].best().unwrap().choices;
        if choices.contains_key(&node.eclass) {
            return None;
        }
        let mut choices = choices.clone();
        for child_cid in &childrens_classes {
            if child_cid == id_of_biggest {
                continue;
            }

            let next_choices = self.memo[child_cid].best().unwrap().choices.clone();
            for (can_cid, (can_nid, can_cost)) in next_choices.iter() {
                if can_cid == &node.eclass {
                    // This would create a cycle
                    return None;
                }
                choices.insert(can_cid.clone(), (can_nid.clone(), *can_cost));
            }
        }
        assert!(!choices.contains_key(&node.eclass), "Contains cycle");

        choices.insert(node.eclass.clone(), (nid.clone(), node.cost));
        let cost = choices.values().map(|(_, cost)| *cost).sum();
        Some(Candidate { choices, cost })
    }
}

/// A simple data structure to keep the top-k unique elements seen so far.
/// Orders elements by their `Ord` implementation, smallest first.
#[derive(Clone, Debug)]
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

    fn best(&self) -> Option<&T> {
        self.data.first()
    }

    fn worst(&self) -> Option<&T> {
        self.data.last()
    }

    fn cutoff(&self) -> Option<&T> {
        if self.data.len() < self.k {
            None
        } else {
            self.worst()
        }
    }

    fn candidates(&self) -> &[T] {
        &self.data
    }

    /// Consider a new candidate, return true if kept
    fn consider(&mut self, item: T) -> bool {
        match self.data.binary_search(&item) {
            Ok(_) => false, // Duplicate
            Err(index) if index < self.k => {
                if self.data.len() == self.k {
                    self.data.pop();
                }
                self.data.insert(index, item);
                true
            }
            Err(_) => false, // Too large
        }
    }

    fn merge(&mut self, other: Self) -> bool {
        let mut changed = false;
        for item in other.data {
            changed |= self.consider(item);
        }
        changed
    }
}
