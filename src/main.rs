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
        (
            "prio-queue",
            ExtractorDetail {
                extractor: extract::prio_queue::PrioQueueExtractor.boxed(),
                optimal: Optimal::Tree,
                use_for_bench: true,
            },
        ),
        (
            "faster-greedy-dag",
            ExtractorDetail {
                extractor: extract::faster_greedy_dag::FasterGreedyDagExtractor.boxed(),
                optimal: Optimal::Neither,
                use_for_bench: true,
            },
        ),
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
        #[cfg(feature = "ilp-cbc")]
        (
            "faster-ilp-cbc-timeout",
            ExtractorDetail {
                extractor: extract::faster_ilp_cbc::FasterCbcExtractorWithTimeout::<10>.boxed(),
                optimal: Optimal::DAG,
                use_for_bench: true,
            },
        ),
        #[cfg(feature = "ilp-cbc")]
        (
            "faster-ilp-cbc",
            ExtractorDetail {
                extractor: extract::faster_ilp_cbc::FasterCbcExtractor::<10>.boxed(),
                optimal: Optimal::DAG,
                use_for_bench: true,
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
mod test;
