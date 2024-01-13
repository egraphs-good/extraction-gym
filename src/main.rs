mod extract;

pub use extract::*;

use egraph_serialize::*;

use indexmap::IndexMap;
use ordered_float::NotNan;

use anyhow::Context;

use std::io::Write;
use std::path::PathBuf;

pub type Cost = NotNan<f64>;
pub const INFINITY: Cost = unsafe { NotNan::new_unchecked(std::f64::INFINITY) };

#[derive(PartialEq, Eq)]
enum Optimal {
    Tree,
    DAG,
    Neither,
}

struct ExtractorDetail {
    extractor: Box<dyn Extractor>,
    optimal: Optimal,
    use_for_bench: bool,
}

fn extractors() -> IndexMap<&'static str, ExtractorDetail> {
    let extractors: IndexMap<&'static str, ExtractorDetail> = [
        (
            "bottom-up",
            ExtractorDetail {
                extractor: extract::bottom_up::BottomUpExtractor.boxed(),
                optimal: Optimal::Tree,
                use_for_bench: true,
            },
        ),
        (
            "faster-bottom-up",
            ExtractorDetail {
                extractor: extract::faster_bottom_up::FasterBottomUpExtractor.boxed(),
                optimal: Optimal::Tree,
                use_for_bench: true,
            },
        ),
        /*(
            "faster-greedy-dag",
            ExtractorDetail {
                extractor: extract::faster_greedy_dag::FasterGreedyDagExtractor.boxed(),
                optimal: Optimal::Neither,
                use_for_bench: true,
            },
        ),*/

        /*(
            "global-greedy-dag",
            ExtractorDetail {
                extractor: extract::global_greedy_dag::GlobalGreedyDagExtractor.boxed(),
                optimal: Optimal::Neither,
                use_for_bench: true,
            },
        ),*/
        #[cfg(feature = "ilp-cbc")]
        (
            "ilp-cbc-timeout",
            ExtractorDetail {
                extractor: extract::ilp_cbc::CbcExtractorWithTimeout::<10>.boxed(),
                optimal: Optimal::DAG,
                use_for_bench: true,
            },
        ),
        #[cfg(feature = "ilp-cbc")]
        (
            "ilp-cbc",
            ExtractorDetail {
                extractor: extract::ilp_cbc::CbcExtractor.boxed(),
                optimal: Optimal::DAG,
                use_for_bench: false, // takes >10 hours sometimes
            },
        ),
    ]
    .into_iter()
    .collect();
    return extractors;
}

fn main() {
    env_logger::init();

    let mut extractors = extractors();
    extractors.retain(|_, ed| ed.use_for_bench);

    let mut args = pico_args::Arguments::from_env();

    let extractor_name: String = args
        .opt_value_from_str("--extractor")
        .unwrap()
        .unwrap_or_else(|| "bottom-up".into());
    if extractor_name == "print" {
        for name in extractors.keys() {
            println!("{}", name);
        }
        return;
    }

    let out_filename: PathBuf = args
        .opt_value_from_str("--out")
        .unwrap()
        .unwrap_or_else(|| "out.json".into());

    let filename: String = args.free_from_str().unwrap();

    let rest = args.finish();
    if !rest.is_empty() {
        panic!("Unknown arguments: {:?}", rest);
    }

    let mut out_file = std::fs::File::create(out_filename).unwrap();

    let egraph = EGraph::from_json_file(&filename)
        .with_context(|| format!("Failed to parse {filename}"))
        .unwrap();

    let ed = extractors
        .get(extractor_name.as_str())
        .with_context(|| format!("Unknown extractor: {extractor_name}"))
        .unwrap();

    let start_time = std::time::Instant::now();
    let result = ed.extractor.extract(&egraph, &egraph.root_eclasses);
    let us = start_time.elapsed().as_micros();

    result.check(&egraph);

    let tree = result.tree_cost(&egraph, &egraph.root_eclasses);
    let dag = result.dag_cost(&egraph, &egraph.root_eclasses);

    log::info!("{filename:40}\t{extractor_name:10}\t{tree:5}\t{dag:5}\t{us:5}");
    writeln!(
        out_file,
        r#"{{ 
    "name": "{filename}",
    "extractor": "{extractor_name}", 
    "tree": {tree}, 
    "dag": {dag}, 
    "micros": {us}
}}"#
    )
    .unwrap();
}

