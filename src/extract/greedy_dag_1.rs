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
    ) -> CostSet {
        let node = &egraph[&node_id];

        let cid = egraph.nid_to_cid(&node_id);

        let mut desc = 0;
        let mut children_cost = Cost::default();
        for child in &node.children {
            let child_cid = egraph.nid_to_cid(child);
            let cs = costs.get(child_cid).unwrap();
            desc += cs.costs.len();
            children_cost += cs.total;
        }

        let mut cost_set = CostSet {
            costs: std::collections::HashMap::with_capacity(desc),
            total: Cost::default(),
            choice: node_id.clone(),
        };

        for child in &node.children {
            let child_cid = egraph.nid_to_cid(child);
            cost_set
                .costs
                .extend(costs.get(child_cid).unwrap().costs.clone());
        }

        let contains = cost_set.costs.contains_key(&cid.clone());
        cost_set.costs.insert(cid.clone(), node.cost); // this node.

        if contains {
            cost_set.total = INFINITY;
        } else {
            if cost_set.costs.len() == desc + 1 {
                // No extra duplicates are found, so the cost is the current
                // nodes cost + the children's cost.
                cost_set.total = children_cost + node.cost;
            } else {
                cost_set.total = cost_set.costs.values().sum();
            }
        };

        cost_set
    }
}

impl FasterGreedyDagExtractor {
    fn check(egraph: &EGraph, node_id: NodeId, costs: &HashMap<ClassId, CostSet>) {
        let cid = egraph.nid_to_cid(&node_id);
        let previous = costs.get(cid).unwrap().total;
        let cs = Self::calculate_cost_set(egraph, node_id, costs);
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

                let cost_set = Self::calculate_cost_set(egraph, node_id.clone(), &costs);
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
