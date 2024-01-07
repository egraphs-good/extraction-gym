use indexmap::IndexMap;
use rustc_hash::{FxHashMap, FxHashSet};
use std::collections::HashMap;

pub use crate::*;

pub mod bottom_up;
pub mod faster_bottom_up;
pub mod faster_greedy_dag;
pub mod global_greedy_dag;
pub mod greedy_dag;
#[cfg(feature = "ilp-cbc")]
pub mod ilp_cbc;

// Allowance for floating point values to be considered equal
pub const EPSILON_ALLOWANCE: f64 = 0.00001;

pub trait Extractor: Sync {
    fn extract(&self, egraph: &EGraph, roots: &[ClassId]) -> ExtractionResult;

    fn boxed(self) -> Box<dyn Extractor>
    where
        Self: Sized + 'static,
    {
        Box::new(self)
    }
}

pub trait MapGet<K, V> {
    fn get(&self, key: &K) -> Option<&V>;
}

impl<K, V> MapGet<K, V> for HashMap<K, V>
where
    K: Eq + std::hash::Hash,
{
    fn get(&self, key: &K) -> Option<&V> {
        HashMap::get(self, key)
    }
}

impl<K, V> MapGet<K, V> for FxHashMap<K, V>
where
    K: Eq + std::hash::Hash,
{
    fn get(&self, key: &K) -> Option<&V> {
        FxHashMap::get(self, key)
    }
}

impl<K, V> MapGet<K, V> for IndexMap<K, V>
where
    K: Eq + std::hash::Hash,
{
    fn get(&self, key: &K) -> Option<&V> {
        IndexMap::get(self, key)
    }
}

#[derive(Default, Clone)]
pub struct ExtractionResult {
    pub choices: IndexMap<ClassId, NodeId>,
}

#[derive(Clone, Copy)]
enum Status {
    Doing,
    Done,
}

impl ExtractionResult {
    pub fn check(&self, egraph: &EGraph) {
        // should be a root
        assert!(!egraph.root_eclasses.is_empty());

        // All roots should be selected.
        for cid in egraph.root_eclasses.iter() {
            assert!(self.choices.contains_key(cid));
        }

        // No cycles
        assert!(self.find_cycles(&egraph, &egraph.root_eclasses).is_empty());

        // Nodes should match the class they are selected into.
        for (cid, nid) in &self.choices {
            let node = &egraph[nid];
            assert!(node.eclass == *cid);
        }

        // All the nodes the roots depend upon should be selected.
        let mut todo: Vec<ClassId> = egraph.root_eclasses.to_vec();
        let mut visited: FxHashSet<ClassId> = Default::default();
        while let Some(cid) = todo.pop() {
            if !visited.insert(cid.clone()) {
                continue;
            }
            assert!(self.choices.contains_key(&cid));

            for child in &egraph[&self.choices[&cid]].children {
                todo.push(egraph.nid_to_cid(child).clone());
            }
        }
    }

    pub fn choose(&mut self, class_id: ClassId, node_id: NodeId) {
        self.choices.insert(class_id, node_id);
    }

    pub fn find_cycles(&self, egraph: &EGraph, roots: &[ClassId]) -> Vec<ClassId> {
        // let mut status = vec![Status::Todo; egraph.classes().len()];
        let mut status = IndexMap::<ClassId, Status>::default();
        let mut cycles = vec![];
        for root in roots {
            // let root_index = egraph.classes().get_index_of(root).unwrap();
            self.cycle_dfs(egraph, root, &mut status, &mut cycles)
        }
        cycles
    }

    fn cycle_dfs(
        &self,
        egraph: &EGraph,
        class_id: &ClassId,
        status: &mut IndexMap<ClassId, Status>,
        cycles: &mut Vec<ClassId>,
    ) {
        match status.get(class_id).cloned() {
            Some(Status::Done) => (),
            Some(Status::Doing) => cycles.push(class_id.clone()),
            None => {
                status.insert(class_id.clone(), Status::Doing);
                let node_id = &self.choices[class_id];
                let node = &egraph[node_id];
                for child in &node.children {
                    let child_cid = egraph.nid_to_cid(child);
                    self.cycle_dfs(egraph, child_cid, status, cycles)
                }
                status.insert(class_id.clone(), Status::Done);
            }
        }
    }

    pub fn tree_cost(&self, egraph: &EGraph, roots: &[ClassId]) -> Cost {
        let node_roots = roots
            .iter()
            .map(|cid| self.choices[cid].clone())
            .collect::<Vec<NodeId>>();
        self.tree_cost_rec(egraph, &node_roots, &mut HashMap::new())
    }

