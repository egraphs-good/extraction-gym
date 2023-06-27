use super::*;

pub struct BottomUpExtractor;
impl Extractor for BottomUpExtractor {
    fn extract(&self, egraph: &SimpleEGraph, _roots: &[Id]) -> ExtractionResult {
        let mut result = ExtractionResult::new(egraph.classes.len());
        let mut costs = vec![INFINITY; egraph.classes.len()];
        let mut did_something = false;

        loop {
            for (i, class) in egraph.classes.values().enumerate() {
                for (node_i, node) in class.nodes.iter().enumerate() {
                    let cost = result.node_sum_cost(node, &costs);
                    if cost < costs[i] {
                        result.choices[i] = node_i;
                        costs[i] = cost;
                        did_something = true;
                    }
                }
            }

            if did_something {
                did_something = false;
            } else {
                break;
            }
        }

        result
    }
}
