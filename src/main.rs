use ordered_float::NotNan;
use std::{rc::Rc, str::FromStr};

pub type Cost = NotNan<f64>;
const INFINITY: Cost = unsafe { NotNan::new_unchecked(std::f64::INFINITY) };

pub type Id = usize;

use indexmap::{IndexMap, IndexSet};

pub struct SimpleEGraph {
    pub roots: Vec<Id>,
    pub classes: IndexMap<String, Class>,
}

impl std::ops::Index<Id> for SimpleEGraph {
    type Output = Class;

    fn index(&self, index: Id) -> &Self::Output {
        &self.classes.get_index(index).unwrap().1
    }
}

#[derive(Default)]
pub struct Class {
    pub nodes: Vec<Node>,
}

pub struct Node {
    pub op: String,
    pub cost: Cost,
    pub children: Vec<Id>,
}

#[derive(Clone)]
pub struct ExtractionResult {
    pub choices: Vec<Id>,
}

impl ExtractionResult {
    pub fn new(n_classes: usize) -> Self {
        ExtractionResult {
            choices: vec![0; n_classes],
        }
    }

    pub fn tree_cost(&self, egraph: &SimpleEGraph, root: Id) -> Cost {
        let node = &egraph[root].nodes[self.choices[root]];
        let mut cost = node.cost;
        for &child in &node.children {
            cost += self.tree_cost(egraph, child);
        }
        cost
    }

    // this will loop if there are cycles
    pub fn dag_cost(&self, egraph: &SimpleEGraph, root: Id) -> Cost {
        let mut costs = vec![INFINITY; egraph.classes.len()];
        let mut todo = vec![root];
        while !todo.is_empty() {
            let i = todo.pop().unwrap();
            let node = &egraph[i].nodes[self.choices[i]];
            costs[i] = node.cost;
            for &child in &node.children {
                todo.push(child);
            }
        }
        costs.iter().filter(|c| **c != INFINITY).sum()
    }

    pub fn node_sum_cost(&self, node: &Node, costs: &[Cost]) -> Cost {
        node.cost + node.children.iter().map(|&i| costs[i]).sum::<Cost>()
    }
}

impl FromStr for SimpleEGraph {
    type Err = String;

    fn from_str<'a>(s: &'a str) -> Result<Self, Self::Err> {
        let mut classes = IndexMap::<&'a str, Class>::new();
        let get_index = |classes: &mut IndexMap<&'a str, Class>, s: &'a str| {
            let entry = classes.entry(s);
            let index = entry.index();
            entry.or_default();
            index
        };

        let mut roots = vec![];

        for (i, line) in s.lines().enumerate() {
            let i = i + 1;
            let line = line.trim();
            if line.starts_with('#') || line.is_empty() {
                if let Some(rest) = line
                    .strip_prefix("## root:")
                    .or_else(|| line.strip_prefix("## roots:"))
                {
                    for root_name in rest.split(',') {
                        let root_name = root_name.trim();
                        let root_i = get_index(&mut classes, root_name);
                        roots.push(root_i);
                    }
                }
                continue;
            }

            let mut parts = line.split(',');
            let class_name = parts
                .next()
                .ok_or_else(|| format!("missing class on line {i}"))?;

            let class_i = get_index(&mut classes, class_name);

            let cost_str = parts
                .next()
                .ok_or_else(|| format!("missing cost on line {i}"))?;
            let cost = cost_str
                .parse()
                .map_err(|e| format!("invalid cost on line {i} '{cost_str}': {e}"))?;

            let op = parts
                .next()
                .ok_or_else(|| format!("missing op on line {i}"))?;

            let mut children = vec![];
            for child_name in parts {
                let child_i = get_index(&mut classes, child_name);
                children.push(child_i);
            }

            let node = Node {
                op: op.into(),
                cost,
                children,
            };

            classes.get_index_mut(class_i).unwrap().1.nodes.push(node);
        }

        for (name, class) in &classes {
            if class.nodes.is_empty() {
                return Err(format!("class {name} is empty"));
            }
        }

        roots = roots
            .into_iter()
            .collect::<IndexSet<_>>()
            .into_iter()
            .collect();

        Ok(SimpleEGraph {
            roots,
            classes: classes
                .into_iter()
                .map(|(k, v)| (k.to_owned(), v))
                .collect(),
        })
    }
}

trait Extractor {
    fn name(&self) -> String;
    fn extract(&self, egraph: &SimpleEGraph, roots: &[Id]) -> ExtractionResult;

    fn boxed(self) -> Box<dyn Extractor>
    where
        Self: Sized + 'static,
    {
        Box::new(self)
    }
}

struct BottomUpExtractor;
impl Extractor for BottomUpExtractor {
    fn name(&self) -> String {
        "bottom-up".to_owned()
    }

    fn extract(&self, egraph: &SimpleEGraph, _roots: &[Id]) -> ExtractionResult {
        let mut result = ExtractionResult::new(egraph.classes.len());
        let mut costs = vec![INFINITY; egraph.classes.len()];
        let mut did_something = false;

        loop {
            for (i, class) in egraph.classes.values().enumerate() {
                for (node_i, node) in class.nodes.iter().enumerate() {
                    let cost = result.node_sum_cost(node, &costs);
                    if cost < costs[i] {
                        result.choices[i] = node_i;
                        costs[i] = cost;
                        did_something = true;
                    }
                }
            }

            if did_something {
                did_something = false;
            } else {
                break;
            }
        }

        result
    }
}

struct BottomUpExtractor2;
impl Extractor for BottomUpExtractor2 {
    fn name(&self) -> String {
        "bottom-up2".to_owned()
    }

    fn extract(&self, egraph: &SimpleEGraph, _roots: &[Id]) -> ExtractionResult {
        let mut result = ExtractionResult::new(egraph.classes.len());
        let mut costs = vec![INFINITY; egraph.classes.len()];
        let mut did_something = false;

        loop {
            for (i, class) in egraph.classes.values().enumerate() {
                for (node_i, node) in class.nodes.iter().enumerate() {
                    let cost = result.node_sum_cost(node, &costs);
                    if cost < costs[i] {
                        result.choices[i] = node_i;
                        costs[i] = cost;
                        did_something = true;
                    }
                }
            }

            if did_something {
                did_something = false;
            } else {
                break;
            }
        }

        result
    }
}

fn main() {
    let args = std::env::args().collect::<Vec<_>>();

    let extractors = &[BottomUpExtractor.boxed(), BottomUpExtractor2.boxed()];

    println!("file, extractor, tree, dag, time (us)");
    for filename in &args[1..] {
        let contents = std::fs::read_to_string(&filename).unwrap();
        let egraph = contents.parse::<SimpleEGraph>().unwrap();

        for extractor in extractors {
            let start_time = std::time::Instant::now();
            let result = extractor.extract(&egraph, &egraph.roots);
            let elapsed = start_time.elapsed();
            for &root in &egraph.roots {
                println!(
                    "{filename:40}\t {ext:10}\t {tree:4}\t {dag:4}\t {us:8}",
                    ext = extractor.name(),
                    tree = result.tree_cost(&egraph, root),
                    dag = result.dag_cost(&egraph, root),
                    us = elapsed.as_micros(),
                );
            }
        }
    }
}
