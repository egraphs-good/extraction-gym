use super::{ClassId, NodeId};
use crate::{Cost, EPSILON_ALLOWANCE};
use std::cmp::{Ord, Ordering};

/// A valid partial solution.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Candidate<U: Copy + Ord> {
    choices: Vec<(ClassId<U>, NodeId<U>)>,
    cost: Cost,
}

impl<U: Copy + Ord> PartialOrd for Candidate<U> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl<U: Copy + Ord> Ord for Candidate<U> {
    fn cmp(&self, other: &Self) -> Ordering {
        if (self.cost - other.cost).abs() < EPSILON_ALLOWANCE {
            // Costs are effectively equal, compare by choices to ensure uniqueness
            self.choices.cmp(&other.choices)
        } else {
            // Costs differ, compare by cost
            self.cost.cmp(&other.cost)
        }
    }
}

impl<U: Copy + Ord> Candidate<U> {
    pub fn empty() -> Self {
        Self {
            choices: Vec::new(),
            cost: 0.into(),
        }
    }

    pub fn leaf(cid: ClassId<U>, nid: NodeId<U>, cost: Cost) -> Self {
        Self {
            choices: vec![(cid, nid)],
            cost,
        }
    }

    pub fn contains(&self, cid: ClassId<U>) -> bool {
        self.choices.binary_search_by_key(&cid, |e| e.0).is_ok()
    }

    pub fn iter(&self) -> impl Iterator<Item = (ClassId<U>, NodeId<U>)> + '_ {
        self.choices.iter().copied()
    }

    pub fn cost(&self) -> Cost {
        self.cost
    }

    pub fn insert(&mut self, cid: ClassId<U>, nid: NodeId<U>, cost: Cost) {
        match self.choices.binary_search_by_key(&cid, |e| e.0) {
            Ok(_) => panic!("Class already in candidate"),
            Err(pos) => self.choices.insert(pos, (cid, nid)),
        }
        self.cost += cost;
    }

    pub fn merge(&self, other: &Self, mut costs: impl FnMut(NodeId<U>) -> Cost) -> Option<Self> {
        let mut choices = Vec::with_capacity(self.choices.len() + other.choices.len());
        let mut cost = self.cost + other.cost;

        let mut i = 0;
        let mut j = 0;
        while i < self.choices.len() && j < other.choices.len() {
            match self.choices[i].0.cmp(&other.choices[j].0) {
                Ordering::Less => {
                    choices.push(self.choices[i]);
                    i += 1;
                }
                Ordering::Greater => {
                    choices.push(other.choices[j]);
                    j += 1;
                }
                Ordering::Equal => {
                    // Duplicate class, make sure they are the same node
                    // if self.choices[i].1 != other.choices[j].1 {
                    //     return None;
                    // }

                    // Take left choice (arbitrary)
                    choices.push(self.choices[i]);
                    cost -= costs(other.choices[j].1);
                    i += 1;
                    j += 1;
                }
            }
        }
        while i < self.choices.len() {
            choices.push(self.choices[i]);
            i += 1;
        }
        while j < other.choices.len() {
            choices.push(other.choices[j]);
            j += 1;
        }

        Some(Self { choices, cost })
    }
}
