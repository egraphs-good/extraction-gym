use std::collections::HashMap;

pub use crate::*;

pub mod bottom_up;
pub mod greedy_dag;

#[cfg(feature = "ilp-cbc")]
pub mod ilp_cbc;

#[cfg(feature = "maxsat")]
pub mod maxsat;

pub trait Extractor: Sync {
    fn extract(&self, egraph: &EGraph, roots: &[ClassId]) -> ExtractionResult;

    fn boxed(self) -> Box<dyn Extractor>
    where
        Self: Sized + 'static,
    {
        Box::new(self)
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

    pub fn node_sum_cost(
        &self,
        egraph: &EGraph,
        node: &Node,
        costs: &IndexMap<ClassId, Cost>,
    ) -> Cost {
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
