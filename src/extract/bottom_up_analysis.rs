use super::*;

pub struct BottomUpAnalysisExtractor;

impl Extractor for BottomUpAnalysisExtractor {
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
        let mut result = ExtractionResult::default();
        let mut costs = IndexMap::<ClassId, Cost>::default();

        while let Some(node_id) = analysis_pending.pop() {
            let class_id = n2c(&node_id);
            let node = &egraph[&node_id];
            if node
                .children
                .iter()
                .all(|c| result.choices.contains_key(n2c(c)))
            {
                let prev_cost = costs.get(class_id).unwrap_or(&INFINITY);
                let cost = result.node_sum_cost(egraph, node, &costs);
                if &cost < prev_cost {
                    result.choose(class_id.clone(), node_id.clone());
                    costs.insert(class_id.clone(), cost);
                    analysis_pending.extend(parents[class_id].iter().cloned());
                }
            } else {
                analysis_pending.insert(node_id);
            }
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
