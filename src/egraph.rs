use crate::*;

use indexmap::{IndexMap, IndexSet};

pub struct SimpleEGraph {
    pub roots: Vec<Id>,
    pub classes: IndexMap<String, Class>,
}
impl SimpleEGraph {
    pub fn total_number_of_nodes(&self) -> usize {
        self.classes.values().map(|c| c.nodes.len()).sum()
    }
}

impl std::ops::Index<Id> for SimpleEGraph {
    type Output = Class;

    fn index(&self, index: Id) -> &Self::Output {
        self.classes.get_index(index).unwrap().1
    }
}

#[derive(Default)]
pub struct Class {
    pub id: Id,
    pub nodes: Vec<Node>,
}

pub struct Node {
    pub op: String,
    pub cost: Cost,
    pub children: Vec<Id>,
}
impl Node {
    pub fn is_leaf(&self) -> bool {
        self.children.is_empty()
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

        // set the ids and check for empty classes
        for (i, (name, class)) in classes.iter_mut().enumerate() {
            class.id = i;
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
