//! Hash sets optimized to represent values in egglog.
use std::{
    fmt,
    hash::{Hash, Hasher},
    mem,
    rc::Rc,
};

use crate::{
    group::Group,
    node::{hash_value, Chunk, HashItem},
};

#[cfg(test)]
mod tests;

/// A persistent set data-structure.
#[derive(Debug, Clone)]
pub struct HashSet<T, G = ()> {
    len: usize,
    node: Rc<Chunk<Inline<T>, G>>,
}

impl<T, G: Group> Default for HashSet<T, G> {
    fn default() -> Self {
        HashSet {
            len: 0,
            node: Default::default(),
        }
    }
}

impl<T: Hash + Eq + Clone, G: Group + Clone> HashSet<T, G> {
    /// Get the group element associated with the sum of the elements of the set.
    pub fn agg(&self) -> &G {
        self.node.agg()
    }

    /// Apply `f` to each of the elements in the set. The order is unspecified.
    pub fn for_each(&self, mut f: impl FnMut(&T)) {
        self.node.for_each(&mut |x| f(&x.0))
    }

    /// The number of elements in the set.
    pub fn len(&self) -> usize {
        debug_assert_eq!(self.node.len(), self.len);
        self.len
    }

    /// Whether or not the set is empty.
    pub fn is_empty(&self) -> bool {
        debug_assert_eq!(self.node.len(), self.len);
        self.len() == 0
    }

    /// Whether or not the set contains `t`.
    pub fn contains(&self, t: &T) -> bool {
        debug_assert_eq!(self.node.len(), self.len);
        let hash = hash_value(t);
        self.node.get(t, hash, 0).is_some()
    }

    /// Add all elements from `other` to the current set, using `as_group` to
    /// map any existing values into the group `G` for updates to the aggregate.
    pub fn union_agg(&mut self, other: &HashSet<T, G>, mut as_group: impl FnMut(&T) -> G) {
        debug_assert_eq!(self.node.len(), self.len);
        debug_assert_eq!(other.node.len(), other.len);
        if Rc::ptr_eq(&self.node, &other.node) {
            return;
        }
        if self.len() < other.len() {
            let mut other = other.clone();
            mem::swap(self, &mut other);
            return self.union_agg(&other, as_group);
        }
        let new_node = Rc::make_mut(&mut self.node);
        new_node.union(&other.node, 0, &mut |inline| as_group(&inline.0));
        self.len = self.node.len();
    }

    /// Add `t` to the current set, using `as_group` to map it into the group
    /// `G` for updates to the aggregate.
    pub fn insert_agg(&mut self, t: T, mut as_group: impl FnMut(&T) -> G) -> bool {
        debug_assert_eq!(self.node.len(), self.len);
        let hash = hash_value(&t);
        let res = Rc::make_mut(&mut self.node)
            .insert(Inline(t), hash, 0, &mut |inline| as_group(&inline.0))
            .is_none();
        self.len += res as usize;
        debug_assert_eq!(self.node.len(), self.len);
        res
    }

    /// Remove `t` from the current set, using `as_group` to map it into the
    /// group `G` for updates to the aggregate.
    pub fn remove_agg(&mut self, t: &T, mut as_group: impl FnMut(&T) -> G) -> bool {
        debug_assert_eq!(self.node.len(), self.len);
        let hash = hash_value(&t);
        let res = Rc::make_mut(&mut self.node)
            .remove(t, hash, 0, &mut |inline| as_group(&inline.0))
            .is_some();
        self.len -= res as usize;
        debug_assert_eq!(self.node.len(), self.len);
        res
    }
}

impl<T: Hash + Eq + Clone> HashSet<T> {
    pub fn union(&mut self, other: &HashSet<T>) {
        self.union_agg(other, |_| ())
    }

    /// Insert `t` into the set. Returns whether or not a new element was inserted.
    pub fn insert(&mut self, t: T) -> bool {
        self.insert_agg(t, |_| ())
    }

    /// Remove `t` from the set, if it is there. Returns whether or not `t` was
    /// present.
    pub fn remove(&mut self, t: &T) -> bool {
        self.remove_agg(t, |_| ())
    }
}

impl<T: PartialEq> PartialEq for HashSet<T> {
    fn eq(&self, other: &HashSet<T>) -> bool {
        self.len == other.len && (Rc::ptr_eq(&self.node, &other.node) || self.node == other.node)
    }
}

impl<T: Eq> Eq for HashSet<T> {}

impl<T> Hash for HashSet<T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.len.hash(state);
        self.node.hash(state)
    }
}

#[derive(Copy, Clone, PartialEq, Eq)]
struct Inline<T>(T);

impl<T: fmt::Debug> fmt::Debug for Inline<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl<T: Hash + Eq + Clone> HashItem for Inline<T> {
    type Key = T;
    fn key(&self) -> &T {
        &self.0
    }
}
