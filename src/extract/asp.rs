use super::*;
use clingo::control;

use clingo::ClingoError;
use clingo::FactBase;
use clingo::ShowType;
use clingo::Symbol;
use clingo::ToSymbol;

// An enode is identified with the eclass id and then index in that eclasses enode list.
#[derive(ToSymbol)]
struct Enode {
    eid: String,
    node_id: String,
    op: String,
    cost: i32,
}

#[derive(ToSymbol)]
struct Root {
    eid: String,
}

#[derive(ToSymbol)]
struct Child {
    node_id: String,
    child_id: String,
}

const ASP_PROGRAM: &str = "
% we may choose to select this enode if we have selected the classes of all it's children.
{ selnode(I) } :- node(I), selclass(Ec) : echild(I,Ec).

% if we select an enode in an eclass, we select that eclass
selclass(E) :- selnode(I), enode(E,I,_,_).

% It is inconsistent for a eclass to be a root and not selected.
% This is *not* the same as saying selclass(E) :- root(E).
:- root(E), not selclass(E).

:- eclass(E), #count { I : selnode(I), enode(E,I,_,_)} > 1.

#minimize { C,E,I : selnode(I), enode(E,I,_,C) }.

#show sel/2.

eclass(E) :- enode(E,_,_,_).
node(I) :- enode(_,I,_,_).
echild(I,E) :- child(I,Ic), enode(E,Ic,_,_).
sel(E,I) :- selnode(I), enode(E,I,_,_).
";

pub struct AspExtractor;
impl Extractor for AspExtractor {
    fn extract(&self, egraph: &EGraph, _roots: &[ClassId]) -> ExtractionResult {
        let mut ctl = control(vec![]).expect("REASON");
        // add a logic program to the base part
        ctl.add("base", &[], ASP_PROGRAM)
            .expect("Failed to add a logic program.");

        let mut fb = FactBase::new();
        for eid in egraph.root_eclasses.iter() {
            let root = Root {
                eid: (*eid).to_string(),
            };

            //println!("{}.", root.symbol().expect("should be symbol"));
            fb.insert(&root);
        }
        for class in egraph.classes().values() {
            for node_id in &class.nodes {
                let node = &egraph[node_id];
                let enode = Enode {
                    eid: class.id.to_string(),
                    node_id: node_id.to_string(),
                    op: node.op.clone(),
                    cost: node.cost.round() as i32,
                };
                //println!("{}.", enode.symbol().expect("should be symbol"));
                fb.insert(&enode);
                for child_id in node.children.iter() {
                    let child = Child {
                        node_id: node_id.to_string(),
                        child_id: (*child_id).to_string(),
                    };
                    //println!("{}.", child.symbol().expect("should be symbol"));
                    fb.insert(&child);
                }
            }
        }

        ctl.add_facts(&fb).expect("Failed to add factbase");
        let part = clingo::Part::new("base", vec![]).unwrap();
        let parts = vec![part];
        ctl.ground(&parts).expect("Failed to ground");
        let mut handle = ctl
            .solve(clingo::SolveMode::YIELD, &[]) // stl.optimal_models()
            .expect("Failed to solve");
        let mut result = ExtractionResult::default();
        let mut ran_once = false;
        while let Some(model) = handle.model().expect("model failed") {
            ran_once = true;
            let atoms = model
                .symbols(ShowType::SHOWN)
                .expect("Failed to retrieve symbols in the model.");
            //println!("atoms length {}", atoms.len());
            for symbol in atoms {
                assert!(symbol.name().unwrap() == "sel");
                let args = symbol.arguments().unwrap();
                result.choose(
                    args[0].string().unwrap().into(),
                    args[1].string().unwrap().into(),
                );
                //println!("{}", symbol);
            }

            //if !handle.wait(Duration::from_secs(30)) {
            //    break;
            //}
            handle.resume().expect("Failed resume on solve handle.");
        }
        assert!(ran_once);
        handle.close().expect("Failed to close solve handle.");
        result
    }
}
