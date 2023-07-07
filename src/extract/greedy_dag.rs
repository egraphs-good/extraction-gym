//! A variant of the greedy algorithm that greedily minimizes DAG cost, rather
//! than tree cost.
//!
//! To do this, the algorithm keeps track of the set of nodes _and_ their
//! optimal cost, rather than just the minimum cost of each node. We use a
//! variant of HAMTs with PATRICIA-style unions (with incrementally computed
//! aggregates) to make this efficient: larger benchmarks are 2-3x faster using
//! this data-structure. That data-structure is in the `val-trie` crate.
//!
//! The current implementation here is fairly simplistic, and a lot of unions
//! are recomputed unnecessarily; there is a lot of room for improvement here in
//! terms of runtime. Still, the largest examples in the data-set finish in a
//! few seconds.

use std::{collections::HashMap, rc::Rc};

use egraph_serialize::{ClassId, NodeId as SrcNodeId};
use ordered_float::NotNan;

use crate::{EGraph, ExtractionResult, Extractor};

pub(crate) struct GreedyDagExtractor;

impl Extractor for GreedyDagExtractor {
    fn extract(&self, egraph: &EGraph, _roots: &[ClassId]) -> ExtractionResult {
        let node_centric = WrapperEgraph::new(egraph);
        node_centric.extract()
    }
}

/// A wrapper around the input Egraph type which keeps track of node costs in an
/// array for incremental node set cost computations.
struct WrapperEgraph<'a> {
    inner: &'a EGraph,
    // Indexed by NodeId
    node_costs: Rc<Vec<NotNan<f64>>>,
}

type NodeId = usize;

impl<'a> WrapperEgraph<'a> {
    fn new(egraph: &'a EGraph) -> WrapperEgraph<'a> {
        let mut node_costs = Vec::with_capacity(egraph.nodes.len());
        for (_, node) in &egraph.nodes {
            node_costs.push(node.cost);
        }
        WrapperEgraph {
            inner: egraph,
            node_costs: Rc::new(node_costs),
        }
    }

    fn empty_node_set(&self) -> NodeSet {
        NodeSet {
            trie: Default::default(),
            costs: self.node_costs.clone(),
        }
    }

    fn compute_cost(&self, node: &SrcNodeId, costs: &HashMap<ClassId, NodeSet>) -> Option<NodeSet> {
        let node_id = self.inner.nodes.get_index_of(node).unwrap();
        let mut init = self.empty_node_set();
        init.add(node_id);
        self.inner.nodes[node]
            .children
            .iter()
            .map(|child| costs.get(self.inner.nid_to_cid(child)))
            .try_fold(init, |mut acc, child| {
                let child = child.as_ref()?;
                acc.union(child);
                Some(acc)
            })
    }

    fn extract(&self) -> ExtractionResult {
        let mut result = ExtractionResult::default();
        let mut costs = HashMap::<ClassId, NodeSet>::default();
        let mut did_something = false;
        loop {
            for (class_id, class) in self.inner.classes().iter() {
                for node in &class.nodes {
                    let new_cost = self.compute_cost(node, &costs);
                    match (costs.get(class_id), new_cost) {
                        (_, None) => {}
                        (None, Some(x)) => {
                            costs.insert(class_id.clone(), x);
                            result.choose(class.id.clone(), node.clone());
                            did_something = true;
                        }
                        (Some(cur), Some(new)) => {
                            if new.cost() < cur.cost() {
                                costs.insert(class_id.clone(), new);
                                result.choose(class.id.clone(), node.clone());
                                did_something = true;
                            }
                        }
                    }
                }
            }
            if did_something {
                did_something = false;
            } else {
                break;
            }
        }
        result
    }
}

/// The `_agg` APIs in val-trie are fairly low-level and easy to misuse.
/// `NodeSet` is a wrapper that exposes the minimal API required for this
/// algorithm to work.
#[derive(Clone)]
struct NodeSet {
    trie: val_trie::HashSet<NodeId, AddNotNan>,
    costs: Rc<Vec<NotNan<f64>>>,
}

impl NodeSet {
    fn add(&mut self, node: NodeId) {
        self.trie
            .insert_agg(node, |node| AddNotNan(self.costs[*node]));
    }

    fn union(&mut self, other: &Self) {
        self.trie
            .union_agg(&other.trie, |node| AddNotNan(self.costs[*node]));
    }

    fn cost(&self) -> NotNan<f64> {
        self.trie.agg().0
    }
}

#[derive(Default, Copy, Clone)]
struct AddNotNan(NotNan<f64>);
impl val_trie::Group for AddNotNan {
    fn add(&mut self, other: &Self) {
        self.0 += other.0;
    }

    fn inverse(&self) -> Self {
        AddNotNan(-self.0)
    }

    fn sub(&mut self, other: &Self) {
        self.0 -= other.0;
    }
}
