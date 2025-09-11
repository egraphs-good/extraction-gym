use super::{ClassId, NodeId};
use crate::{Cost, EPSILON_ALLOWANCE};
use arrayvec::ArrayVec;
use std::{cmp::Ord, cmp::Ordering, collections::HashMap, hash::Hash};

/// A valid partial solution.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Candidate<U: Copy + Ord + Hash> {
    pub choices: HashMap<ClassId<U>, (NodeId<U>, Cost)>,
    cost: Cost,
}

impl<U: Copy + Ord + Hash> PartialOrd for Candidate<U> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl<U: Copy + Ord + Hash> Ord for Candidate<U> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // First by cost, then by choices (to ensure uniqueness)
        match self.cost.cmp(&other.cost) {
            Ordering::Equal => self.choices.iter().cmp(other.choices.iter()),
            ord => ord,
        }
    }
}

impl<U: Copy + Ord + Hash> Candidate<U> {
    pub fn empty() -> Self {
        Self {
            choices: HashMap::new(),
            cost: 0.into(),
        }
    }

    pub fn leaf(cid: ClassId<U>, nid: NodeId<U>, cost: Cost) -> Self {
        Self {
            choices: HashMap::from([(cid, (nid, cost))]),
            cost,
        }
    }

    pub fn contains(&self, cid: ClassId<U>) -> bool {
        self.choices.contains_key(&cid)
    }

    pub fn append(&mut self, cid: ClassId<U>, nid: NodeId<U>, cost: Cost) {
        debug_assert!(!self.contains(cid));
        self.choices.insert(cid, (nid, cost));
        self.cost += cost;

        debug_assert!(
            (self.cost - self.choices.values().map(|(_, c)| *c).sum::<Cost>()).abs()
                < EPSILON_ALLOWANCE
        );
    }

    pub fn merge(&self, other: &Self) -> Self {
        let mut new = self.clone();
        new.choices.extend(other.choices.clone());
        new.cost = new.choices.values().map(|(_, cost)| *cost).sum();
        new
    }
}
