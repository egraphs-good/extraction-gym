use super::*;

struct CostSet {
    costs: std::collections::HashMap<ClassId, Cost>,
    total: Cost,
    choice: NodeId,
}

pub struct GreedyDagExtractor;
impl Extractor for GreedyDagExtractor {
    fn extract(&self, egraph: &EGraph, _roots: &[ClassId]) -> ExtractionResult {
        let mut costs = IndexMap::<ClassId, CostSet>::default();
        let mut keep_going = true;

        let mut nodes = egraph.nodes.clone();

        let mut i = 0;
        while keep_going {
            i += 1;
            println!("iteration {}", i);
            keep_going = false;

            let mut to_remove = vec![];

            'node_loop: for (node_id, node) in &nodes {
                let cid = egraph.nid_to_cid(node_id);
                let mut cost_set = CostSet {
                    costs: std::collections::HashMap::new(),
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
                to_remove.push(node_id.clone());
            }

            // removing nodes you've "done" can speed it up a lot but makes the results much worse
            if false {
                for node_id in to_remove {
                    nodes.remove(&node_id);
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
