use egraph_serialize::{ClassId, NodeId};
use indexmap::IndexMap;

use crate::ExtractionResult;

pub fn get_term(
    egraph: &egraph_serialize::EGraph,
    result: &ExtractionResult,
) -> egraph_serialize::EGraph {
    let choices = &result.choices;
    assert!(
        egraph.root_eclasses.len() == 1,
        "expected exactly one root eclass",
    );
    let root_cid = egraph.root_eclasses[0].clone();
    let mut result_egraph = egraph_serialize::EGraph::default();
    // populate_egraph(egraph, &mut result_egraph, choices, root_cid);
    for cid in choices.keys() {
        let node = &choices[cid];
        // add the node to the result egraph
        if !result_egraph.nodes.contains_key(node) {
            let mut new_node = egraph.nodes[node].clone();
            new_node.children = egraph.nodes[node]
                .children
                .iter()
                .map(|child| choices[egraph.nid_to_cid(&child)].clone())
                .collect();

            result_egraph.add_node(node.clone(), new_node);
        }
    }

    // find number of eclasses in the original egraph
    let mut eclasses = std::collections::HashSet::new();
    for enode in egraph.nodes.values() {
        eclasses.insert(enode.eclass.clone());
    }
    result_egraph.root_eclasses = egraph.root_eclasses.clone();
    println!("eclasses in original: {}", eclasses.len());
    println!("eclasses in result: {}", result.choices.len());
    println!("original egraph size: {}", egraph.nodes.len());
    println!("result egraph size: {}", result_egraph.nodes.len());
    result_egraph
}

fn populate_egraph(
    egraph: &egraph_serialize::EGraph,
    result_egraph: &mut egraph_serialize::EGraph,
    choices: &IndexMap<ClassId, NodeId>,
    cid: ClassId,
) {
    // get the node for the eclass
    let node = &choices[&cid];
    // add the node to the result egraph
    if !result_egraph.nodes.contains_key(node) {
        result_egraph.add_node(node.clone(), egraph.nodes[node].clone());
    }
}
