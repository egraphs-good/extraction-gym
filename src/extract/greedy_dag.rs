use super::*;
use rustc_hash::FxHashMap;

struct CostSet {
    costs: FxHashMap<ClassId, Cost>,
    total: Cost,
    choice: NodeId,
}

pub struct GreedyDagExtractor;
impl Extractor for GreedyDagExtractor {
    fn extract(&self, egraph: &EGraph, _roots: &[ClassId]) -> ExtractionResult {
        let mut costs = FxHashMap::<ClassId, CostSet>::with_capacity_and_hasher(
            egraph.classes().len(),
            Default::default(),
        );

        let mut keep_going = true;

        let mut i = 0;
        while keep_going {
            i += 1;
            println!("iteration {}", i);
            keep_going = false;

            'node_loop: for (node_id, node) in &egraph.nodes {
                let cid = egraph.nid_to_cid(node_id);
                let mut cost_set = CostSet {
                    costs: Default::default(),
                    total: Cost::default(),
                    choice: node_id.clone(),
                };

                // compute the cost set from the children
                for child in &node.children {
                    let child_cid = egraph.nid_to_cid(child);
                    if let Some(child_cost_set) = costs.get(child_cid) {
                        // prevent a cycle
                        if child_cost_set.costs.contains_key(cid) {
                            continue 'node_loop;
                        }
                        cost_set.costs.extend(child_cost_set.costs.clone());
                    } else {
                        continue 'node_loop;
                    }
                }

                // add this node
                cost_set.costs.insert(cid.clone(), node.cost);

                cost_set.total = cost_set.costs.values().sum();

                // if the cost set is better than the current one, update it
                if let Some(old_cost_set) = costs.get(cid) {
                    if cost_set.total < old_cost_set.total {
                        costs.insert(cid.clone(), cost_set);
                        keep_going = true;
                    }
                } else {
                    costs.insert(cid.clone(), cost_set);
                    keep_going = true;
                }
            }
        }

        let mut result = ExtractionResult::default();
        for (cid, cost_set) in costs {
            result.choose(cid, cost_set.choice);
        }
        result
    }
}
