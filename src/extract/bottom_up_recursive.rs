use super::*;
use std::collections::HashMap;
use std::collections::HashSet;

// calculates a local cost for each node, where the local cost is:
// The cost of the node, plus the sum on it's children's cost.
// A class gets the cost of the chepest node it contains.

pub struct BottomUpRecursiveExtractor;

impl BottomUpRecursiveExtractor {
    // For debugging. At the end, we should be at a fixed-point. No updates should happen.
    fn check_finished(
        egraph: &EGraph,
        costs: &mut IndexMap<ClassId, Cost>,
        result: &mut ExtractionResult,
    ) {
        println!("checking");
        for class in egraph.classes().values() {
            for node in &class.nodes {
                // NB this sometimes fails because there are tiny differences between floating point numbers.
                assert!(!Self::update_class_cost(node, egraph, costs, result));
            }
        }
    }

    // If the node is cheaper, update the class's cost.
    fn update_class_cost(
        node_id: &NodeId,
        egraph: &EGraph,
        costs: &mut IndexMap<ClassId, Cost>,
        result: &mut ExtractionResult,
    ) -> bool {
        let node = &egraph[node_id];
        let cost = result.node_sum_cost(egraph, node, costs);
        let class_id = egraph.nid_to_cid(node_id);
        if &cost < costs.get(class_id).unwrap_or(&INFINITY) {
            result.choose(class_id.clone(), node_id.clone());
            costs.insert(class_id.clone(), cost);
            return true;
        }
        false
    }

    // Depth first from each node in class_id
    fn depth_first(
        class_id: &ClassId,
        egraph: &EGraph,
        costs: &mut IndexMap<ClassId, Cost>,
        result: &mut ExtractionResult,
        path: &mut HashSet<ClassId>,
        worklist: &mut HashSet<NodeId>,
    ) {
        if costs.contains_key(class_id) {
            // We don't update values until we have a worklist.
            return;
        }

        assert!(!path.contains(&class_id.clone()));
        path.insert(class_id.clone());

        let class = egraph.classes().get(class_id).unwrap();

        let mut best_cost = INFINITY;
        let mut best_node_id = class.nodes[0].clone();

        for node_id in &class.nodes {
            let node = &egraph[node_id];
            let mut cost = node.cost;

            for child_id in &node.children {
                let child_class_id = egraph.nid_to_cid(child_id);

                let mut child_cost = costs.get(child_class_id);
                if child_cost.is_none() {
                    if path.contains(child_class_id) {
                        // Cycle - need to reprocess it later.
                        worklist.insert(node_id.clone());
                    } else {
                        Self::depth_first(child_class_id, egraph, costs, result, path, worklist);
                        child_cost = costs.get(child_class_id);
                    }
                }

                if child_cost.is_none() {
                    cost += INFINITY;
                } else {
                    cost += child_cost.unwrap();
                }
            }

            if cost < best_cost {
                best_cost = cost;
                best_node_id = node_id.clone();
            }
        }

        result.choose(class_id.clone(), best_node_id.clone());
        costs.insert(class_id.clone(), best_cost);
        path.remove(class_id);
    }

    // builds back links. Maps from a class to it's parents.
    fn build_depends(egraph: &EGraph) -> HashMap<ClassId, HashSet<NodeId>> {
        let mut parents =
            HashMap::<ClassId, HashSet<NodeId>>::with_capacity(egraph.classes().len());

        for class in egraph.classes().values() {
            parents.insert(class.id.clone(), HashSet::<NodeId>::default());
        }

        for class in egraph.classes().values() {
            for node_id in &class.nodes {
                let node = &egraph[node_id];

                for child in &node.children {
                    let child_class_id = egraph.nid_to_cid(child);
                    parents
                        .get_mut(&child_class_id)
                        .unwrap()
                        .insert(node_id.clone());
                }
            }
        }
        parents
    }
}
impl Extractor for BottomUpRecursiveExtractor {
    fn extract(&self, egraph: &EGraph, _roots: &[ClassId]) -> ExtractionResult {
        let mut costs = IndexMap::<ClassId, Cost>::with_capacity(egraph.classes().len());
        let mut result = ExtractionResult::default();
        let mut path = HashSet::<ClassId>::default(); // Stack of visited classes, used to detect cycles.
        let mut worklist = HashSet::<NodeId>::default(); // Nodes we need to reprocess.

        for class in egraph.classes().values() {
            Self::depth_first(
                &class.id,
                egraph,
                &mut costs,
                &mut result,
                &mut path,
                &mut worklist,
            );
        }
        assert!(path.is_empty());

        if worklist.is_empty() {
            //Self::check_finished(egraph, &mut costs, &mut result);
            return result; // no cycle. all done.
        }

        // Anytime we detected a cycle, we added the node into "worklist"
        // Now, we go through the worklist, and using the dependencies we build (parents), we reprocess each of those
        // nodes. Anytime a class's cost reduces, we recursively process the parents - because their cost might have
        // changed (a different node might now be cheaper).

        let mut extras = Vec::<NodeId>::default();
        let mut parents = HashMap::<ClassId, HashSet<NodeId>>::with_capacity(0);
        let mut first = true;

        while !extras.is_empty() || !worklist.is_empty() {
            for e in &extras {
                worklist.insert(e.clone());
            }
            extras.clear();

            for nid in &worklist {
                let changed = Self::update_class_cost(nid, egraph, &mut costs, &mut result);

                if changed {
                    if first {
                        parents = Self::build_depends(egraph);
                        first = false;
                    }

                    let class_id = egraph.nid_to_cid(nid);
                    if parents.contains_key(class_id) {
                        for e0 in parents.get(class_id).unwrap() {
                            extras.push(e0.clone());
                        }
                    }
                }
            }
            worklist.clear();
        }

        //Self::check_finished(egraph, &mut costs, &mut result);
        result
    }
}
