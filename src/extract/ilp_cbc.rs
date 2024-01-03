/* An ILP extractor that returns the optimal DAG-extraction.

This extractor is simple so that it's easy to see that it's correct.

If the timeout is reached, it will return the result of the faster-greedy-dag extractor.
*/

use super::*;
use coin_cbc::{Col, Model, Sense};
use indexmap::IndexSet;

struct ClassVars {
    active: Col,
    nodes: Vec<Col>,
}

pub struct CbcExtractorWithTimeout<const TIMEOUT_IN_SECONDS: u32>;

impl<const TIMEOUT_IN_SECONDS: u32> Extractor for CbcExtractorWithTimeout<TIMEOUT_IN_SECONDS> {
    fn extract(&self, egraph: &EGraph, roots: &[ClassId]) -> ExtractionResult {
        return extract(egraph, roots, TIMEOUT_IN_SECONDS);
    }
}

pub struct CbcExtractor;

impl Extractor for CbcExtractor {
    fn extract(&self, egraph: &EGraph, roots: &[ClassId]) -> ExtractionResult {
        return extract(egraph, roots, std::u32::MAX);
    }
}

fn extract(egraph: &EGraph, roots: &[ClassId], timeout_seconds: u32) -> ExtractionResult {
    let mut model = Model::default();

    model.set_parameter("seconds", &timeout_seconds.to_string());

    let vars: IndexMap<ClassId, ClassVars> = egraph
        .classes()
        .values()
        .map(|class| {
            let cvars = ClassVars {
                active: model.add_binary(),
                nodes: class.nodes.iter().map(|_| model.add_binary()).collect(),
            };
            (class.id.clone(), cvars)
        })
        .collect();

    for (class_id, class) in &vars {
        // class active == some node active
        // sum(for node_active in class) == class_active
        let row = model.add_row();
        model.set_row_equal(row, 0.0);
        model.set_weight(row, class.active, -1.0);
        for &node_active in &class.nodes {
            model.set_weight(row, node_active, 1.0);
        }

        let childrens_classes_var = |nid: NodeId| {
            egraph[&nid]
                .children
                .iter()
                .map(|n| egraph[n].eclass.clone())
                .map(|n| vars[&n].active)
                .collect::<IndexSet<_>>()
        };

        for (node_id, &node_active) in egraph[class_id].nodes.iter().zip(&class.nodes) {
            for child_active in childrens_classes_var(node_id.clone()) {
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
    for class in egraph.classes().values() {
        for (node_id, &node_active) in class.nodes.iter().zip(&vars[&class.id].nodes) {
            let node = &egraph[node_id];
            let node_cost = node.cost.into_inner();
            assert!(node_cost >= 0.0);

            if node_cost != 0.0 {
                model.set_obj_coeff(node_active, node_cost);
            }
        }
    }

    for root in roots {
        model.set_col_lower(vars[root].active, 1.0);
    }

    block_cycles(&mut model, &vars, &egraph);

    let solution = model.solve();
    log::info!(
        "CBC status {:?}, {:?}, obj = {}",
        solution.raw().status(),
        solution.raw().secondary_status(),
        solution.raw().obj_value(),
    );

    if solution.raw().status() != coin_cbc::raw::Status::Finished {
        assert!(timeout_seconds != std::u32::MAX);

        let initial_result =
            super::faster_greedy_dag::FasterGreedyDagExtractor.extract(egraph, roots);
        log::info!("Unfinished CBC solution");
        return initial_result;
    }

    let mut result = ExtractionResult::default();

    for (id, var) in &vars {
        let active = solution.col(var.active) > 0.0;
        if active {
            let node_idx = var
                .nodes
                .iter()
                .position(|&n| solution.col(n) > 0.0)
                .unwrap();
            let node_id = egraph[id].nodes[node_idx].clone();
            result.choose(id.clone(), node_id);
        }
    }

    return result;
}

/*

 To block cycles, we enforce that a topological ordering exists on the extraction.
 Each class is mapped to a variable (called its level).  Then for each node,
 we add a constraint that if a node is active, then the level of the class the node
 belongs to must be less than than the level of each of the node's children.

 To create a cycle, the levels would need to decrease, so they're blocked. For example,
 given a two class cycle: if class A, has level 'l', and class B has level 'm', then
 'l' must be less than 'm', but because there is also an active node in class B that
 has class A as a child, 'm' must be less than 'l', which is a contradiction.
*/

fn block_cycles(model: &mut Model, vars: &IndexMap<ClassId, ClassVars>, egraph: &EGraph) {
    let mut levels: IndexMap<ClassId, Col> = Default::default();
    for c in vars.keys() {
        let var = model.add_col();
        levels.insert(c.clone(), var);
        //model.set_col_lower(var, 0.0);
        // It solves the benchmarks about 5% faster without this
        //model.set_col_upper(var, vars.len() as f64);
    }

    // If n.variable is true, opposite_col will be false and vice versa.
    let mut opposite: IndexMap<Col, Col> = Default::default();
    for c in vars.values() {
        for n in &c.nodes {
            let opposite_col = model.add_binary();
            opposite.insert(*n, opposite_col);
            let row = model.add_row();
            model.set_row_equal(row, 1.0);
            model.set_weight(row, opposite_col, 1.0);
            model.set_weight(row, *n, 1.0);
        }
    }

    for (class_id, c) in vars {
        for i in 0..c.nodes.len() {
            let n_id = &egraph[class_id].nodes[i];
            let n = &egraph[n_id];
            let var = c.nodes[i];

            let children_classes = n
                .children
                .iter()
                .map(|n| egraph[n].eclass.clone())
                .collect::<IndexSet<_>>();

            if children_classes.contains(class_id) {
                // Self loop - disable this node.
                // This is clumsier than calling set_col_lower(var,0.0),
                // but means it'll be infeasible (rather than producing an
                // incorrect solution) if var corresponds to a root node.
                let row = model.add_row();
                model.set_weight(row, var, 1.0);
                model.set_row_equal(row, 0.0);
                continue;
            }

            for cc in children_classes {
                assert!(*levels.get(class_id).unwrap() != *levels.get(&cc).unwrap());

                let row = model.add_row();
                model.set_row_lower(row, 1.0);
                model.set_weight(row, *levels.get(class_id).unwrap(), -1.0);
                model.set_weight(row, *levels.get(&cc).unwrap(), 1.0);

                // If n.variable is 0, then disable the contraint.
                model.set_weight(row, *opposite.get(&var).unwrap(), (vars.len() + 1) as f64);
            }
        }
    }
}
