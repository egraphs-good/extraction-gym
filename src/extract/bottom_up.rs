use super::*;

pub struct BottomUpExtractor;
impl Extractor for BottomUpExtractor {
    fn extract(&self, egraph: &EGraph, _roots: &[ClassId]) -> ExtractionResult {
        let mut result = ExtractionResult::default();
        let mut costs = IndexMap::<ClassId, Cost>::default();
        let mut did_something = false;

        loop {
            for class in egraph.classes().values() {
                for node in &class.nodes {
                    let cost = result.node_sum_cost(egraph, &egraph[node], &costs);
                    if &cost < costs.get(&class.id).unwrap_or(&INFINITY) {
                        result.choose(class.id.clone(), node.clone());
                        costs.insert(class.id.clone(), cost);
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
