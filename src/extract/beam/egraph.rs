use ordered_float::NotNan;
use std::{fmt::Debug, hash::Hash, ops::Range};

use crate::{Cost, INFINITY};

pub trait UInt: Copy + Ord + TryInto<usize> + TryFrom<usize> + Hash + Debug
where
    <Self as TryInto<usize>>::Error: Debug,
    <Self as TryFrom<usize>>::Error: Debug,
    Range<Self>: Iterator<Item = Self> + ExactSizeIterator + DoubleEndedIterator + Clone + Debug,
{
}

impl UInt for u16 {}
impl UInt for u32 {}
impl UInt for usize {}

/// A compact representation of an e-graph for extraction purposes.
/// This representation uses contiguous arrays to store the e-classes and nodes,
/// allowing for efficient access and traversal.
///
/// # Type Parameters
///
/// - `U`: The unsigned integer type used for indexing (e.g., `u16`, `u32`, `usize`).
/// - `C`: The type of foreign class key associated with each e-class.
/// - `N`: The type of foreign node key associated with each node.
/// - `M`: The type of memoization data associated with each e-class.
///
#[derive(Clone, Debug)]
pub struct FastEgraph<U, C, N, M> {
    class_ids: Vec<C>,
    memo: Vec<M>,
    min_cost: Vec<Cost>,
    nodes_start: Vec<NodeId<U>>,

    node_ids: Vec<N>,
    node_cost: Vec<NotNan<f64>>,

    children_start: Vec<U>,
    children: Vec<ClassId<U>>,