    fn tree_cost_rec(
        &self,
        egraph: &EGraph,
        roots: &[NodeId],
        memo: &mut HashMap<NodeId, Cost>,
    ) -> Cost {
        let mut cost = Cost::default();
        for root in roots {
            if let Some(c) = memo.get(root) {
                cost += *c;
                continue;
            }
            let class = egraph.nid_to_cid(root);
            let node = &egraph[&self.choices[class]];
            let inner = node.cost + self.tree_cost_rec(egraph, &node.children, memo);
            memo.insert(root.clone(), inner);
            cost += inner;
        }
        cost
    }

    // this will loop if there are cycles
    pub fn dag_cost(&self, egraph: &EGraph, roots: &[ClassId]) -> Cost {
        let mut costs: IndexMap<ClassId, Cost> = IndexMap::new();
        let mut todo: Vec<ClassId> = roots.to_vec();
        while let Some(cid) = todo.pop() {
            let node_id = &self.choices[&cid];
            let node = &egraph[node_id];
            if costs.insert(cid.clone(), node.cost).is_some() {
                continue;
            }
            for child in &node.children {
                todo.push(egraph.nid_to_cid(child).clone());
            }
        }
        costs.values().sum()
    }

    pub fn node_sum_cost<M>(&self, egraph: &EGraph, node: &Node, costs: &M) -> Cost
    where
        M: MapGet<ClassId, Cost>,
    {
        node.cost
            + node
                .children
                .iter()
                .map(|n| {
                    let cid = egraph.nid_to_cid(n);
                    costs.get(cid).unwrap_or(&INFINITY)
                })
                .sum::<Cost>()
    }
}

use ordered_float::NotNan;
use rand::Rng;

// generates a float between 0 and 1
fn generate_random_not_nan() -> NotNan<f64> {
    let mut rng: rand::prelude::ThreadRng = rand::thread_rng();
    let random_float: f64 = rng.gen();
    NotNan::new(random_float).unwrap()
}

//make a random egraph that has a loop-free extraction.
pub fn generate_random_egraph() -> EGraph {
    let mut rng = rand::thread_rng();
    let core_node_count = rng.gen_range(1..100) as usize;
    let extra_node_count = rng.gen_range(1..100);
    let mut nodes: Vec<Node> = Vec::with_capacity(core_node_count + extra_node_count);
    let mut eclass = 0;

    let id2nid = |id: usize| -> NodeId { format!("node_{}", id).into() };

    // Unless we do it explicitly, the costs are almost never equal to others' costs or zero:
    let get_semi_random_cost = |nodes: &Vec<Node>| -> Cost {
        let mut rng = rand::thread_rng();

        if nodes.len() > 0 && rng.gen_bool(0.1) {
            return nodes[rng.gen_range(0..nodes.len())].cost;
        } else if rng.gen_bool(0.05) {
            return Cost::default();
        } else {
            return generate_random_not_nan() * 100.0;
        }
    };

    for i in 0..core_node_count {
        let children: Vec<NodeId> = (0..i).filter(|_| rng.gen_bool(0.1)).map(id2nid).collect();

        if rng.gen_bool(0.2) {
            eclass += 1;
        }

        nodes.push(Node {
            op: "operation".to_string(),
            children: children,
            eclass: eclass.to_string().clone().into(),
            cost: get_semi_random_cost(&nodes),
        });
    }

    // So far we have the nodes for a feasible egraph. Now we add some
    // cycles to extra nodes - nodes that aren't required in the extraction.
    for _ in 0..extra_node_count {
        nodes.push(Node {
            op: "operation".to_string(),
            children: vec![],
            eclass: rng.gen_range(0..eclass * 2 + 1).to_string().clone().into(),
            cost: get_semi_random_cost(&nodes),
        });
    }

    for i in core_node_count..nodes.len() {
        for j in 0..nodes.len() {
            if rng.gen_bool(0.05) {
                nodes.get_mut(i).unwrap().children.push(id2nid(j));
            }
        }
    }

    let mut egraph = EGraph::default();

    for i in 0..nodes.len() {
        egraph.add_node(id2nid(i), nodes[i].clone());
    }

    // Set roots
    for _ in 1..rng.gen_range(2..6) {
        egraph.root_eclasses.push(
            nodes
                .get(rng.gen_range(0..core_node_count))
                .unwrap()
                .eclass
                .clone(),
        );
    }

    egraph
}
