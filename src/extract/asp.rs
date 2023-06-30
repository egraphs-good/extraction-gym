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
    eid: u32,
    node_i: u32,
    op: String,
    cost: i32,
}

#[derive(ToSymbol)]
struct Root {
    eid: u32,
}

#[derive(ToSymbol)]
struct Child {
    eid: u32,
    node_i: u32,
    child_eid: u32,
}

const ASP_PROGRAM: &str = "
% we may choose to select this enode if we have selected that class of all it's children.
{ sel(E,I) } :- enode(E,I,_,_), selclass(Ec) : child(E,I,Ec).

% if we select an enode in an eclass, we select that eclass
selclass(E) :- sel(E,_).

% It is inconsistent for a eclass to be a root and not selected.
% This is *not* the same as saying  selclass(E) :- root(E). 
:- root(E), not selclass(E).

:- enode(E,_,_,_), #count { E,I : sel(E,I)} > 1.

#minimize { C,E,I : sel(E,I), enode(E,I,_,C) }.

#show sel/2.
";

pub struct AspExtractor;
impl Extractor for AspExtractor {
    fn extract(&self, egraph: &SimpleEGraph, _roots: &[Id]) -> ExtractionResult {
        let mut ctl = control(vec![]).expect("REASON");
        // add a logic program to the base part
        ctl.add("base", &[], ASP_PROGRAM)
            .expect("Failed to add a logic program.");

        let mut fb = FactBase::new();
        for eid in egraph.roots.iter() {
            let root = Root {
                eid: (*eid).try_into().unwrap(),
            };

            //println!("{}.", root.symbol().expect("should be symbol"));
            fb.insert(&root);
        }
        for (_i, class) in egraph.classes.values().enumerate() {
            for (node_i, node) in class.nodes.iter().enumerate() {
                let enode = Enode {
                    eid: class.id.try_into().unwrap(),
                    node_i: node_i.try_into().unwrap(),
                    op: node.op.clone(),
                    cost: node.cost.round() as i32,
                };
                //println!("{}.", enode.symbol().expect("should be symbol"));
                fb.insert(&enode);
                for child_eid in node.children.iter() {
                    let child = Child {
                        eid: class.id.try_into().unwrap(),
                        node_i: node_i.try_into().unwrap(),
                        child_eid: (*child_eid).try_into().unwrap(),
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
        let mut result = ExtractionResult::new(egraph.classes.len());
        while let Some(model) = handle.model().expect("model failed") {
            let atoms = model
                .symbols(ShowType::SHOWN)
                .expect("Failed to retrieve symbols in the model.");
            //println!("atoms length {}", atoms.len());
            for symbol in atoms {
                assert!(symbol.name().unwrap() == "sel");
                let args = symbol.arguments().unwrap();
                result.choices[args[0].number().unwrap() as usize] =
                    args[1].number().unwrap() as usize;
                //println!("{}", symbol);
            }

            //if !handle.wait(Duration::from_secs(30)) {
            //    break;
            //}
            handle.resume().expect("Failed resume on solve handle.");
        }
        handle.close().expect("Failed to close solve handle.");
        result
    }
}
