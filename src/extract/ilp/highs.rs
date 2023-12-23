use good_lp::{*, solvers::highs::*};
use indexmap::IndexSet;

use super::*;

struct ClassVars {
    active: Variable,
    nodes: Vec<Variable>,
}

pub struct HighsExtractor;

impl Extractor for HighsExtractor {
    fn extract(&self, egraph: &egraph_serialize::EGraph, roots: &[ClassId]) -> ExtractionResult {
        let mut problem_vars = ProblemVariables::new();
        let mut constraints = Vec::new();

        let true_literal = problem_vars.add(variable().binary());
        constraints.push(constraint!(true_literal == 1.0));

        let vars: IndexMap<ClassId, ClassVars> = egraph
            .classes()
            .values()
            .map(|class| {
                let cvars = ClassVars {
                    active: if roots.contains(&class.id) {
                        // Roots must be active.
                        true_literal
                    } else {
                        problem_vars.add(variable().binary())
                    },
                    nodes: class
                        .nodes
                        .iter()
                        .map(|_| problem_vars.add(variable().binary()))
                        .collect(),
                };
                (class.id.clone(), cvars)
            })
            .collect();

        for (class_id, class) in &vars {
            // class active == some node active
            // sum(for node_active in class) == class_active
            let mut row = -class.active;
            for &node_active in &class.nodes {
                row += node_active;
            }
            constraints.push(constraint!(row == 0.0));

            let childrens_classes_var = |nid: NodeId| {
                egraph[&nid]
                    .children
                    .iter()
                    .map(|n| egraph[n].eclass.clone())
                    .map(|n| vars[&n].active)
                    .collect::<IndexSet<_>>()
            };

            let mut intersection: IndexSet<_> =
                childrens_classes_var(egraph[class_id].nodes[0].clone());

            for node in &egraph[class_id].nodes[1..] {
                intersection = intersection
                    .intersection(&childrens_classes_var(node.clone()))
                    .copied()
                    .collect();
            }

            // A class being active implies that all in the intersection
            // of it's children are too.
            for c in &intersection {
                let row = class.active - *c;
                constraints.push(constraint!(row <= 0.0));
            }

            for (node_id, &node_active) in egraph[class_id].nodes.iter().zip(&class.nodes) {
                for child_active in childrens_classes_var(node_id.clone()) {
                    // node active implies child active, encoded as:
                    //   node_active <= child_active
                    //   node_active - child_active <= 0
                    if !intersection.contains(&child_active) {
                        let row = node_active - child_active;
                        constraints.push(constraint!(row <= 0.0));
                    }
                }
            }
        }

        let mut total_cost = Expression::from(0);

        for class in egraph.classes().values() {
            let min_cost = class
                .nodes
                .iter()
                .map(|n_id| egraph[n_id].cost)
                .min()
                .unwrap_or_else(Cost::default)
                .into_inner();

            // Most helpful when the members of the class all have the same cost.
            // For example if the members' costs are [1,1,1], three terms get
            // replaced by one in the objective function.
            if min_cost != 0.0 {
                total_cost += vars[&class.id].active * min_cost;
            }

            for (node_id, &node_active) in class.nodes.iter().zip(&vars[&class.id].nodes) {
                let node = &egraph[node_id];
                let node_cost = node.cost.into_inner() - min_cost;
                assert!(node_cost >= 0.0);

                if node_cost != 0.0 {
                    total_cost += node_active * node_cost;
                }
            }
        }

        let mut banned_cycles: IndexSet<(ClassId, usize)> = IndexSet::default();
        find_cycles(egraph, |id, i| {
            banned_cycles.insert((id, i));
        });
        for (class_id, class_vars) in &vars {
            for (i, &node_active) in class_vars.nodes.iter().enumerate() {
                if banned_cycles.contains(&(class_id.clone(), i)) {
                    constraints.push(constraint!(node_active == 0.0));
                }
            }
        }
        log::info!("@blocked {}", banned_cycles.len());

        let problem = constraints.into_iter().fold(
            problem_vars
                .minimise(total_cost)
                .using(good_lp::default_solver),
            |acc, constraint| acc.with(constraint),
        );
        let mut problem = problem.set_parallel(HighsParallelType::On);
        problem.set_verbose(true);

        let solution = problem.solve().unwrap();

        let mut result = ExtractionResult::default();

        for (id, var) in &vars {
            let active = (solution.value(var.active) - 1.0).abs() < 0.1;
            if active {
                let node_idx = var
                    .nodes
                    .iter()
                    .position(|&n| (solution.value(n) - 1.0).abs() < 0.1)
                    .unwrap();
                let node_id = egraph[id].nodes[node_idx].clone();
                result.choose(id.clone(), node_id);
            }
        }

        let cycles = result.find_cycles(egraph, roots);
        assert!(cycles.is_empty());
        result
    }
}
