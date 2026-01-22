use crate::{extractors, Extractor, Optimal, EPSILON_ALLOWANCE};
pub type Cost = NotNan<f64>;
use egraph_serialize::{EGraph, Node, NodeId};
use ordered_float::NotNan;
use rand::Rng;

// I want this to write to a tempfs file system, you'll
// want to change the path in test_save_path to something
// that works for you.
pub const ELABORATE_TESTING: bool = false;

pub fn test_save_path(name: &str) -> String {
    if ELABORATE_TESTING {
        format!("/dev/shm/{}_egraph.json", name)
    } else {
        "".to_string()
    }
}

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

        if !nodes.is_empty() && rng.gen_bool(0.1) {
            nodes[rng.gen_range(0..nodes.len())].cost
        } else if rng.gen_bool(0.05) {
            Cost::default()
        } else {
            generate_random_not_nan() * 100.0
        }
    };

    for i in 0..core_node_count {
        let children: Vec<NodeId> = (0..i).filter(|_| rng.gen_bool(0.1)).map(id2nid).collect();

        if rng.gen_bool(0.2) {
            eclass += 1;
        }

        nodes.push(Node {
            op: "operation".to_string(),
            children,
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

    for (i, node) in nodes.iter().enumerate() {
        egraph.add_node(id2nid(i), node.clone());
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

/*
 * Checks that no extractors produce better results than the extractors that produce optimal results.
 * Checks that the extractions are valid.
 */

#[cfg(feature = "ilp-cbc")]
fn check_dag_optimal(
    egraph: &EGraph,
    optimal_dag: &[Box<dyn Extractor>],
    others: &[Box<dyn Extractor>],
    optimal_tree_cost: Option<Cost>,
) {
    let mut optimal_dag_cost: Option<Cost> = None;

    for e in optimal_dag {
        let extract = e.extract(egraph, &egraph.root_eclasses);
        extract.check(egraph);
        let dag_cost = extract.dag_cost(egraph, &egraph.root_eclasses);
        let tree_cost = extract.tree_cost(egraph, &egraph.root_eclasses);
        if optimal_dag_cost.is_none() {
            optimal_dag_cost = Some(dag_cost);
            continue;
        }

        assert!(
            (dag_cost.into_inner() - optimal_dag_cost.unwrap().into_inner()).abs()
                < EPSILON_ALLOWANCE
        );

        assert!(
            tree_cost.into_inner() + EPSILON_ALLOWANCE > optimal_dag_cost.unwrap().into_inner()
        );
    }

    if let (Some(dag_cost), Some(tree_cost)) = (optimal_dag_cost, optimal_tree_cost) {
        assert!(dag_cost < tree_cost + EPSILON_ALLOWANCE);
    }

    if let Some(optimal_dag_cost) = optimal_dag_cost {
        for e in others {
            let extract = e.extract(egraph, &egraph.root_eclasses);
            let dag_cost = extract.dag_cost(egraph, &egraph.root_eclasses);
            // The optimal dag should be <= any extractor's dag cost
            assert!(optimal_dag_cost <= dag_cost + EPSILON_ALLOWANCE);
        }
    }
}

fn check_optimal_results<I: Iterator<Item = EGraph>>(egraphs: I) {
    #[cfg(feature = "ilp-cbc")]
    let mut optimal_dag: Vec<Box<dyn Extractor>> = Default::default();
    let mut optimal_tree: Vec<Box<dyn Extractor>> = Default::default();
    let mut others: Vec<Box<dyn Extractor>> = Default::default();

    for (_, ed) in extractors().into_iter() {
        match ed.optimal {
            #[cfg(feature = "ilp-cbc")]
            Optimal::Dag => optimal_dag.push(ed.extractor),
            Optimal::Tree => optimal_tree.push(ed.extractor),
            Optimal::Neither => others.push(ed.extractor),
        }
    }

    for egraph in egraphs {
        let mut optimal_tree_cost: Option<Cost> = None;

        for e in &optimal_tree {
            let extract = e.extract(&egraph, &egraph.root_eclasses);
            extract.check(&egraph);
            let tree_cost = extract.tree_cost(&egraph, &egraph.root_eclasses);
            if optimal_tree_cost.is_none() {
                optimal_tree_cost = Some(tree_cost);
                continue;
            }

            assert!(
                (tree_cost.into_inner() - optimal_tree_cost.unwrap().into_inner()).abs()
                    < EPSILON_ALLOWANCE
            );
        }

        for e in &others {
            let extract = e.extract(&egraph, &egraph.root_eclasses);
            extract.check(&egraph);
            let tree_cost = extract.tree_cost(&egraph, &egraph.root_eclasses);

            // The optimal tree cost should be <= any extractor's tree cost.
            if let Some(optimal_tree_cost) = optimal_tree_cost {
                assert!(optimal_tree_cost <= tree_cost + EPSILON_ALLOWANCE);
            }
        }

        #[cfg(feature = "ilp-cbc")]
        check_dag_optimal(&egraph, &optimal_dag, &others, optimal_tree_cost);
    }
}

// Run on all the .json test files
#[test]
fn run_on_test_egraphs() {
    use walkdir::WalkDir;

    let egraphs = WalkDir::new("./test_data/")
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| {
            e.file_type().is_file()
                && e.path().extension().and_then(std::ffi::OsStr::to_str) == Some("json")
        })
        .map(|e| e.path().to_string_lossy().into_owned())
        .map(|e| EGraph::from_json_file(e).unwrap());
    check_optimal_results(egraphs);
}

#[test]
#[should_panic]
#[allow(clippy::assertions_on_constants)]
fn check_assert_enabled() {
    assert!(false);
}

macro_rules! create_optimal_check_tests {
    ($($name:ident),*) => {
        $(
            #[test]
            fn $name() {
                // Fewer iterations when ilp-cbc is enabled since it's slow
                let iterations = if cfg!(feature = "ilp-cbc") { 100 } else { 10000 };
                let egraphs = (0..iterations).map(|_| generate_random_egraph());
                check_optimal_results(egraphs);
            }
        )*
    }
}

create_optimal_check_tests!(check0, check1, check2, check3, check4, check5, check6, check7);
