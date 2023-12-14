use std::iter;

use rpds::{HashTrieMap, HashTrieSet};

use super::*;

type TermId = usize;

#[derive(Clone, PartialEq, Eq, Hash)]
struct Term {
    op: String,
    children: Vec<TermId>,
}

type Reachable = HashTrieSet<ClassId>;

struct TermInfo {
    node: NodeId,
    eclass: ClassId,
    node_cost: Cost,
    total_cost: Cost,
    // store the set of reachable terms from this term
    reachable: Reachable,
    size: usize,
}

/// A TermDag needs to store terms that share common
/// subterms using a hashmap.
/// However, it also critically needs to be able to answer
/// reachability queries in this dag `reachable`.
/// This prevents double-counting costs when
/// computing the cost of a term.
#[derive(Default)]
pub struct TermDag {
    nodes: Vec<Term>,
    info: Vec<TermInfo>,
    hash_cons: HashMap<Term, TermId>,
}

impl TermDag {
    /// Makes a new term using a node and children terms
    /// Correctly computes total_cost with sharing
    /// If this term contains itself, returns None
    /// If this term costs more than target, returns None
    pub fn make(
        &mut self,
        node_id: NodeId,
        node: &Node,
        children: Vec<TermId>,
        target: Cost,
    ) -> Option<TermId> {
        let term = Term {
            op: node.op.clone(),
            children: children.clone(),
        };

        if let Some(id) = self.hash_cons.get(&term) {
            return Some(*id);
        }

        let node_cost = node.cost;

        if children.is_empty() {
            let next_id = self.nodes.len();
            self.nodes.push(term.clone());
            self.info.push(TermInfo {
                node: node_id,
                eclass: node.eclass.clone(),
                node_cost,
                total_cost: node_cost,
                reachable: iter::once(node.eclass.clone()).collect(),
                size: 1,
            });
            self.hash_cons.insert(term, next_id);
            Some(next_id)
        } else {
            // check if children contains this node, preventing cycles
            // This is sound because `reachable` is the set of reachable eclasses
            // from this term.
            for child in &children {
                if self.info[*child].reachable.contains(&node.eclass) {
                    return None;
                }
            }

            let biggest_child = (0..children.len())
                .max_by_key(|i| self.info[children[*i]].size)
                .unwrap();

            let mut cost = node_cost + self.total_cost(children[biggest_child]);
            let mut reachable = self.info[children[biggest_child]].reachable.clone();
            let next_id = self.nodes.len();

            for child in children.iter() {
                if cost > target {
                    return None;
                }
                let child_cost = self.get_cost(&mut reachable, *child);
                cost += child_cost;
            }

            if cost > target {
                return None;
            }

            reachable = reachable.insert(node.eclass.clone());

            self.info.push(TermInfo {
                node: node_id,
                node_cost,
                eclass: node.eclass.clone(),
                total_cost: cost,
                reachable,
                size: 1 + children.iter().map(|c| self.info[*c].size).sum::<usize>(),
            });
            self.nodes.push(term.clone());
            self.hash_cons.insert(term, next_id);
            Some(next_id)
        }
    }

    /// Return a new term, like this one but making use of shared terms.
    /// Also return the cost of the new nodes.
    fn get_cost(&self, shared: &mut Reachable, id: TermId) -> Cost {
        let eclass = self.info[id].eclass.clone();

        // This is the key to why this algorithm is faster than greedy_dag.
        // While doing the set union between reachable sets, we can stop early
        // if we find a shared term.
        // Since the term with `id` is shared, the reachable set of `id` will already
        // be in `shared`.
        if shared.contains(&eclass) {
            NotNan::<f64>::new(0.0).unwrap()
        } else {
            let mut cost = self.node_cost(id);
            for child in &self.nodes[id].children {
                let child_cost = self.get_cost(shared, *child);
                cost += child_cost;
            }
            *shared = shared.insert(eclass);
            cost
        }
    }

    pub fn node_cost(&self, id: TermId) -> Cost {
        self.info[id].node_cost
    }

    pub fn total_cost(&self, id: TermId) -> Cost {
        self.info[id].total_cost
    }
}

pub struct GlobalGreedyDagExtractor;
impl Extractor for GlobalGreedyDagExtractor {
    fn extract(&self, egraph: &EGraph, _roots: &[ClassId]) -> ExtractionResult {
        let mut keep_going = true;

        let nodes = egraph.nodes.clone();
        let mut termdag = TermDag::default();
        let mut best_in_class: HashMap<ClassId, TermId> = HashMap::default();

        let mut i = 0;
        while keep_going {
            i += 1;
            println!("iteration {}", i);
            keep_going = false;

            'node_loop: for (node_id, node) in &nodes {
                let mut children: Vec<TermId> = vec![];
                // compute the cost set from the children
                for child in &node.children {
                    let child_cid = egraph.nid_to_cid(child);
                    if let Some(best) = best_in_class.get(child_cid) {
                        children.push(*best);
                    } else {
                        continue 'node_loop;
                    }
                }

                let old_cost = best_in_class
                    .get(&node.eclass)
                    .map(|id| termdag.total_cost(*id))
                    .unwrap_or(INFINITY);

                if let Some(candidate) = termdag.make(node_id.clone(), node, children, old_cost) {
                    let cadidate_cost = termdag.total_cost(candidate);

                    if cadidate_cost < old_cost {
                        best_in_class.insert(node.eclass.clone(), candidate);
                        keep_going = true;
                    }
                }
            }
        }

        let mut result = ExtractionResult::default();
        for (class, term) in best_in_class {
            result.choose(class, termdag.info[term].node.clone());
        }
        result
    }
}
