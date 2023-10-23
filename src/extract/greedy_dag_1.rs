// Calculates the cost where shared nodes are just costed once,
// For example (+ (* x x ) (* x x )) has one mulitplication
// included in the cost.

use super::*;

struct CostSet {
    costs: std::collections::HashMap<ClassId, Cost>,
    total: Cost,
    choice: NodeId,
}

pub struct FasterGreedyDagExtractor;

impl FasterGreedyDagExtractor {
    fn calculate_cost_set(
        egraph: &EGraph,
        node_id: NodeId,
        costs: &HashMap<ClassId, CostSet>,
        best_cost: Cost,
    ) -> CostSet {
        let node = &egraph[&node_id];

        // No children -> easy.
        if node.children.is_empty() {
            return CostSet {
                costs: std::collections::HashMap::default(),
                total: node.cost,
                choice: node_id.clone(),
            };
        }

        // Get unique classes of children.
        let mut childrens_classes = node
            .children
            .iter()
            .map(|c| egraph.nid_to_cid(&c).clone())
            .collect::<Vec<ClassId>>();
        childrens_classes.sort();
        childrens_classes.dedup();

        let first_cost = costs.get(&childrens_classes[0]).unwrap();

        if childrens_classes.len() == 1 && (node.cost + first_cost.total > best_cost) {
            // Shortcut. Can't be cheaper so return junk.
            return CostSet {
                costs: std::collections::HashMap::default(),
                total: INFINITY,
                choice: node_id.clone(),
            };
        }

        // Clone the biggest set and insert the others into it.
        let id_of_biggest = childrens_classes
            .iter()
            .max_by_key(|s| costs.get(s).unwrap().costs.len())
            .unwrap();
        let mut result = costs.get(&id_of_biggest).unwrap().costs.clone();
        for child_cid in &childrens_classes {
            if child_cid == id_of_biggest {
                continue;
            }

            let next_cost = &costs.get(child_cid).unwrap().costs;
            for (key, value) in next_cost.iter() {
                result.insert(key.clone(), value.clone());
            }
        }

        let cid = egraph.nid_to_cid(&node_id);
        let contains = result.contains_key(&cid);
        result.insert(cid.clone(), node.cost);

        let result_cost = if contains {
            INFINITY
        } else {
            result.values().sum()
        };

        return CostSet {
            costs: result,
            total: result_cost,
            choice: node_id.clone(),
        };
    }
}

impl FasterGreedyDagExtractor {
    fn check(egraph: &EGraph, node_id: NodeId, costs: &HashMap<ClassId, CostSet>) {
        let cid = egraph.nid_to_cid(&node_id);
        let previous = costs.get(cid).unwrap().total;
        let cs = Self::calculate_cost_set(egraph, node_id, costs, INFINITY);
        println!("{} {}", cs.total, previous);
        assert!(cs.total >= previous);
    }
}

impl Extractor for FasterGreedyDagExtractor {
    fn extract(&self, egraph: &EGraph, _roots: &[ClassId]) -> ExtractionResult {
        // 1. build map from class to parent nodes
        let mut parents = IndexMap::<ClassId, Vec<NodeId>>::default();
        let n2c = |nid: &NodeId| egraph.nid_to_cid(nid);

        for class in egraph.classes().values() {
            parents.insert(class.id.clone(), Vec::new());
        }
        for class in egraph.classes().values() {
            for node in &class.nodes {
                for c in &egraph[node].children {
                    parents[n2c(c)].push(node.clone());
                }
            }
        }

        // 2. start analysis from leaves
        let mut analysis_pending = UniqueQueue::default();

        for class in egraph.classes().values() {
            for node in &class.nodes {
                if egraph[node].is_leaf() {
                    analysis_pending.insert(node.clone());
                }
            }
        }

        // 3. analyse from leaves towards parents until fixpoint
        let mut costs = HashMap::<ClassId, CostSet>::default();

        while let Some(node_id) = analysis_pending.pop() {
            let class_id = n2c(&node_id);
            let node = &egraph[&node_id];
            if node.children.iter().all(|c| costs.contains_key(n2c(c))) {
                let lookup = costs.get(class_id);
                let mut prev_cost = INFINITY;
                if lookup.is_some() {
                    prev_cost = lookup.unwrap().total;
                }

                let cost_set = Self::calculate_cost_set(egraph, node_id.clone(), &costs, prev_cost);
                if cost_set.total < prev_cost {
                    costs.insert(class_id.clone(), cost_set);
                    analysis_pending.extend(parents[class_id].iter().cloned());
                }
            } else {
                analysis_pending.insert(node_id.clone());
            }
        }

        /*
                for class in egraph.classes().values() {
                    for node in &class.nodes {
                        Self::check(&egraph, node.clone(), &costs);
                    }
                }
        */

        let mut result = ExtractionResult::default();
        for (cid, cost_set) in costs {
            result.choose(cid, cost_set.choice);
        }

        result
    }
}

/** A data structure to maintain a queue of unique elements.

Notably, insert/pop operations have O(1) expected amortized runtime complexity.
*/
#[derive(Clone)]
#[cfg_attr(feature = "serde-1", derive(Serialize, Deserialize))]
pub(crate) struct UniqueQueue<T>
where
    T: Eq + std::hash::Hash + Clone,
{
    set: std::collections::HashSet<T>, // hashbrown::
    queue: std::collections::VecDeque<T>,
}

impl<T> Default for UniqueQueue<T>
where
    T: Eq + std::hash::Hash + Clone,
{
    fn default() -> Self {
        UniqueQueue {
            set: std::collections::HashSet::default(),
            queue: std::collections::VecDeque::new(),
        }
    }
}

impl<T> UniqueQueue<T>
where
    T: Eq + std::hash::Hash + Clone,
{
    pub fn insert(&mut self, t: T) {
        if self.set.insert(t.clone()) {
            self.queue.push_back(t);
        }
    }

    pub fn extend<I>(&mut self, iter: I)
    where
        I: IntoIterator<Item = T>,
    {
        for t in iter.into_iter() {
            self.insert(t);
        }
    }

    pub fn pop(&mut self) -> Option<T> {
        let res = self.queue.pop_front();
        res.as_ref().map(|t| self.set.remove(t));
        res
    }

    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        let r = self.queue.is_empty();
        debug_assert_eq!(r, self.set.is_empty());
        r
    }
}
