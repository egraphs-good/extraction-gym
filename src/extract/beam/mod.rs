//! Beam extraction implementation.
mod candidate;
mod egraph;
mod top_k;

use self::{
    candidate::Candidate,
    egraph::{ClassId, FastEgraph, NodeId, UInt},
    top_k::TopK,
};
use crate::extract::{ExtractionResult, Extractor};
use egraph_serialize::{
    ClassId as ExtClassId, EGraph as ExtEGraph, Node as ExtNode, NodeId as ExtNodeId,
};
use indexmap::IndexMap;
use std::collections::{HashMap, HashSet};
use std::fmt::Debug;
use std::hash::Hash;
use std::mem::swap;
use std::ops::Range;
use std::time::Instant;

pub struct BeamExtractor<const BEAM: usize>;

type EGraph<U> = FastEgraph<U, ExtClassId, ExtNodeId>;

struct BeamExtract<U: Copy + Ord + Hash, const BEAM: usize> {
    egraph: EGraph<U>,
    memo: HashMap<ClassId<U>, TopK<Candidate<U>, BEAM>>,
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
enum NodeStatus {
    NotReady,
    Unchanged,
    Updated,
}

impl<const BEAM: usize> Extractor for BeamExtractor<BEAM> {
    fn extract(&self, egraph: &ExtEGraph, roots: &[ExtClassId]) -> ExtractionResult {
        let start = Instant::now();
        let result: ExtractionResult = if let Ok(egraph) = FastEgraph::<u16, _, _>::try_from(egraph)
        {
            log::info!(
                "Using 16-bit indices. Fast egraph conversion in {:?}",
                start.elapsed()
            );
            let mut extractor: BeamExtract<u16, BEAM> = BeamExtract {
                egraph,
                memo: HashMap::default(),
            };
            extractor.iterate();
            extractor.extract_solution(roots)
        } else if let Ok(egraph) = FastEgraph::<u32, _, _>::try_from(egraph) {
            log::info!(
                "Using 32-bit indices. Fast egraph conversion in {:?}",
                start.elapsed()
            );
            let mut extractor: BeamExtract<u32, BEAM> = BeamExtract {
                egraph,
                memo: HashMap::default(),
            };
            extractor.iterate();
            extractor.extract_solution(roots)
        } else if let Ok(egraph) = FastEgraph::<usize, _, _>::try_from(egraph) {
            log::info!(
                "Using {}-bit indices. Fast egraph conversion in {:?}",
                usize::BITS,
                start.elapsed()
            );
            let mut extractor: BeamExtract<usize, BEAM> = BeamExtract {
                egraph,
                memo: HashMap::default(),
            };
            extractor.iterate();
            extractor.extract_solution(roots)
        } else {
            panic!("EGraph too large for beam extraction");
        };
        let duration = start.elapsed();
        let cost = result.dag_cost(egraph, roots);
        log::info!("Beam extraction (beam={BEAM}) found cost {cost} in {duration:?}",);
        result
    }
}

impl<U: UInt, const BEAM: usize> BeamExtract<U, BEAM>
where
    <U as TryInto<usize>>::Error: Debug,
    <U as TryFrom<usize>>::Error: Debug,
    Range<U>: Iterator<Item = U> + ExactSizeIterator + DoubleEndedIterator + Clone + Debug,
{
    fn extract_solution(&self, roots: &[ExtClassId]) -> ExtractionResult {
        let roots = roots
            .iter()
            .map(|ext_cid| self.egraph.from_class_id(ext_cid).unwrap())
            .collect::<Vec<_>>();

        let candidates = self.candidates(&roots, None);
        let solution = candidates
            .best()
            .expect("No candidate found for the given roots");

        let mut choices = IndexMap::new();
        for (cid, nid) in solution.iter() {
            let cid = self.egraph.class_id(cid).clone();
            let nid = self.egraph.node_id(nid).clone();
            choices.insert(cid, nid);
        }
        ExtractionResult { choices }
    }

    fn iterate(&mut self) {
        let mut loop_counter = 0;
        // Process the worklist until stable
        let mut changed = true;
        while changed {
            loop_counter += 1;
            dbg!(loop_counter);
            log::info!("Beam extraction iteration {}", loop_counter);
            changed = false;

            // Start with leaf nodes
            let mut worklist: HashSet<NodeId<U>> = self
                .egraph
                .all_nodes()
                .filter_map(|nid| {
                    if self.egraph.children(nid).is_empty() {
                        Some(nid.clone())
                    } else {
                        None
                    }
                })
                .collect();
            let mut next_worklist = HashSet::new();

            while !worklist.is_empty() {
                for nid in worklist.drain() {
                    match self.recompute_node(nid) {
                        NodeStatus::NotReady => {
                            // Presumably the non-ready child is already in the worklist.
                            // When it becomes ready, it will re-trigger this node as a parent.
                            // If not, then the node was cyclic.
                        }
                        NodeStatus::Unchanged => {}
                        NodeStatus::Updated => {
                            changed = true;
                            let cid = self.egraph.node_class(nid);
                            let parents = self.egraph.parents(cid);
                            // dbg!(nid, cid, parents);
                            next_worklist.extend(parents.iter().copied());
                        }
                    }
                }
                swap(&mut worklist, &mut next_worklist);
            }
        }

        // Assert stability
        // for nid in self.egraph.nodes.keys() {
        //     assert_ne!(self.recompute_node(nid), NodeStatus::Updated);
        // }
    }

    fn recompute_node(&mut self, nid: NodeId<U>) -> NodeStatus {
        // Check if all children are ready
        let ready = self
            .egraph
            .children(nid)
            .iter()
            .all(|child_cid| self.memo.contains_key(child_cid));
        if !ready {
            return NodeStatus::NotReady;
        }

        // Generate candidates by combining top-k from children
        let candidates = self.node_candidates(nid);

        // Insert candidate into memo, return true if changed
        let cid = self.egraph.node_class(nid);
        let updated = self
            .memo
            .entry(cid)
            .or_insert_with(|| TopK::new())
            .merge(candidates);

        if updated {
            NodeStatus::Updated
        } else {
            NodeStatus::Unchanged
        }
    }

    fn node_candidates(&self, nid: NodeId<U>) -> TopK<Candidate<U>, BEAM> {
        let cid = self.egraph.node_class(nid);
        let cost = self.egraph.cost(nid);
        let children = self.egraph.children(nid);
        if children.is_empty() {
            return TopK::singleton(Candidate::leaf(cid, nid, cost));
        }
        if children.contains(&cid) {
            // Self-cycle, can't be part of valid solution.
            // TODO: We should filter these out of the egraph earlier.
            // Same with unreachable nodes.
            return TopK::new();
        }

        // Generate candidates and add this node
        let mut candidates = self.candidates(children, Some(cid));
        for candidate in candidates.candidates_mut() {
            candidate.insert(cid, nid, cost);
        }
        candidates
    }

    fn candidates(
        &self,
        roots: &[ClassId<U>],
        ban: Option<ClassId<U>>,
    ) -> TopK<Candidate<U>, BEAM> {
        // Make sure all roots have candidates
        if !roots
            .iter()
            .all(|cid| self.memo.get(cid).and_then(|top| top.best()).is_some())
        {
            return TopK::new();
        }

        // Generate candidates
        let mut candidates = TopK::singleton(Candidate::empty());
        for rid in roots {
            let mut new_candidates = TopK::new();
            for candidate in self.memo[rid].candidates() {
                if let Some(ban) = ban {
                    if candidate.contains(ban) {
                        // Banned (e.g. this would create a cycle)
                        continue;
                    }
                }
                for existing in candidates.candidates() {
                    new_candidates.consider(existing.merge(candidate));
                }
            }
            if new_candidates.is_empty() {
                return TopK::new(); // No valid candidates
            }
            candidates = new_candidates;
        }
        candidates
    }
}
