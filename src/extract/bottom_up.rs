use super::*;

pub struct BottomUpExtractor;
impl Extractor for BottomUpExtractor {
    fn extract(&self, egraph: &EGraph, _roots: &[ClassId]) -> ExtractionResult {
        let mut result = ExtractionResult::default();
        let mut costs = FxHashMap::<ClassId, Cost>::with_capacity_and_hasher(
            egraph.classes().len(),
            Default::default(),
        );
        let mut repeat = true;
        while repeat {
            repeat = false;
            for class in egraph.classes().values() {
                for node in &class.nodes {
                    let cost = result.node_sum_cost(egraph, &egraph[node], &costs);
                    if &cost < costs.get(&class.id).unwrap_or(&INFINITY) {
                        result.choose(class.id.clone(), node.clone());
                        costs.insert(class.id.clone(), cost);
                        repeat = true;
                    }
                }
            }
        }

        result
    }
}
