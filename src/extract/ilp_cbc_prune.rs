use super::*;
use coin_cbc::{Col, Model, Sense};
use indexmap::IndexSet;
use ordered_float::NotNan;

const BAN_ABOVE_COST: Cost = unsafe { NotNan::new_unchecked(1000.0) };

struct ClassVars {
    active: Col,
    order: Col,
    nodes: Vec<Option<Col>>, // some nodes are pruned
}

pub struct CbcPruneExtractor;

impl Extractor for CbcPruneExtractor {
    fn extract(&self, egraph: &EGraph, roots: &[ClassId]) -> ExtractionResult {
        let max_order = egraph.nodes.len() as f64 * 10.0;

        let mut to_prune: IndexSet<(ClassId, usize)> = Default::default();
        find_nodes_to_prune(egraph, |id, i| {
            to_prune.insert((id, i));
        });

        let mut model = Model::default();
        model.set_parameter("seconds", "30");

        let vars: IndexMap<ClassId, ClassVars> = egraph
            .classes()
            .values()
            .map(|class| {
                let cvars = ClassVars {
                    active: model.add_binary(),
                    order: model.add_col(),
                    nodes: class
                        .nodes
                        .iter()
                        .enumerate()
                        .map(|(i, _)| {
                            if to_prune.contains(&(class.id.clone(), i)) {
                                None
                            } else {
                                Some(model.add_binary())
                            }
                        })
                        .collect(),
                };
                model.set_col_upper(cvars.order, max_order);
                (class.id.clone(), cvars)
            })
            .collect();

        for (id, class) in &vars {
            let row = model.add_row();
            model.set_row_equal(row, 0.0);
            model.set_weight(row, class.active, -1.0);
            for &node_active in class.nodes.iter().flatten() {
                // only set weight for non-pruned e-nodes
                model.set_weight(row, node_active, 1.0);
            }

            for (node_id, &node_active_opt) in egraph[id].nodes.iter().zip(&class.nodes) {
                if let Some(node_active) = node_active_opt {
                    let node = &egraph[node_id];
                    for child in &node.children {
                        let eclass_id = &egraph[child].eclass;
                        let child_active = vars[eclass_id].active;
                        // node active implies child active, encoded as:
                        //   node_active <= child_active
                        //   node_active - child_active <= 0
                        let row = model.add_row();
                        model.set_row_upper(row, 0.0);
                        model.set_weight(row, node_active, 1.0);
                        model.set_weight(row, child_active, -1.0);
                    }
                }
            }
        }

        model.set_obj_sense(Sense::Minimize);
        for class in egraph.classes().values() {
            for (node_id, &node_active_opt) in class.nodes.iter().zip(&vars[&class.id].nodes) {
                if let Some(node_active) = node_active_opt {
                    let node = &egraph[node_id];
                    model.set_obj_coeff(node_active, node.cost.into_inner());
                }
            }
        }

        dbg!(max_order);

        for class in vars.values() {
            model.set_binary(class.active);
        }

        for root in roots {
            // let root = &egraph.find(*root);
            model.set_col_lower(vars[root].active, 1.0);
        }

        // set initial solution based on bottom up extractor
        let initial_result = super::bottom_up::BottomUpExtractor.extract(egraph, roots);
        /* FIXME: would need to keep ILP variables for pruned cycle nodes, only removing the cost pruned ones.
        for (class, class_vars) in egraph.classes().values().zip(vars.values()) {
            if let Some(node_id) = initial_result.choices.get(&class.id) {
                model.set_col_initial_solution(class_vars.active, 1.0);
                for col in class_vars.nodes.iter().flatten() {
                    model.set_col_initial_solution(*col, 0.0);
                }
                let node_idx = class.nodes.iter().position(|n| n == node_id).unwrap();
                if to_prune.contains(&(class.id.clone(), node_idx)) {
                    println!("WARNING: infeasible initial solution, returning it anyway");
                    return initial_result;
                }
                model.set_col_initial_solution(class_vars.nodes[node_idx].unwrap(), 1.0);
            } else {
                model.set_col_initial_solution(class_vars.active, 0.0);
            }
        } */

        let solution = model.solve();
        log::info!(
            "CBC status {:?}, {:?}, obj = {}",
            solution.raw().status(),
            solution.raw().secondary_status(),
            solution.raw().obj_value(),
        );
        if solution.raw().is_proven_infeasible()
            || solution.raw().status() != coin_cbc::raw::Status::Finished
        {
            println!("WARNING: no solution found, returning bottom up solution.");
            return initial_result;
        }

        let mut result = ExtractionResult::default();
        for (id, var) in &vars {
            let active = solution.col(var.active) > 0.0;
            if active {
                let node_idx = var
                    .nodes
                    .iter()
                    .position(|&n_opt| n_opt.map(|n| solution.col(n) > 0.0).unwrap_or(false))
                    .unwrap();
                let node_id = egraph[id].nodes[node_idx].clone();
                result.choose(id.clone(), node_id);
            }
        }

        return result;
    }
}

// does not use @khaki3's fix
// https://github.com/egraphs-good/egg/issues/207#issuecomment-1264737441
fn find_nodes_to_prune(egraph: &EGraph, mut f: impl FnMut(ClassId, usize)) {
    enum Color {
        White,
        Gray,
        Black,
    }
    type Enter = bool;

    let mut color: HashMap<ClassId, Color> = egraph
        .classes()
        .values()
        .map(|c| (c.id.clone(), Color::White))
        .collect();
    let mut stack: Vec<(Enter, ClassId)> = egraph
        .classes()
        .values()
        .map(|c| (true, c.id.clone()))
        .collect();

    let n2c = |nid: &NodeId| egraph.nid_to_cid(nid);

    while let Some((enter, id)) = stack.pop() {
        if enter {
            *color.get_mut(&id).unwrap() = Color::Gray;
            stack.push((false, id.clone()));
            for (i, node_id) in egraph[&id].nodes.iter().enumerate() {
                let node = &egraph[node_id];
                if node.cost >= BAN_ABOVE_COST {
                    f(id.clone(), i);
                    continue;
                }
                for child in &node.children {
                    let child = n2c(child);
                    match &color[&child] {
                        Color::White => stack.push((true, child.clone())),
                        Color::Gray => f(id.clone(), i),
                        Color::Black => (),
                    }
                }
            }
        } else {
            *color.get_mut(&id).unwrap() = Color::Black;
        }
    }
}
