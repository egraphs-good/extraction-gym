pub use crate::*;

pub mod bottom_up;

#[cfg(feature = "ilp-cbc")]
pub mod ilp_cbc;

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

#[derive(Clone, Copy)]
enum Status {
    Todo,
    Doing,
    Done,
}

impl ExtractionResult {
    pub fn new(n_classes: usize) -> Self {
        ExtractionResult {
            choices: vec![usize::MAX; n_classes],
        }
    }

    pub fn find_cycles(&self, egraph: &SimpleEGraph, roots: &[Id]) -> Vec<Id> {
        let mut status = vec![Status::Todo; egraph.classes.len()];
        let mut cycles = vec![];
        for root in roots {
            self.cycle_dfs(egraph, *root, &mut status, &mut cycles)
        }
        cycles
    }

    fn cycle_dfs(
        &self,
        egraph: &SimpleEGraph,
        id: Id,
        status: &mut [Status],
        cycles: &mut Vec<Id>,
    ) {
        match status[id] {
            Status::Done => (),
            Status::Doing => cycles.push(id),
            Status::Todo => {
                status[id] = Status::Doing;
                let node = &egraph[id].nodes[self.choices[id]];
                for &child in &node.children {
                    self.cycle_dfs(egraph, child, status, cycles)
                }
                status[id] = Status::Done;
            }
        }
    }

    pub fn tree_cost(&self, egraph: &SimpleEGraph, roots: &[Id]) -> Cost {
        let mut cost = Cost::default();
        for &root in roots {
            let node = &egraph[root].nodes[self.choices[root]];
            cost += node.cost;
            cost += self.tree_cost(egraph, &node.children);
        }
        cost
    }

    // this will loop if there are cycles
    pub fn dag_cost(&self, egraph: &SimpleEGraph, roots: &[Id]) -> Cost {
        let mut costs = vec![None; egraph.classes.len()];
        let mut todo = roots.to_owned();
        while !todo.is_empty() {
            let i = todo.pop().unwrap();
            let node = &egraph[i].nodes[self.choices[i]];
            costs[i] = Some(node.cost);
            for &child in &node.children {
                todo.push(child);
            }
        }
        costs.iter().filter_map(|c| *c).sum()
    }

    pub fn node_sum_cost(&self, node: &Node, costs: &[Cost]) -> Cost {
        node.cost + node.children.iter().map(|&i| costs[i]).sum::<Cost>()
    }
}
