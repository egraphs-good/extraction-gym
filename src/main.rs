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

struct ExtractorDetail {
    extractor: Box<dyn Extractor>,
    is_dag_optimal: bool,
    is_tree_optimal: bool,
    use_for_bench: bool,
}

fn extractors() -> IndexMap<&'static str, ExtractorDetail> {
    let extractors: IndexMap<&'static str, ExtractorDetail> = [
        (
            "bottom-up",
            ExtractorDetail {
                extractor: extract::bottom_up::BottomUpExtractor.boxed(),
                is_dag_optimal: false,
                is_tree_optimal: true,
                use_for_bench: true,
            },
        ),
        (
            "faster-bottom-up",
            ExtractorDetail {
                extractor: extract::faster_bottom_up::FasterBottomUpExtractor.boxed(),
                is_dag_optimal: false,
                is_tree_optimal: true,
                use_for_bench: true,
            },
        ),
        /*(
            "faster-greedy-dag",
            ExtractorDetail {
                extractor: extract::faster_greedy_dag::FasterGreedyDagExtractor.boxed(),
                is_dag_optimal: false,
                is_tree_optimal: false,
                use_for_bench: true,
            },
        ),*/

        /*(
            "global-greedy-dag",
            ExtractorDetail {
                extractor: extract::global_greedy_dag::GlobalGreedyDagExtractor.boxed(),
                is_dag_optimal: false,
                is_tree_optimal: false,
                use_for_bench: true,
            },
        ),*/
        #[cfg(feature = "ilp-cbc")]
        (
            "ilp-cbc-timeout",
            ExtractorDetail {
                extractor: extract::ilp_cbc::CbcExtractorWithTimeout::<10>.boxed(),
                is_dag_optimal: false,
                is_tree_optimal: false,
                use_for_bench: true,
            },
        ),
        #[cfg(feature = "ilp-cbc")]
        (
            "ilp-cbc",
            ExtractorDetail {
                extractor: extract::ilp_cbc::CbcExtractor.boxed(),
                is_dag_optimal: true,
                is_tree_optimal: false,
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

/*
* Checks that no extractors produce better results than the extractors that produce optimal results.
* Checks that the extractions are valid.
*/

fn check_optimal_results<I: Iterator<Item = EGraph>>(egraphs: I) {
    let optimal_dag: Vec<Box<dyn Extractor>> = extractors()
        .into_iter()
        .filter(|(_, ed)| ed.is_dag_optimal)
        .map(|(_, ed)| ed.extractor)
        .collect();

    let optimal_tree: Vec<Box<dyn Extractor>> = extractors()
        .into_iter()
        .filter(|(_, ed)| ed.is_tree_optimal)
        .map(|(_, ed)| ed.extractor)
        .collect();

    let others: Vec<Box<dyn Extractor>> = extractors()
        .into_iter()
        .filter(|(_, ed)| !ed.is_dag_optimal || !ed.is_tree_optimal)
        .map(|(_, ed)| ed.extractor)
        .collect();

    let mut count = 0;
    for egraph in egraphs {
        count += 1;

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
                tree_cost.into_inner() + EPSILON_ALLOWANCE > optimal_dag_cost.unwrap().into_inner()
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

// Run on all the .json files in the data/fuzz directory
#[test]
fn run_on_fuzz_egraphs() {
    use walkdir::WalkDir;

    let egraphs = WalkDir::new("./data/fuzz")
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
                let optimal_dag_found = extractors().into_iter().any(|(_, ed)| ed.is_dag_optimal);
                let iterations = if optimal_dag_found { 100 } else { 10000 };
                let egraphs = (0..iterations).map(|_| generate_random_egraph());
                check_optimal_results(egraphs);
            }
        )*
    }
}

create_optimal_check_tests!(check0, check1, check2, check3, check4, check5, check6, check7);
