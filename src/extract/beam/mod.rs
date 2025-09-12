//! Beam extraction implementation.
mod candidate;
mod egraph;
mod top_k;

use self::{
    candidate::Candidate,
    egraph::{ClassId, FastEgraph, NodeId, UInt},
    top_k::TopK,
};
use crate::INFINITY;
use crate::{
    extract::{ExtractionResult, Extractor},
    Cost,
};
use arrayvec::ArrayVec;
use egraph_serialize::{ClassId as ExtClassId, EGraph as ExtEGraph, NodeId as ExtNodeId};
use indexmap::IndexMap;
use parking_lot::RwLock;
use rand::seq::SliceRandom;
use rayon::prelude::*;
use std::fmt::Debug;
use std::hash::Hash;
use std::mem::swap;
use std::ops::Range;
use std::time::Instant;
use std::{collections::HashSet, sync::atomic::AtomicBool};

pub struct BeamExtractor<const BEAM: usize>;

type EGraph<U, const BEAM: usize> =
    FastEgraph<U, ExtClassId, ExtNodeId, RwLock<TopK<Candidate<U>, BEAM>>>;

struct BeamExtract<U: Copy + Ord + Hash, const BEAM: usize> {
    egraph: EGraph<U, BEAM>,
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
        let result: ExtractionResult = if let Ok(egraph) = EGraph::<u16, BEAM>::try_from(egraph) {
            log::info!(
                "Using 16-bit indices. Fast egraph conversion in {:?}",
                start.elapsed()
            );
            let mut extractor: BeamExtract<u16, BEAM> = BeamExtract { egraph };
            extractor.iterate();
            extractor.extract_solution(roots)
        } else if let Ok(egraph) = EGraph::<u32, BEAM>::try_from(egraph) {
            log::info!(
                "Using 32-bit indices. Fast egraph conversion in {:?}",
                start.elapsed()
            );
            let mut extractor: BeamExtract<u32, BEAM> = BeamExtract { egraph };
            extractor.iterate();
            extractor.extract_solution(roots)
        } else if let Ok(egraph) = EGraph::<usize, BEAM>::try_from(egraph) {
            log::info!(
                "Using {}-bit indices. Fast egraph conversion in {:?}",
                usize::BITS,
                start.elapsed()
            );
            let mut extractor: BeamExtract<usize, BEAM> = BeamExtract { egraph };
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
    U: Send + Sync,
    <U as TryInto<usize>>::Error: Debug,
    <U as TryFrom<usize>>::Error: Debug,
    Range<U>: Iterator<Item = U> + ExactSizeIterator + DoubleEndedIterator + Clone + Debug,
{
    fn extract_solution(&self, roots: &[ExtClassId]) -> ExtractionResult {
        let mut roots = roots
            .iter()
            .map(|ext_cid| self.egraph.from_class_id(ext_cid).unwrap())
            .collect::<Vec<_>>();
        roots.sort();
        roots.dedup();

        let candidates = self.candidates(&roots, None, INFINITY);
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
        let mut changed_global = true;

        // Start with leaf nodes as initial workset
        let mut workset: HashSet<NodeId<U>> = self
            .egraph
            .all_nodes()
            .filter(|&nid| self.egraph.children(nid).is_empty())
            .collect();
        let next_workset = RwLock::new(HashSet::new());

        while changed_global {
            loop_counter += 1;
            log::info!("Beam extraction global iteration {}", loop_counter);
            changed_global = false;

            if workset.is_empty() {
                // Add all nodes for 2nd and subsequent iterations.
                workset.extend(self.egraph.all_nodes());
            }

            while !workset.is_empty() {
                let worklist: Vec<NodeId<U>> = workset.drain().collect();
                log::info!("Beam extraction local workset {} nodes", worklist.len());
                let changed_any = AtomicBool::new(false);

                worklist.par_iter().for_each(|&nid| {
                    match self.recompute_node(nid) {
                        NodeStatus::NotReady => {
                            // Presumably the non-ready child is already in the worklist.
                            // When it becomes ready, it will re-trigger this node as a parent.
                            // If not, then the node was cyclic.
                        }
                        NodeStatus::Unchanged => {}
                        NodeStatus::Updated => {
                            changed_any.store(true, std::sync::atomic::Ordering::SeqCst);
                            let cid = self.egraph.node_class(nid);
                            let parents = self.egraph.parents(cid);
                            next_workset.write().extend(parents.iter().copied());
                        }
                    }
                });

                swap(&mut workset, &mut next_workset.write());

                if changed_any.load(std::sync::atomic::Ordering::SeqCst) {
                    changed_global = true;
                }
            }
        }

        // Assert stability
        // for nid in self.egraph.nodes.keys() {
        //     assert_ne!(self.recompute_node(nid), NodeStatus::Updated);
        // }
    }

    fn recompute_node(&self, nid: NodeId<U>) -> NodeStatus {
        // Check if all children are ready
        let ready = self
            .egraph
            .children(nid)
            .iter()
            .all(|&child_cid| !self.egraph.memo(child_cid).read().is_empty());
        if !ready {
            return NodeStatus::NotReady;
        }

        // Compute cutoff cost for node
        let cid = self.egraph.node_class(nid);
        let cutoff = self
            .egraph
            .memo(cid)
            .read()
            .cutoff()
            .map_or(INFINITY, |c| c.cost());

        // Generate candidates by combining top-k from children
        let candidates = self.node_candidates(nid, cutoff);

        // Insert candidate into memo, return true if changed
        let updated = self.egraph.memo(cid).write().merge(candidates);

        if updated {
            NodeStatus::Updated
        } else {
            NodeStatus::Unchanged
        }
    }

    /// Generate candidates that include the given node.
    /// Cuts off candidates that cannot improve on the given cutoff cost.
    fn node_candidates(&self, nid: NodeId<U>, cutoff: Cost) -> TopK<Candidate<U>, BEAM> {
        let cid = self.egraph.node_class(nid);
        let cost = self.egraph.cost(nid);
        if cost >= cutoff {
            return TopK::new(); // Can't improve on cutoff
        }
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
        // TODO: Fix cutoff value
        let mut candidates =
            self.candidates(children, Some(cid), /* cutoff - cost */ INFINITY);
        for candidate in candidates.candidates_mut() {
            candidate.insert(cid, nid, cost);
        }
        candidates
    }

    /// Generate candidates for the given roots, optionally banning one class (to avoid cycles).
    /// Cuts off candidates that cannot improve on the given cutoff cost.
    ///
    /// Assumes roots are distinct.
    fn candidates(
        &self,
        roots: &[ClassId<U>],
        ban: Option<ClassId<U>>,
        cutoff: Cost,
    ) -> TopK<Candidate<U>, BEAM> {
        // Make sure all roots have candidates and compute lower bound cost
        let mut lower_bound = Cost::default();
        for &cid in roots {
            if self.egraph.memo(cid).read().is_empty() {
                return TopK::new(); // No candidates for this root
            };
            lower_bound += self.egraph.min_cost(cid);
        }
        if lower_bound >= cutoff {
            return TopK::new(); // Can't improve on cutoff
        }

        // Randomly permute roots to avoid bias
        let mut roots = roots.to_vec();
        roots.shuffle(&mut rand::thread_rng());

        // Create a snapshot of the root beams to avoid locking issues
        // let root_beams = roots
        //     .iter()
        //     .map(|&cid| (cid, self.egraph.memo(cid).read().clone()))
        //     .collect::<Vec<_>>();
        // TODO: Benchmark against locking inside the loop.

        // Generate candidates
        let mut candidates = TopK::singleton(Candidate::empty());
        //        for (i, (cid, root_beam)) in root_beams.into_iter().enumerate() {
        for (i, &cid) in roots.iter().enumerate() {
            let remaining_roots = &roots[i + 1..];

            // Sort existing solutions in partial ones and ones that already contain this root.
            let mut partials = ArrayVec::<_, BEAM>::new();
            let mut new_candidates = TopK::new();
            for candidate in candidates.into_iter() {
                if candidate.contains(cid) {
                    // Already contains this root
                    new_candidates.consider(candidate);
                } else {
                    partials.push(candidate);
                }
            }

            // Complete the partial solutions.
            if !partials.is_empty() {
                lower_bound -= self.egraph.min_cost(cid);
                //for candidate in root_beam.candidates() {
                for candidate in self.egraph.memo(cid).read().candidates() {
                    if let Some(ban) = ban {
                        if candidate.contains(ban) {
                            // Banned (e.g. this would create a cycle)
                            continue;
                        }
                    }
                    for partial in &partials {
                        if let Some(candidate) =
                            partial.merge(candidate, |nid| self.egraph.cost(nid))
                        {
                            let cutoff = new_candidates
                                .cutoff()
                                .map_or(INFINITY, |c| c.cost())
                                .min(cutoff);
                            let lower_bound: Cost = remaining_roots
                                .iter()
                                .copied()
                                .filter(|&cid| !candidate.contains(cid))
                                .map(|cid| self.egraph.min_cost(cid))
                                .sum();
                            if candidate.cost() + lower_bound >= cutoff {
                                continue;
                            }
                            new_candidates.consider(candidate);
                        }
                    }
                }
            }
            if new_candidates.is_empty() {
                return TopK::new(); // No candidates left
            }
            candidates = new_candidates;
        }
        candidates
    }
}
