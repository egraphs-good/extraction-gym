mod egraph;
mod extract;

pub use egraph::*;
pub use extract::*;

use indexmap::IndexMap;
use ordered_float::NotNan;

use anyhow::Context;
use rayon::prelude::*;

use std::io::Write;
use std::{path::PathBuf, str::FromStr};

pub type Cost = NotNan<f64>;
pub const INFINITY: Cost = unsafe { NotNan::new_unchecked(std::f64::INFINITY) };

pub type Id = usize;

fn main() {
    env_logger::init();

    let mut args = pico_args::Arguments::from_env();

    let out_filename: PathBuf = args
        .opt_value_from_str("--out")
        .unwrap()
        .unwrap_or_else(|| "out.csv".into());

    let mut out_file = std::fs::File::create(&out_filename).unwrap();

    let mut filenames: Vec<String> = vec![];
    while let Some(filename) = args.opt_free_from_str().unwrap() {
        filenames.push(filename);
    }

    let extractors: IndexMap<&str, Box<dyn Extractor>> = [
        ("bottom-up", extract::bottom_up::BottomUpExtractor.boxed()),
        #[cfg(feature = "ilp-cbc")]
        ("ilp-cbc", extract::ilp_cbc::CbcExtractor.boxed()),
    ]
    .into_iter()
    .collect();

    let go = |filename| {
        let mut rows = vec![];
        let contents = std::fs::read_to_string(filename)
            .unwrap_or_else(|e| panic!("Failed to read {filename}: {e}"));
        let egraph = contents
            .parse::<SimpleEGraph>()
            .with_context(|| format!("Failed to parse {filename}"))
            .unwrap();

        for (ext_name, extractor) in &extractors {
            let start_time = std::time::Instant::now();
            let result = extractor.extract(&egraph, &egraph.roots);
            let elapsed = start_time.elapsed();
            let msg = format!(
                "{filename:40}, {ext_name:10}, {tree:4}, {dag:4}, {us:8}",
                tree = result.tree_cost(&egraph, &egraph.roots),
                dag = result.dag_cost(&egraph, &egraph.roots),
                us = elapsed.as_micros(),
            );
            log::info!("{}", msg);
            rows.push(msg);
        }

        rows
    };

    writeln!(out_file, "file, extractor, tree, dag, time (us)").unwrap();

    // check if there is parallelism
    let rows = match std::env::var("RAYON_NUM_THREADS") {
        Ok(threads) if threads == "1" => filenames.iter().flat_map(go).collect::<Vec<String>>(),
        _ => filenames.par_iter().flat_map(go).collect::<Vec<String>>(),
    };

    for row in rows {
        writeln!(out_file, "{}", row).unwrap();
    }
}
