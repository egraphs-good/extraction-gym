use super::{ClassId, NodeId};
use crate::{Cost, EPSILON_ALLOWANCE};
use std::{cmp::Ord, cmp::Ordering};

/// A valid partial solution.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Candidate<U: Copy + Ord> {
    pub choices: Vec<(ClassId<U>, NodeId<U>, Cost)>,
    cost: Cost,
}

impl<U: Copy + Ord> PartialOrd for Candidate<U> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl<U: Copy + Ord> Ord for Candidate<U> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // First by cost, then by choices (to ensure uniqueness)
        match self.cost.cmp(&other.cost) {
            Ordering::Equal => self.choices.iter().cmp(other.choices.iter()),
            ord => ord,
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
            choices: vec![(cid, nid, cost)],
            cost,
        }
    }

    pub fn contains(&self, cid: ClassId<U>) -> bool {
        self.choices.binary_search_by_key(&cid, |e| e.0).is_ok()
    }

    pub fn iter(&self) -> impl Iterator<Item = (ClassId<U>, NodeId<U>)> + '_ {
        self.choices.iter().map(|(c, n, _)| (*c, *n))
    }

    pub fn insert(&mut self, cid: ClassId<U>, nid: NodeId<U>, cost: Cost) {
        match self.choices.binary_search_by_key(&cid, |e| e.0) {
            Ok(_) => panic!("Class already in candidate"),
            Err(pos) => self.choices.insert(pos, (cid, nid, cost)),
        }
        self.cost += cost;
        debug_assert!(
            (self.cost - self.choices.iter().map(|(_, _, c)| *c).sum::<Cost>()).abs()
                < EPSILON_ALLOWANCE
        );
    }

    pub fn merge(&self, other: &Self) -> Self {
        let mut choices = Vec::with_capacity(self.choices.len() + other.choices.len());
        let mut cost = 0.into();

        let mut i = 0;
        let mut j = 0;
        while i < self.choices.len() && j < other.choices.len() {
            match self.choices[i].0.cmp(&other.choices[j].0) {
                Ordering::Less => {
                    choices.push(self.choices[i]);
                    cost += self.choices[i].2;
                    i += 1;
                }
                Ordering::Greater => {
                    choices.push(other.choices[j]);
                    cost += other.choices[j].2;
                    j += 1;
                }
                Ordering::Equal => {
                    // Duplicate class, keep the one from self
                    // TODO: Other strategy? Lowest cost?
                    choices.push(self.choices[i]);
                    cost += self.choices[i].2;
                    i += 1;
                    j += 1;
                }
            }
        }
        while i < self.choices.len() {
            choices.push(self.choices[i]);
            cost += self.choices[i].2;
            i += 1;
        }
        while j < other.choices.len() {
            choices.push(other.choices[j]);
            cost += other.choices[j].2;
            j += 1;
        }

        Self { choices, cost }
    }
}
