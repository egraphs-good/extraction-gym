mod egraph;
mod extract;

pub use egraph::*;
pub use extract::*;

use indexmap::IndexMap;
use ordered_float::NotNan;

use std::io::Write;
use std::{path::PathBuf, str::FromStr};

pub type Cost = NotNan<f64>;
pub const INFINITY: Cost = unsafe { NotNan::new_unchecked(std::f64::INFINITY) };

pub type Id = usize;

fn main() {
    env_logger::builder()
        .filter_level(log::LevelFilter::Info)
        .init();

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

    writeln!(out_file, "file, extractor, tree, dag, time (us)").unwrap();
    for filename in &filenames {
        let contents = std::fs::read_to_string(filename)
            .unwrap_or_else(|e| panic!("Failed to read {filename}: {e}"));
        let egraph = contents.parse::<SimpleEGraph>().unwrap();

        for (ext_name, extractor) in &extractors {
            let start_time = std::time::Instant::now();
            let result = extractor.extract(&egraph, &egraph.roots);
            let elapsed = start_time.elapsed();
            for &root in &egraph.roots {
                let msg = format!(
                    "{filename:40}, {ext_name:10}, {tree:4}, {dag:4}, {us:8}",
                    tree = result.tree_cost(&egraph, root),
                    dag = result.dag_cost(&egraph, root),
                    us = elapsed.as_micros(),
                );
                writeln!(out_file, "{}", msg).unwrap();
                log::info!("{}", msg);
            }
        }
    }
}
