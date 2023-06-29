pub use crate::*;

pub mod bottom_up;

#[cfg(feature = "ilp-cbc")]
pub mod ilp_cbc;

#[cfg(feature = "maxsat")]
pub mod maxsat;

pub trait Extractor: Sync {
    fn extract(&self, egraph: &SimpleEGraph, roots: &[Id]) -> ExtractionResult;

    fn boxed(self) -> Box<dyn Extractor>
    where
        Self: Sized + 'static,
    {
        Box::new(self)
    }
}

#[derive(Clone)]
pub struct ExtractionResult {
    pub choices: Vec<Id>,
}

impl ExtractionResult {
    pub fn new(n_classes: usize) -> Self {
        ExtractionResult {
            choices: vec![0; n_classes],
        }
    }

    pub fn tree_cost(&self, egraph: &SimpleEGraph, root: Id) -> Cost {
        let node = &egraph[root].nodes[self.choices[root]];
        let mut cost = node.cost;
        for &child in &node.children {
            cost += self.tree_cost(egraph, child);
        }
        cost
    }

    // this will loop if there are cycles
    pub fn dag_cost(&self, egraph: &SimpleEGraph, root: Id) -> Cost {
        let mut costs = vec![INFINITY; egraph.classes.len()];
        let mut todo = vec![root];
        while !todo.is_empty() {
            let i = todo.pop().unwrap();
            let node = &egraph[i].nodes[self.choices[i]];
            costs[i] = node.cost;
            for &child in &node.children {
                todo.push(child);
            }
        }
        costs.iter().filter(|c| **c != INFINITY).sum()
    }

    pub fn node_sum_cost(&self, node: &Node, costs: &[Cost]) -> Cost {
        node.cost + node.children.iter().map(|&i| costs[i]).sum::<Cost>()
    }
}
