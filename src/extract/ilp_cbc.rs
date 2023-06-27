use super::*;
use coin_cbc::{Col, Model, Sense};
use indexmap::IndexSet;

struct ClassVars {
    active: Col,
    order: Col,
    nodes: Vec<Col>,
}

pub struct CbcExtractor;

impl Extractor for CbcExtractor {
    fn extract(&self, egraph: &SimpleEGraph, roots: &[Id]) -> ExtractionResult {
        let max_order = egraph.total_number_of_nodes() as f64 * 10.0;

        let mut model = Model::default();

        let vars: IndexMap<Id, ClassVars> = egraph
            .classes
            .values()
            .map(|class| {
                let cvars = ClassVars {
                    active: model.add_binary(),
                    order: model.add_col(),
                    nodes: class.nodes.iter().map(|_| model.add_binary()).collect(),
                };
                model.set_col_upper(cvars.order, max_order);
                (class.id, cvars)
            })
            .collect();

        let mut cycles: IndexSet<(Id, usize)> = Default::default();
        find_cycles(egraph, |id, i| {
            cycles.insert((id, i));
        });

        for (&id, class) in &vars {
            // class active == some node active
            // sum(for node_active in class) == class_active
            let row = model.add_row();
            model.set_row_equal(row, 0.0);
            model.set_weight(row, class.active, -1.0);
            for &node_active in &class.nodes {
                model.set_weight(row, node_active, 1.0);
            }

            for (i, (node, &node_active)) in egraph[id].nodes.iter().zip(&class.nodes).enumerate() {
                if cycles.contains(&(id, i)) {
                    model.set_col_upper(node_active, 0.0);
                    model.set_col_lower(node_active, 0.0);
                    continue;
                }

                for child in &node.children {
                    let child_active = vars[child].active;
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

        model.set_obj_sense(Sense::Minimize);
        for class in egraph.classes.values() {
            for (node, &node_active) in class.nodes.iter().zip(&vars[&class.id].nodes) {
                model.set_obj_coeff(node_active, node.cost.into_inner());
            }
        }

        // model is now ready to go, time to solve
        dbg!(max_order);

        for class in vars.values() {
            model.set_binary(class.active);
        }

        for root in roots {
            model.set_col_lower(vars[root].active, 1.0);
        }

        let solution = model.solve();
        log::info!(
            "CBC status {:?}, {:?}",
            solution.raw().status(),
            solution.raw().secondary_status()
        );

        let mut result = ExtractionResult::new(egraph.classes.len());

        for (id, var) in vars {
            let active = solution.col(var.active) > 0.0;
            if active {
                let node_idx = var
                    .nodes
                    .iter()
                    .position(|&n| solution.col(n) > 0.0)
                    .unwrap();
                result.choices[id] = node_idx;
            }
        }

        result
    }
}

// from @khaki3
// fixes bug in egg 0.9.4's version
// https://github.com/egraphs-good/egg/issues/207#issuecomment-1264737441
fn find_cycles(egraph: &SimpleEGraph, mut f: impl FnMut(Id, usize)) {
    let mut pending: IndexMap<Id, Vec<(Id, usize)>> = IndexMap::default();

    let mut order: IndexMap<Id, usize> = IndexMap::default();

    let mut memo: IndexMap<(Id, usize), bool> = IndexMap::default();

    let mut stack: Vec<(Id, usize)> = vec![];

    for class in egraph.classes.values() {
        let id = class.id;
        for (i, node) in egraph[id].nodes.iter().enumerate() {
            for &child in &node.children {
                pending.entry(child).or_insert_with(Vec::new).push((id, i));
            }

            if node.is_leaf() {
                stack.push((id, i));
            }
        }
    }

    let mut count = 0;

    while let Some((id, i)) = stack.pop() {
        if memo.get(&(id, i)).is_some() {
            continue;
        }

        let node = &egraph[id].nodes[i];
        let mut update = false;

        if node.is_leaf() {
            update = true;
        } else if node.children.iter().all(|&x| order.get(&x).is_some()) {
            if let Some(ord) = order.get(&id) {
                update = node.children.iter().all(|&x| order.get(&x).unwrap() < ord);
                if !update {
                    memo.insert((id, i), false);
                    continue;
                }
            } else {
                update = true;
            }
        }

        if update {
            if order.get(&id).is_none() {
                order.insert(id, count);
                count += 1;
            }
            memo.insert((id, i), true);
            if let Some(mut v) = pending.remove(&id) {
                stack.append(&mut v);
                stack.sort();
                stack.dedup();
            };
        }
    }

    for class in egraph.classes.values() {
        let id = class.id;
        for (i, _node) in egraph[id].nodes.iter().enumerate() {
            if let Some(true) = memo.get(&(id, i)) {
                continue;
            }
            f(id, i);
        }
    }
}
