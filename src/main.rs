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

fn main() {
    env_logger::init();
    let extractors: IndexMap<&str, Box<dyn Extractor>> = [
        ("bottom-up", extract::bottom_up::BottomUpExtractor.boxed()),
        (
            "faster-bottom-up",
            extract::faster_bottom_up::FasterBottomUpExtractor.boxed(),
        ),
        (
            "faster-greedy-dag",
            extract::faster_greedy_dag::FasterGreedyDagExtractor.boxed(),
        ),
        (
            "greedy-dag",
            extract::greedy_dag::GreedyDagExtractor.boxed(),
        ),
        (
            "faster-greedy-dag",
            extract::faster_greedy_dag::FasterGreedyDagExtractor.boxed(),
        ),
        /*
        (
            "global-greedy-dag",
            extract::global_greedy_dag::GlobalGreedyDagExtractor.boxed(),
        ),
        */
        #[cfg(feature = "faster-ilp-cbc")]
        (
            "faster-ilp-cbc",
            extract::faster_ilp_cbc::FasterCbcExtractor.boxed(),
        ),
        #[cfg(feature = "ilp-cbc")]
        ("ilp-cbc", extract::ilp_cbc::CbcExtractor.boxed()),
    ]
    .into_iter()
    .collect();

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

    let extractor = extractors
        .get(extractor_name.as_str())
        .with_context(|| format!("Unknown extractor: {extractor_name}"))
        .unwrap();

    let start_time = std::time::Instant::now();
    let result = extractor.extract(&egraph, &egraph.root_eclasses);
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

/*
* Checks that no extractors produce better results than the extractors that produce optimal results.
* Checks that the extractions are valid.
*/

fn check_optimal_results() {
    let optimal_dag: Vec<Box<dyn Extractor>> = vec![
        #[cfg(feature = "faster-ilp-cbc")]
        (ilp_cbc::CbcExtractor.boxed()),
        #[cfg(feature = "faster-ilp-cbc")]
        (faster_ilp_cbc::FasterCbcExtractor.boxed()),
    ];

    let iterations = if optimal_dag.is_empty() { 2000 } else { 100 };

    let optimal_tree: Vec<Box<dyn Extractor>> = vec![
        bottom_up::BottomUpExtractor.boxed(),
        faster_bottom_up::FasterBottomUpExtractor.boxed(),
    ];

    let others: Vec<Box<dyn Extractor>> = vec![
        greedy_dag::GreedyDagExtractor.boxed(),
        faster_greedy_dag::FasterGreedyDagExtractor.boxed(),
        //global_greedy_dag::GlobalGreedyDagExtractor.boxed(),
    ];

    for _ in 0..iterations {
        let egraph = generate_random_egraph();

        let mut optimal_dag_cost: Option<Cost> = None;

        for e in &optimal_dag {
            let extract = e.extract(&egraph, &egraph.root_eclasses);
            extract.check(&egraph);
            let dag_cost = extract.dag_cost(&egraph, &egraph.root_eclasses);
            let tree_cost = extract.tree_cost(&egraph, &egraph.root_eclasses);
            if optimal_dag_cost.is_some() {
                assert!(
                    dag_cost.into_inner() + EPSILON_ALLOWANCE
                        > optimal_dag_cost.unwrap().into_inner()
                );

                assert!(
                    dag_cost.into_inner()
                        < optimal_dag_cost.unwrap().into_inner() + EPSILON_ALLOWANCE
                );

                assert!(
                    tree_cost.into_inner() + EPSILON_ALLOWANCE
                        > optimal_dag_cost.unwrap().into_inner()
                );
            } else {
                optimal_dag_cost = Some(dag_cost);
            }
        }

        let mut optimal_tree_cost: Option<Cost> = None;

        for e in &optimal_tree {
            let extract = e.extract(&egraph, &egraph.root_eclasses);
            extract.check(&egraph);
            let tree_cost = extract.tree_cost(&egraph, &egraph.root_eclasses);
            if optimal_tree_cost.is_some() {
                assert!(
                    tree_cost.into_inner() + EPSILON_ALLOWANCE
                        > optimal_tree_cost.unwrap().into_inner()
                );
                assert!(
                    tree_cost.into_inner()
                        < optimal_tree_cost.unwrap().into_inner() + EPSILON_ALLOWANCE
                );
            } else {
                optimal_tree_cost = Some(tree_cost);
            }
        }

        if optimal_dag_cost.is_some() {
            assert!(optimal_dag_cost.unwrap() < optimal_tree_cost.unwrap() + EPSILON_ALLOWANCE);
        }

        for e in &others {
            let extract = e.extract(&egraph, &egraph.root_eclasses);
            extract.check(&egraph);
            let tree_cost = extract.tree_cost(&egraph, &egraph.root_eclasses);
            let dag_cost = extract.dag_cost(&egraph, &egraph.root_eclasses);

            // The optimal tree cost should be <= any extractor's tree cost.
            assert!(optimal_tree_cost.unwrap() <= tree_cost + EPSILON_ALLOWANCE);

            if optimal_dag_cost.is_some() {
                // The optimal dag should be less <= any extractor's dag cost
                assert!(optimal_dag_cost.unwrap() <= dag_cost + EPSILON_ALLOWANCE);
            }
        }
    }
}

// Make several identical functions so they'll be run in parallel
#[test]
fn check0() {
    check_optimal_results();
}

#[test]
fn check1() {
    check_optimal_results();
}

#[test]
fn check2() {
    check_optimal_results();
}

#[test]
fn check3() {
    check_optimal_results();
}

#[test]
fn check4() {
    check_optimal_results();
}
#[test]
fn check5() {
    check_optimal_results();
}

#[test]
fn check6() {
    check_optimal_results();
}

#[test]
fn check7() {
    check_optimal_results();
}
