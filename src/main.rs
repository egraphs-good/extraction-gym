mod egraph;
mod extract;

pub use egraph::*;
pub use extract::*;

use indexmap::indexmap;
use ordered_float::NotNan;
use std::str::FromStr;

pub type Cost = NotNan<f64>;
pub const INFINITY: Cost = unsafe { NotNan::new_unchecked(std::f64::INFINITY) };

pub type Id = usize;

fn main() {
    let args = std::env::args().collect::<Vec<_>>();

    let extractors = indexmap! {
        "bottom-up" => extract::bottom_up::BottomUpExtractor.boxed(),
        // this is a fake second extractor just to test my loops,
        // delete it once there's a real second extractor
        "bottom-up2" => extract::bottom_up::BottomUpExtractor.boxed(),
    };

    println!("file, extractor, tree, dag, time (us)");
    for filename in &args[1..] {
        let contents = std::fs::read_to_string(filename).unwrap();
        let egraph = contents.parse::<SimpleEGraph>().unwrap();

        for (ext_name, extractor) in &extractors {
            let start_time = std::time::Instant::now();
            let result = extractor.extract(&egraph, &egraph.roots);
            let elapsed = start_time.elapsed();
            for &root in &egraph.roots {
                println!(
                    "{filename:40}, {ext_name:10}, {tree:4}, {dag:4}, {us:8}",
                    tree = result.tree_cost(&egraph, root),
                    dag = result.dag_cost(&egraph, root),
                    us = elapsed.as_micros(),
                );
            }
        }
    }
}
