use rpds::HashTrieSet;

use super::*;

type TermId = usize;

#[derive(Clone, PartialEq, Eq, Hash)]
struct Term {
    op: String,
    children: Vec<TermId>,
}

struct TermInfo {
    node: NodeId,
    node_cost: NotNan<f64>,
    total_cost: NotNan<f64>,
    reachable: HashTrieSet<TermId>,
}

// A TermDag needs to store terms that share common
// subterms using a hashmap.
// However, it also critically needs to be able to answer
// reachability queries in this dag `reachable`.
// This prevents double-counting costs when
// computing the cost of a term.
#[derive(Default)]
pub struct TermDag {
    nodes: Vec<Term>,
    info: Vec<TermInfo>,
    hash_cons: HashMap<Term, TermId>,
}

impl TermDag {
    pub fn make(
        &mut self,
        node: NodeId,
        op: String,
        children: Vec<TermId>,
        node_cost: NotNan<f64>,
    ) -> TermId {
        let term = Term {
            op,
            children: children.clone(),
        };

        if let Some(id) = self.hash_cons.get(&term) {
            return *id;
        }

        let next_id = self.nodes.len();
        if children.is_empty() {
            let next_id = self.nodes.len();
            self.nodes.push(term.clone());
            self.info.push(TermInfo {
                node,
                node_cost,
                total_cost: node_cost,
                reachable: [next_id].iter().cloned().collect(),
            });
            self.hash_cons.insert(term, next_id);
            next_id
        } else {
            let mut cost = node_cost + self.total_cost(children[0]);
            let mut reachable = Box::new(self.info[children[0]].reachable.clone());
            let next_id = self.nodes.len();
            self.nodes.push(term.clone());

            for child in children.iter().skip(1) {
                let child_cost = self.get_cost(&mut reachable, *child);
                cost += child_cost;
            }

            self.info.push(TermInfo {
                node,
                node_cost,
                total_cost: cost,
                reachable: *reachable,
            });
            self.hash_cons.insert(term, next_id);
            next_id
        }
    }

    // Recompute the cost of this term, but don't count shared
    // subterms.
    fn get_cost(&self, shared: &HashTrieSet<TermId>, id: TermId) -> NotNan<f64> {
        if shared.contains(&id) {
            NotNan::<f64>::new(0.0).unwrap()
        } else {
            let mut cost = self.term_cost(id);
            for child in &self.nodes[id].children {
                cost += self.get_cost(shared, *child);
            }
            cost
        }
    }

    pub fn term_cost(&self, id: TermId) -> NotNan<f64> {
        self.info[id].node_cost
    }

    pub fn total_cost(&self, id: TermId) -> NotNan<f64> {
        self.info[id].total_cost
    }
}

pub struct GreedyDagExtractor;
impl Extractor for GreedyDagExtractor {
    fn extract(&self, egraph: &EGraph, _roots: &[ClassId]) -> ExtractionResult {
        let mut keep_going = true;

        let mut nodes = egraph.nodes.clone();
        let mut termdag = TermDag::default();
        let mut best_in_class: HashMap<ClassId, TermId> = HashMap::default();

        let mut i = 0;
        while keep_going {
            i += 1;
            println!("iteration {}", i);
            keep_going = false;

            'node_loop: for (node_id, node) in &nodes {
                let node_cost = node.cost;
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

                let candidate = termdag.make(node_id.clone(), node.op.clone(), children, node_cost);
                let cadidate_cost = termdag.total_cost(candidate);

                if let Some(old_term) = best_in_class.get(&node.eclass) {
                    let old_cost = termdag.total_cost(*old_term);
                    if cadidate_cost < old_cost {
                        best_in_class.insert(node.eclass.clone(), candidate);
                        keep_going = true;
                    }
                } else {
                    best_in_class.insert(node.eclass.clone(), candidate);
                    keep_going = true;
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