#[cfg(test)]
mod main_test {
    /*
     * Checks that no extractors produce better results than the extractors that produce optimal results.
     * Checks that the extractions are valid.
     */

    use crate::{extractors, Extractor, Optimal, EPSILON_ALLOWANCE};
    pub type Cost = NotNan<f64>;
    use egraph_serialize::{EGraph, Node, NodeId};
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

    fn check_optimal_results<I: Iterator<Item = EGraph>>(egraphs: I) {
        let mut optimal_dag: Vec<Box<dyn Extractor>> = Default::default();
        let mut optimal_tree: Vec<Box<dyn Extractor>> = Default::default();
        let mut others: Vec<Box<dyn Extractor>> = Default::default();

        for (_, ed) in extractors().into_iter() {
            match ed.optimal {
                Optimal::DAG => optimal_dag.push(ed.extractor),
                Optimal::Tree => optimal_tree.push(ed.extractor),
                Optimal::Neither => others.push(ed.extractor),
            }
        }

        for egraph in egraphs {
            let mut optimal_dag_cost: Option<Cost> = None;

            for e in &optimal_dag {
                let extract = e.extract(&egraph, &egraph.root_eclasses);
                extract.check(&egraph);
                let dag_cost = extract.dag_cost(&egraph, &egraph.root_eclasses);
                let tree_cost = extract.tree_cost(&egraph, &egraph.root_eclasses);
                if optimal_dag_cost.is_none() {
                    optimal_dag_cost = Some(dag_cost);
                    continue;
                }

                assert!(
                    (dag_cost.into_inner() - optimal_dag_cost.unwrap().into_inner()).abs()
                        < EPSILON_ALLOWANCE
                );

                assert!(
                    tree_cost.into_inner() + EPSILON_ALLOWANCE
                        > optimal_dag_cost.unwrap().into_inner()
                );
            }

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

            if optimal_dag_cost.is_some() && optimal_tree_cost.is_some() {
                assert!(optimal_dag_cost.unwrap() < optimal_tree_cost.unwrap() + EPSILON_ALLOWANCE);
            }

            for e in &others {
                let extract = e.extract(&egraph, &egraph.root_eclasses);
                extract.check(&egraph);
                let tree_cost = extract.tree_cost(&egraph, &egraph.root_eclasses);
                let dag_cost = extract.dag_cost(&egraph, &egraph.root_eclasses);

                // The optimal tree cost should be <= any extractor's tree cost.
                if optimal_tree_cost.is_some() {
                    assert!(optimal_tree_cost.unwrap() <= tree_cost + EPSILON_ALLOWANCE);
                }

                if optimal_dag_cost.is_some() {
                    // The optimal dag should be less <= any extractor's dag cost
                    assert!(optimal_dag_cost.unwrap() <= dag_cost + EPSILON_ALLOWANCE);
                }
            }
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
    fn check_assert_enabled() {
        assert!(false);
    }

    macro_rules! create_optimal_check_tests {
    ($($name:ident),*) => {
        $(
            #[test]
            fn $name() {
                let optimal_dag_found = extractors().into_iter().any(|(_, ed)| ed.optimal == Optimal::DAG);
                let iterations = if optimal_dag_found { 100 } else { 10000 };
                let egraphs = (0..iterations).map(|_| generate_random_egraph());
                check_optimal_results(egraphs);
            }
        )*
    }
}

    create_optimal_check_tests!(check0, check1, check2, check3, check4, check5, check6, check7);
}
