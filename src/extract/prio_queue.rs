use rustc_hash::{FxHashMap, FxHashSet};

use super::*;

pub struct PrioQueueExtractor;

impl Extractor for PrioQueueExtractor {
    fn extract(&self, egraph: &EGraph, _roots: &[ClassId]) -> ExtractionResult {
        let mut parents = IndexMap::<ClassId, Vec<NodeId>>::with_capacity(egraph.classes().len());
        let n2c = |nid: &NodeId| egraph.nid_to_cid(nid);
        let mut analysis_pending: PrioQueue<NodeId, Cost> = PrioQueue::new();

        // counts how many child classes of this node still require to be constructed
        // (it counts multiple references to the same e-class only once)
        let mut child_counter: IndexMap<NodeId, usize> = IndexMap::new();

        for class in egraph.classes().values() {
            parents.insert(class.id.clone(), Vec::new());
        }

        let mut result = ExtractionResult::default();
        let mut costs = FxHashMap::<ClassId, Cost>::with_capacity_and_hasher(
            egraph.classes().len(),
            Default::default(),
        );

        for class in egraph.classes().values() {
            for node in &class.nodes {
                let child_classes: FxHashSet<&ClassId> =
                    egraph[node].children.iter().map(n2c).collect();

                child_counter.insert(node.clone(), child_classes.len());

                for c in child_classes {
                    parents.get_mut(c).unwrap().push(node.clone());
                }

                // start the analysis from leaves
                if egraph[node].is_leaf() {
                    let cost = result.node_sum_cost(egraph, &egraph[node], &costs);
                    analysis_pending.insert(node.clone(), cost);
                }
            }
        }

        while let Some((node_id, _cost)) = analysis_pending.pop() {
            let class_id = n2c(&node_id);
            if costs.contains_key(class_id) {
                continue;
            }

            let node = &egraph[&node_id];
            let cost = result.node_sum_cost(egraph, node, &costs);
            result.choose(class_id.clone(), node_id.clone());
            costs.insert(class_id.clone(), cost);
            for p in parents[class_id].iter() {
                if costs.contains_key(&n2c(p)) {
                    continue;
                }

                let ctr = child_counter.get_mut(p).unwrap();
                *ctr -= 1;
                if *ctr == 0 {
                    let cost = result.node_sum_cost(egraph, &egraph[p], &costs);
                    analysis_pending.insert(p.clone(), cost);
                }
            }
        }

        result
    }
}

mod prio {
    use std::cmp::{Ord, Ordering};
    use std::collections::BinaryHeap;

    // Takes the `Ord` from U, but reverses it.
    #[derive(PartialEq, Eq, Debug)]
    struct WithOrdRev<T: Eq, U: Ord>(pub T, pub U);

    impl<T: Eq, U: Ord> PartialOrd for WithOrdRev<T, U> {
        fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
            // It's the other way around, because we want a min-heap!
            other.1.partial_cmp(&self.1)
        }
    }
    impl<T: Eq, U: Ord> Ord for WithOrdRev<T, U> {
        fn cmp(&self, other: &Self) -> Ordering {
            self.partial_cmp(&other).unwrap()
        }
    }

    pub struct PrioQueue<T: Eq, C: Ord>(BinaryHeap<WithOrdRev<T, C>>);

    impl<T: Eq, C: Ord> PrioQueue<T, C> {
        pub fn new() -> Self {
            PrioQueue(BinaryHeap::new())
        }

        pub fn pop(&mut self) -> Option<(T, C)> {
            self.0.pop().map(|WithOrdRev(t, c)| (t, c))
        }

        pub fn insert(&mut self, t: T, c: C) {
            self.0.push(WithOrdRev(t, c));
        }
    }
}
use prio::*;
