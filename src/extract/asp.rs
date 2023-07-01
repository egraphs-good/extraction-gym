use super::*;
use std::io::Write;
use std::process::{Command, Stdio};

pub struct ClingoExtractor;

fn node_var(class: Id, node: usize) -> String {
    format!("n_{}_{}", class, node)
}

fn class_var(class: Id) -> String {
    format!("c_{}", class)
}

impl Extractor for ClingoExtractor {
    fn extract(&self, egraph: &SimpleEGraph, roots: &[Id]) -> ExtractionResult {
        let mut constraints = vec![];
        for (_, c) in &egraph.classes {
            for (i, n) in c.nodes.iter().enumerate() {
                // pick some leafs
                if n.is_leaf() {
                    constraints.push(format!("{{ pick({}) }}.", node_var(c.id, i)));
                }

                // add cost
                constraints.push(format!("cost({}, {}).", node_var(c.id, i), n.cost));

                // if pick a node, pick its class
                constraints.push(format!(
                    "pick({}) :- pick({}).",
                    class_var(c.id),
                    node_var(c.id, i),
                ));

                // if pick all children, may pick node
                let ch_vars: Vec<_> = n
                    .children
                    .iter()
                    .map(|ch| format!("pick({})", class_var(*ch)))
                    .collect();

                constraints.push(format!(
                    "{{ pick({}) }} :- {}.",
                    node_var(c.id, i),
                    ch_vars.join(", ")
                ));
            }
        }

        //  must pick roots
        for root in roots {
            constraints.push(format!(":- not pick({}).", class_var(*root)));
        }

        // minimize cost

        constraints.push("#minimize { C, N : pick(N), cost(N, C) }.".to_string());

        let encoding = constraints.join("\n");
        // println!("{}", &encoding);

        {
            let mut cmd = Command::new("clingo");

            cmd.arg("-q2,1");

            cmd.stdin(Stdio::piped());

            // Spawn the command process
            let mut child = match cmd.spawn() {
                Ok(child) => child,
                Err(e) => panic!("Failed to execute command: {}", e),
            };

            {
                let stdin = child.stdin.as_mut().expect("Failed to open stdin");

                // Write the string to stdin
                if let Err(e) = stdin.write_all(encoding.as_bytes()) {
                    panic!("Failed to write to stdin: {}", e);
                }

                // Close stdin explicitly to signal the end of input
            }

            // Wait for the command to finish
            let output = match child.wait_with_output() {
                Ok(output) => output,
                Err(e) => panic!("Failed to wait for command: {}", e),
            };
            dbg!(output);
        }

        ExtractionResult { choices: vec![] }
    }
}