    parents_start: Vec<U>,
    parents: Vec<NodeId<U>>,
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct NodeId<U>(U);

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct ClassId<U>(U);

impl<U: UInt, C, N, M> FastEgraph<U, C, N, M>
where
    <U as TryInto<usize>>::Error: Debug,
    <U as TryFrom<usize>>::Error: Debug,
    Range<U>: Iterator<Item = U> + ExactSizeIterator + DoubleEndedIterator + Clone + Debug,
{
    pub fn class_id(&self, class: ClassId<U>) -> &C {
        let class: usize = class.0.try_into().unwrap();
        &self.class_ids[class]
    }

    pub fn node_id(&self, node: NodeId<U>) -> &N {
        let node: usize = node.0.try_into().unwrap();
        &self.node_ids[node]
    }

    pub fn memo(&self, class: ClassId<U>) -> &M {
        let class: usize = class.0.try_into().unwrap();
        &self.memo[class]
    }

    pub fn memo_mut(&mut self, class: ClassId<U>) -> &mut M {
        let class: usize = class.0.try_into().unwrap();
        &mut self.memo[class]
    }

    pub fn from_class_id(&self, class: &C) -> Option<ClassId<U>>
    where
        C: PartialEq,
    {
        self.class_ids
            .iter()
            .position(|c| c == class)
            .map(|idx| ClassId(U::try_from(idx).unwrap()))
    }

    pub fn classes(&self) -> impl Iterator<Item = ClassId<U>> {
        let start = 0_usize.try_into().unwrap();
        let end = self.class_ids.len().try_into().unwrap();
        (start..end).map(ClassId)
    }

    pub fn all_nodes(&self) -> impl Iterator<Item = NodeId<U>> {
        let start = 0_usize.try_into().unwrap();
        let end = self.node_ids.len().try_into().unwrap();
        (start..end).map(NodeId)
    }

    pub fn node_class(&self, node: NodeId<U>) -> ClassId<U> {
        let node: usize = node.0.try_into().unwrap();
        let class = self
            .nodes_start
            .binary_search(&NodeId(U::try_from(node).unwrap()))
            .unwrap_or_else(|x| x - 1);
        ClassId(U::try_from(class).unwrap())
    }

    pub fn min_cost(&self, class: ClassId<U>) -> Cost {
        let class: usize = class.0.try_into().unwrap();
        self.min_cost[class]
    }

    pub fn nodes(&self, class: ClassId<U>) -> impl Iterator<Item = NodeId<U>> {
        let class: usize = class.0.try_into().unwrap();
        let start = self.nodes_start[class].0;
        let end = self.nodes_start[class + 1].0;
        (start..end).map(NodeId)
    }

    pub fn cost(&self, node: NodeId<U>) -> NotNan<f64> {
        let node: usize = node.0.try_into().unwrap();
        self.node_cost[node]
    }

    pub fn children(&self, node: NodeId<U>) -> &[ClassId<U>] {
        let node: usize = node.0.try_into().unwrap();
        let start = self.children_start[node].try_into().unwrap();
        let end = self.children_start[node + 1].try_into().unwrap();
        &self.children[start..end]
    }

    pub fn parents(&self, class: ClassId<U>) -> &[NodeId<U>] {
        let class: usize = class.0.try_into().unwrap();
        debug_assert!(class < self.parents_start.len() - 1);
        let start = self.parents_start[class].try_into().unwrap();
        let end = self.parents_start[class + 1].try_into().unwrap();
        &self.parents[start..end]
    }
}

impl<U: UInt, M> TryFrom<&egraph_serialize::EGraph>
    for FastEgraph<U, egraph_serialize::ClassId, egraph_serialize::NodeId, M>
where
    M: Default + Clone,
    <U as TryInto<usize>>::Error: Debug,
    <U as TryFrom<usize>>::Error: Debug,
    Range<U>: Iterator<Item = U> + ExactSizeIterator + DoubleEndedIterator + Clone + Debug,
{
    type Error = Box<dyn std::error::Error>;

    fn try_from(egraph: &egraph_serialize::EGraph) -> Result<Self, Self::Error> {
        use std::collections::HashMap;

        let num_classes: usize = egraph.classes().len();
        let num_nodes: usize = egraph.nodes.len();
        let num_total_children = egraph
            .nodes
            .values()
            .map(|n| n.children.len())
            .sum::<usize>();
        // Total parents will be the same as total children

        // Check if U can hold the sizes
        if U::try_from(num_classes + 10).is_err()
            || U::try_from(num_nodes + 10).is_err()
            || U::try_from(num_total_children + 10).is_err()
        {
            return Err(format!("Type U is too small to hold the e-graph data").into());
        }

        let mut result = Self {
            class_ids: Vec::with_capacity(num_classes),
            memo: vec![M::default(); num_classes],
            min_cost: Vec::with_capacity(num_classes),
            nodes_start: Vec::with_capacity(num_classes + 1),
            node_ids: Vec::with_capacity(num_nodes),
            node_cost: Vec::with_capacity(num_nodes),
            children_start: Vec::with_capacity(num_nodes + 1),
            children: Vec::with_capacity(num_total_children),
            parents_start: Vec::with_capacity(num_nodes + 1),
            parents: Vec::with_capacity(num_total_children),
        };

        let mut class_map: HashMap<egraph_serialize::ClassId, ClassId<U>> = HashMap::new();
        for cid in egraph.classes().keys() {
            result.class_ids.push(cid.clone());
            class_map.insert(
                cid.clone(),
                ClassId(U::try_from(result.class_ids.len() - 1).unwrap()),
            );
        }

        for class in egraph.classes().values() {
            result
                .nodes_start
                .push(NodeId(U::try_from(result.node_ids.len()).unwrap()));
            for nid in &class.nodes {
                let node = &egraph[nid];

                // Map children to classes and deduplicate
                // (For DAG extraction we only care about the set)
                let mut children: Vec<ClassId<U>> = node
                    .children
                    .iter()
                    .map(|child_nid| class_map[&egraph[child_nid].eclass])
                    .collect();
                children.sort();
                children.dedup();

                // TODO: We can skip nodes that have self-cycles.

                result.node_ids.push(nid.clone());
                result.node_cost.push(node.cost);
                result
                    .children_start
                    .push(U::try_from(result.children.len()).unwrap());
                result.children.extend(children);
            }
        }
        result
            .nodes_start
            .push(NodeId(U::try_from(result.node_ids.len()).unwrap()));
        result
            .children_start
            .push(U::try_from(result.children.len()).unwrap());

        // Compute min costs
        for class in result.classes() {
            let min_cost = result
                .nodes(class)
                .map(|nid| result.cost(nid))
                .min()
                .unwrap_or(INFINITY);
            result.min_cost.push(min_cost);
        }

        // Compute parents
        let mut parents_map = vec![Vec::new(); num_classes];
        for nid in result.all_nodes() {
            for &child in result.children(nid) {
                parents_map[child.0.try_into().unwrap()].push(nid);
            }
        }
        for mut parents in parents_map {
            parents.sort();
            parents.dedup();
            result
                .parents_start
                .push(U::try_from(result.parents.len()).unwrap());
            result.parents.extend(parents);
        }
        result
            .parents_start
            .push(U::try_from(result.parents.len()).unwrap());

        Ok(result)
    }
}
