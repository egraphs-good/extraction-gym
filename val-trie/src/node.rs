//! Underlying node representation for the maps.
use std::{
    fmt,
    hash::{Hash, Hasher},
    mem::{self, ManuallyDrop, MaybeUninit},
    rc::Rc,
};

use rustc_hash::FxHasher;

use crate::group::Group;

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
#[repr(u32)]
enum Kind {
    Null = 0,
    Leaf = 1,
    Collision = 2,
    Inner = 3,
}

const BITS: usize = 5;
const ARITY: usize = 1 << BITS;
type HashBits = u32;

pub(crate) trait HashItem: Clone {
    type Key: Eq + Hash;
    fn key(&self) -> &Self::Key;
}

pub(crate) struct Chunk<T, G> {
    // Rather than store an array of enums, pack the enum discriminant into a
    // bitset and then store untagged unions as children. This saves us ~2x
    // space for small Ts.
    bs: u64,
    hash: HashBits,
    len: u32,
    children: MaybeUninit<[Child<T, G>; ARITY]>,
    agg: G,
}

type Leaf<T> = T;

union Child<T, G> {
    inner: ManuallyDrop<Rc<Chunk<T, G>>>,
    leaf: ManuallyDrop<Leaf<T>>,
    collision: ManuallyDrop<Rc<CollisionNode<T, G>>>,
}

#[derive(Clone, Eq)]
struct CollisionNode<T, G> {
    hash: HashBits,
    agg: G,
    data: Vec<T>,
}

impl<T: PartialEq, G> PartialEq for CollisionNode<T, G> {
    fn eq(&self, other: &Self) -> bool {
        // O(n^2) comparison: we'll want to use a different data-structure if
        // this becomes a problem.
        if self.hash != other.hash || self.data.len() != other.data.len() {
            return false;
        }
        for l in &self.data {
            if !other.data.iter().any(|x| x == l) {
                return false;
            }
        }
        true
    }
}

impl<T, G: Group> CollisionNode<T, G> {
    fn push(&mut self, elt: T, agg: &G) {
        self.data.push(elt);
        self.agg.add(agg);
    }

    fn remove(&mut self, index: usize, agg: &G) -> T {
        let res = self.data.swap_remove(index);
        self.agg.sub(agg);
        res
    }
}

impl<T: HashItem, G: Group + Clone> Chunk<T, G> {
    pub(crate) fn agg(&self) -> &G {
        &self.agg
    }
    pub(crate) fn len(&self) -> usize {
        self.len as usize
    }

    fn has_one_child(&self) -> bool {
        let odds = self.bs & 0xAAAA_AAAA_AAAA_AAAAu64;
        let evens = self.bs & 0x5555_5555_5555_5555u64;
        if odds == 0 {
            evens.is_power_of_two()
        } else if evens == 0 {
            odds.is_power_of_two()
        } else {
            (evens | (odds >> 1)).is_power_of_two()
        }
    }

    pub(crate) fn for_each(&self, mut f: &mut impl FnMut(&T)) {
        for child in 0..ARITY {
            match self.get_kind(child) {
                Kind::Null => continue,
                Kind::Leaf => f(self.get_leaf(child)),
                Kind::Collision => {
                    let collision = self.get_collision(child);
                    collision.data.iter().for_each(&mut f);
                }
                Kind::Inner => {
                    let inner = self.get_inner(child);
                    inner.for_each(f)
                }
            }
        }
    }

    pub(crate) fn union(
        &mut self,
        other: &Chunk<T, G>,
        bits: u32,
        as_group: &mut impl FnMut(&T) -> G,
    ) {
        for i in 0..ARITY {
            match (self.get_kind(i), other.get_kind(i)) {
                (_, Kind::Null) => continue,
                (Kind::Null, Kind::Inner) => {
                    let inner = other.get_inner(i);
                    self.add_inner(i, inner);
                    self.len += inner.len;
                }
                (Kind::Null, Kind::Collision) => {
                    let collision = other.get_collision(i).clone();
                    self.add_collision(i, collision.clone());
                    self.len += collision.data.len() as u32;
                }
                (_, Kind::Leaf) => {
                    let leaf = other.get_leaf(i).clone();
                    let hash = hash_value(leaf.key());
                    self.insert(leaf, hash, bits, as_group);
                }
                (_, Kind::Collision) => {
                    let others = other.get_collision(i);
                    for elt in &others.data {
                        self.insert(elt.clone(), others.hash, bits, as_group);
                    }
                }
                (Kind::Leaf, Kind::Inner) => {
                    let mut inner = other.get_inner(i).clone();
                    let mut len_delta = 0;
                    self.replace_leaf_chunk(
                        i,
                        |leaf, as_group| {
                            let res = Rc::make_mut(&mut inner);
                            let next_bits = bits + BITS as u32;
                            let hash = hash_value(leaf.key());
                            res.insert(leaf, hash, next_bits, as_group);
                            len_delta = res.len - 1;
                            (inner.clone(), hash)
                        },
                        as_group,
                    );
                    self.len += len_delta;
                }
                (Kind::Collision, Kind::Inner) => {
                    let mut others = other.get_inner(i).clone();
                    let others_mut = Rc::make_mut(&mut others);
                    let collision = self.get_collision(i);
                    let collision_len = collision.data.len();
                    for elt in &collision.data {
                        others_mut.insert(
                            elt.clone(),
                            collision.hash,
                            bits + BITS as u32,
                            as_group,
                        );
                    }
                    self.len -= collision_len as u32;
                    self.len += others.len() as u32;
                    self.replace_collision_chunk(i, |_| others);
                }
                (Kind::Inner, Kind::Inner) => {
                    self.len += self.with_inner_mut(i, |inner_chunk| {
                        let other_inner = other.get_inner(i);
                        if !Rc::ptr_eq(inner_chunk, other_inner) {
                            // TODO: swap these and only union the smaller one
                            let start_len = inner_chunk.len;
                            Rc::make_mut(inner_chunk).union(
                                other_inner,
                                bits + BITS as u32,
                                as_group,
                            );
                            inner_chunk.len - start_len
                        } else {
                            0
                        }
                    });
                }
            }
        }
    }

    pub(crate) fn get(&self, key: &T::Key, hash: u32, bits: u32) -> Option<&T> {
        let child = Self::mask(hash, bits);
        match self.get_kind(child) {
            Kind::Null => None,
            Kind::Leaf => {
                let candidate = self.get_leaf(child);
                if candidate.key() == key {
                    Some(candidate)
                } else {
                    None
                }
            }
            Kind::Collision => {
                let collision = self.get_collision(child);
                if collision.hash != hash {
                    None
                } else {
                    collision.data.iter().find(|x| x.key() == key)
                }
            }
            Kind::Inner => {
                let inner = self.get_inner(child);
                inner.get(key, hash, bits + BITS as u32)
            }
        }
    }

    pub(crate) fn insert(
        &mut self,
        mut elt: T,
        hash: u32,
        bits: u32,
        as_group: &mut impl FnMut(&T) -> G,
    ) -> Option<T> {
        let child = Self::mask(hash, bits);
        let res = match self.get_kind(child) {
            Kind::Null => {
                let g = as_group(&elt);
                self.add_leaf(child, elt, hash, &g);
                None
            }
            Kind::Leaf => {
                let candidate = self.get_leaf(child);
                if elt.key() == candidate.key() {
                    self.with_leaf_mut(child, |prev| {
                        mem::swap(prev, &mut elt);
                    });
                    return Some(elt);
                }
                let other_hash = hash_value(candidate.key());
                if other_hash == hash {
                    // we have a hash collision
                    self.replace_leaf_collision(
                        child,
                        |prev, as_group| {
                            let mut agg = as_group(&prev);
                            agg.add(&as_group(&elt));
                            (
                                CollisionNode {
                                    hash,
                                    agg,
                                    data: vec![prev, elt],
                                },
                                hash,
                            )
                        },
                        as_group,
                    );
                    None
                } else {
                    // We need to split this node: the hashes are distinct.
                    self.replace_leaf_chunk(
                        child,
                        |other, as_group| {
                            let mut res = Chunk::<T, G>::default();
                            let next_bits = bits + BITS as u32;
                            res.insert(other, other_hash, next_bits, as_group);
                            res.insert(elt, hash, next_bits, as_group);
                            (Rc::new(res), other_hash)
                        },
                        as_group,
                    );
                    None
                }
            }
            Kind::Collision => {
                let collision = self.get_collision(child);
                if collision.hash == hash {
                    // Another collision!
                    self.with_collision_mut(child, |c| {
                        if let Some(prev) = c.data.iter_mut().find(|x| x.key() == elt.key()) {
                            mem::swap(prev, &mut elt);
                            Some(elt)
                        } else {
                            let g = as_group(&elt);
                            c.push(elt, &g);
                            None
                        }
                    })
                } else {
                    // Split this node and reinsert.
                    self.replace_collision_chunk(child, |c| {
                        let next_bits = bits + BITS as u32;
                        let next_child = Self::mask(c.hash, next_bits);
                        let mut res = Chunk::default();
                        res.len = c.data.len() as u32;
                        res.add_collision(next_child, c);
                        res.insert(elt, hash, next_bits, as_group);
                        Rc::new(res)
                    });
                    None
                }
            }
            Kind::Inner => self.with_inner_mut(child, |inner| {
                Rc::make_mut(inner).insert(elt, hash, bits + BITS as u32, as_group)
            }),
        };
        self.len += if res.is_none() { 1 } else { 0 };
        res
    }

    pub(crate) fn remove(
        &mut self,
        key: &T::Key,
        hash: u32,
        bits: u32,
        as_group: &mut impl FnMut(&T) -> G,
    ) -> Option<T> {
        let child = Self::mask(hash, bits);
        let res = match self.get_kind(child) {
            Kind::Null => None,
            Kind::Leaf => self.remove_leaf(child, |leaf| (leaf.key() == key, hash), as_group),
            Kind::Collision => {
                let collision = self.get_collision(child);
                if collision.hash != hash {
                    return None;
                }
                let (to_remove_ix, to_remove) = collision
                    .data
                    .iter()
                    .enumerate()
                    .find(|(_, x)| x.key() == key)?;

                let to_remove_agg = as_group(to_remove);

                if collision.data.len() == 2 {
                    // replace the collision with a leaf.
                    Some(self.replace_collision_leaf(child, |mut collision| {
                        let res = collision.remove(to_remove_ix, &to_remove_agg);
                        let leaf = collision.data.pop().unwrap();
                        let g = as_group(&leaf);
                        (res, leaf, collision.hash, g)
                    }))
                } else {
                    // Remove the element from the node
                    self.with_collision_mut(child, |collision| {
                        Some(collision.remove(to_remove_ix, &to_remove_agg))
                    })
                }
            }
            Kind::Inner => {
                let (res, try_promote, bs) = self.with_inner_mut(child, |inner| {
                    let res = Rc::make_mut(inner).remove(key, hash, bits + BITS as u32, as_group);
                    (res, inner.has_one_child(), inner.bs)
                });
                if try_promote {
                    self.replace_chunk_with_child(child, bs.trailing_zeros() as usize / 2)
                }
                res
            }
        };
        self.len -= if res.is_some() { 1 } else { 0 };
        res
    }

    #[inline(always)]
    fn mask(hash: u32, bits: u32) -> usize {
        #[cfg(test)]
        {
            if let Some(res) = hash.checked_shr(bits) {
                res as usize % ARITY
            } else {
                panic!("overflow in mask shift: bits = {}, hash = {}", bits, hash);
            }
        }
        #[cfg(not(test))]
        {
            (hash >> bits) as usize % ARITY
        }
    }

    /// Remove the given hashcode from the node's digest.
    fn remove_summary(&mut self, hc: u32, g: &G) {
        self.hash ^= hc;
        self.agg.sub(g);
    }

    /// Add the given hashcode to the node's digest.
    fn add_summary(&mut self, hc: u32, g: &G) {
        self.hash ^= hc;
        self.agg.add(g);
    }

    fn add_leaf(&mut self, i: usize, leaf: Leaf<T>, hash: HashBits, g: &G) {
        assert_eq!(self.get_kind(i), Kind::Null);
        assert!(i < ARITY);
        unsafe {
            self.add_summary(hash, g);
            self.child_ptr_mut(i).write(Child {
                leaf: ManuallyDrop::new(leaf),
            })
        }
        self.set_kind(i, Kind::Leaf);
    }

    fn add_collision(&mut self, i: usize, collision: Rc<CollisionNode<T, G>>) {
        assert_eq!(self.get_kind(i), Kind::Null);
        assert!(i < ARITY);
        unsafe {
            self.add_summary(collision.hash, &collision.agg);
            self.child_ptr_mut(i).write(Child {
                collision: ManuallyDrop::new(collision),
            })
        }
        self.set_kind(i, Kind::Collision);
    }

    fn add_inner(&mut self, i: usize, inner: &Rc<Chunk<T, G>>) {
        assert_eq!(self.get_kind(i), Kind::Null);
        assert!(i < ARITY);
        unsafe {
            self.add_summary(inner.hash, &inner.agg);
            self.child_ptr_mut(i).write(Child {
                inner: ManuallyDrop::new(inner.clone()),
            })
        }
        self.set_kind(i, Kind::Inner);
    }

    fn replace_leaf_chunk<F>(
        &mut self,
        i: usize,
        chunk: impl FnOnce(Leaf<T>, &mut F) -> (Rc<Chunk<T, G>>, HashBits),
        as_group: &mut F,
    ) where
        F: FnMut(&T) -> G,
    {
        assert_eq!(self.get_kind(i), Kind::Leaf);
        assert!(i < ARITY);
        let (prev_hash, new_hash, prev_summary, new_summary) = unsafe {
            let ptr = self.child_ptr_mut(i);
            let leaf = ManuallyDrop::into_inner(ptr.read().leaf);
            let summary = as_group(&leaf);
            let (inner, prev_hash) = chunk(leaf, as_group);
            let new_hash = inner.hash;
            let new_summary = inner.agg.clone();
            ptr.write(Child {
                inner: ManuallyDrop::new(inner),
            });
            (prev_hash, new_hash, summary, new_summary)
        };
        self.remove_summary(prev_hash, &prev_summary);
        self.add_summary(new_hash, &new_summary);
        self.set_kind(i, Kind::Inner);
    }

    fn replace_collision_chunk(
        &mut self,
        i: usize,
        chunk: impl FnOnce(Rc<CollisionNode<T, G>>) -> Rc<Chunk<T, G>>,
    ) {
        assert_eq!(self.get_kind(i), Kind::Collision);
        assert!(i < ARITY);
        unsafe {
            let ptr = self.child_ptr_mut(i);
            let collision_ptr = ManuallyDrop::into_inner(ptr.read().collision);
            self.remove_summary(collision_ptr.hash, &collision_ptr.agg);
            let inner = chunk(collision_ptr);
            self.add_summary(inner.hash, &inner.agg);
            // re-borrow
            let ptr = self.child_ptr_mut(i);
            ptr.write(Child {
                inner: ManuallyDrop::new(inner),
            });
        }
        self.set_kind(i, Kind::Inner);
    }

    fn replace_collision_leaf<R>(
        &mut self,
        i: usize,
        leaf: impl FnOnce(CollisionNode<T, G>) -> (R, Leaf<T>, HashBits, G),
    ) -> R {
        assert_eq!(self.get_kind(i), Kind::Collision);
        assert!(i < ARITY);
        unsafe {
            let ptr = self.child_ptr_mut(i);
            let collision = ManuallyDrop::into_inner(ptr.read().collision);
            self.remove_summary(collision.hash, &collision.agg);
            let (res, leaf, leaf_hash, new_summary) = leaf(unwrap_or_clone(collision));
            self.add_summary(leaf_hash, &new_summary);
            // re-borrow
            let ptr = self.child_ptr_mut(i);
            ptr.write(Child {
                leaf: ManuallyDrop::new(leaf),
            });
            self.set_kind(i, Kind::Leaf);
            res
        }
    }

    fn replace_chunk_with_child(&mut self, i: usize, child: usize) {
        assert_eq!(self.get_kind(i), Kind::Inner);
        unsafe {
            // First, check if the grandchild is another interior node. If it
            // is, stop: we can't collapse interior paths for this trie.
            let ptr = self.child_ptr_mut(i);
            let chunk_mut = &mut (&mut *ptr).inner;
            let grandchild_kind = chunk_mut.get_kind(child);
            if let Kind::Inner = grandchild_kind {
                // Abort!
                return;
            }

            // We have some kind of 'leaf': promote the grandchild.

            let mut chunk = ManuallyDrop::into_inner(ptr.read().inner);
            let grandchild_kind = chunk.get_kind(child);
            let chunk_mut = Rc::make_mut(&mut chunk);
            let grandchild = chunk_mut.child_ptr_mut(child).read();
            // Null out the elements of `chunk`: we're going to drop it.
            chunk_mut.set_kind(child, Kind::Null);
            chunk_mut.len = 0;

            ptr.write(grandchild);
            self.set_kind(i, grandchild_kind);

            // Don't bother updating the hash: one-element chunks will have the
            // same hash as their children.
        }
    }

    fn replace_leaf_collision<F>(
        &mut self,
        i: usize,
        collision: impl FnOnce(Leaf<T>, &mut F) -> (CollisionNode<T, G>, HashBits),
        as_group: &mut F,
    ) where
        F: FnMut(&T) -> G,
    {
        assert_eq!(self.get_kind(i), Kind::Leaf);
        assert!(i < ARITY);
        unsafe {
            let ptr = self.child_ptr_mut(i);
            let leaf = ManuallyDrop::into_inner(ptr.read().leaf);
            let prev_summary = as_group(&leaf);
            let (collision, leaf_hash) = collision(leaf, as_group);
            self.remove_summary(leaf_hash, &prev_summary);
            self.add_summary(collision.hash, &collision.agg);
            // re-borrow
            let ptr = self.child_ptr_mut(i);
            ptr.write(Child {
                collision: ManuallyDrop::new(Rc::new(collision)),
            });
        }
        self.set_kind(i, Kind::Collision);
    }

    // "setters" are CPS-d so we can properly adjust hashcodes.

    fn remove_leaf(
        &mut self,
        i: usize,
        f: impl FnOnce(&Leaf<T>) -> (bool, HashBits),
        as_group: &mut impl FnMut(&T) -> G,
    ) -> Option<Leaf<T>> {
        assert_eq!(self.get_kind(i), Kind::Leaf);
        assert!(i < 32);
        unsafe {
            let ptr = self.child_ptr_mut(i);
            let leaf = &(&*ptr).leaf;
            let summary = as_group(leaf);
            let (remove, hash) = f(leaf);
            if !remove {
                return None;
            }
            // remove

            // Borrow of `leaf` is over

            // Safe because remove_hash only touches the hash code
            self.remove_summary(hash, &summary);
            self.set_kind(i, Kind::Null);
            // Re-borrow
            let ptr = self.child_ptr_mut(i);
            // Safe because `ptr` is no longer reachable with Kind::Null.
            Some(ManuallyDrop::into_inner(ptr.read().leaf))
        }
    }

    fn with_leaf_mut<R>(&mut self, i: usize, f: impl FnOnce(&mut Leaf<T>) -> R) -> R {
        assert_eq!(self.get_kind(i), Kind::Leaf);
        assert!(i < 32);
        let leaf: &mut Leaf<T> = unsafe {
            let child = &mut *self.child_ptr_mut(i);
            &mut child.leaf
        };
        // We don't both updating the hash here. It should not change.
        let _old_hash = hash_value(leaf.key());
        let res = f(leaf);
        debug_assert_eq!(_old_hash, hash_value(leaf.key()));
        res
    }

    fn with_collision_mut<R>(
        &mut self,
        i: usize,
        f: impl FnOnce(&mut CollisionNode<T, G>) -> R,
    ) -> R {
        assert_eq!(self.get_kind(i), Kind::Collision);
        assert!(i < 32);
        let node: &mut CollisionNode<T, G> = unsafe {
            let child = &mut *self.child_ptr_mut(i);
            Rc::make_mut(&mut child.collision)
        };
        self.remove_summary(node.hash, &node.agg);
        let res = f(node);
        self.add_summary(node.hash, &node.agg);
        res
    }

    fn with_inner_mut<R>(&mut self, i: usize, f: impl FnOnce(&mut Rc<Chunk<T, G>>) -> R) -> R {
        assert_eq!(self.get_kind(i), Kind::Inner);
        assert!(i < 32);
        let node: &mut Rc<Chunk<T, G>> = unsafe {
            let child = &mut *self.child_ptr_mut(i);
            &mut child.inner
        };
        let prev_hash = node.hash;
        let prev_agg = node.agg.clone();
        let res = f(node);
        // What is this prev_hash, and re-borrow business?
        // We'd like to simply do self.remove_hash(prev_hash); f(node); // self.add_hash(node.hash);
        // But that violates the stacked borrowed rules implemented by miri.
        let node: &mut Rc<Chunk<T, G>> = unsafe {
            let child = &mut *self.child_ptr_mut(i);
            &mut child.inner
        };
        self.remove_summary(prev_hash, &prev_agg);
        self.add_summary(node.hash, &node.agg);
        res
    }

    fn set_kind(&mut self, i: usize, k: Kind) {
        debug_assert!(i < 32);
        #[inline(always)]
        fn set_bit(bs: &mut u64, i: u32) {
            *bs |= 1 << i;
        }
        #[inline(always)]
        fn clear_bit(bs: &mut u64, i: u32) {
            *bs &= !(1 << i);
        }
        let i = i as u32;
        match k {
            Kind::Null => {
                debug_assert_eq!(k as u32, 0);
                clear_bit(&mut self.bs, 2 * i);
                clear_bit(&mut self.bs, 2 * i + 1);
            }
            Kind::Leaf => {
                debug_assert_eq!(k as u32, 1);
                set_bit(&mut self.bs, 2 * i);
                clear_bit(&mut self.bs, 2 * i + 1);
            }
            Kind::Collision => {
                debug_assert_eq!(k as u32, 2);
                clear_bit(&mut self.bs, 2 * i);
                set_bit(&mut self.bs, 2 * i + 1);
            }
            Kind::Inner => {
                debug_assert_eq!(k as u32, 3);
                set_bit(&mut self.bs, 2 * i);
                set_bit(&mut self.bs, 2 * i + 1);
            }
        }
        debug_assert_eq!(self.get_kind(i as usize), k);
    }
}

impl<T, G> Chunk<T, G> {
    fn get_kind(&self, i: usize) -> Kind {
        debug_assert!(i < 32);
        match (self.bs >> (i * 2)) % 4 {
            0 => Kind::Null,
            1 => Kind::Leaf,
            2 => Kind::Collision,
            3 => Kind::Inner,
            _ => unreachable!(),
        }
    }

    unsafe fn child_ptr(&self, i: usize) -> *const Child<T, G> {
        (self.children.as_ptr() as *const Child<T, G>).add(i)
    }

    unsafe fn child_ptr_mut(&mut self, i: usize) -> *mut Child<T, G> {
        (self.children.as_mut_ptr() as *mut Child<T, G>).add(i)
    }

    fn get_leaf(&self, i: usize) -> &T {
        assert_eq!(self.get_kind(i), Kind::Leaf);
        assert!(i < ARITY);
        unsafe {
            let child = &*self.child_ptr(i);
            &child.leaf
        }
    }

    fn get_collision(&self, i: usize) -> &Rc<CollisionNode<T, G>> {
        assert_eq!(self.get_kind(i), Kind::Collision);
        assert!(i < ARITY);
        unsafe {
            let child = &*self.child_ptr(i);
            &child.collision
        }
    }

    fn get_inner(&self, i: usize) -> &Rc<Chunk<T, G>> {
        assert_eq!(self.get_kind(i), Kind::Inner);
        assert!(i < ARITY);
        unsafe {
            let child = &*self.child_ptr(i);
            &child.inner
        }
    }
}

// -- trait implementations --

impl<T, G> Hash for Chunk<T, G> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.hash.hash(state)
    }
}

impl<T, G: Default> Default for Chunk<T, G> {
    fn default() -> Chunk<T, G> {
        Chunk {
            bs: 0,
            hash: 0,
            len: 0,
            children: MaybeUninit::uninit(),
            agg: Default::default(),
        }
    }
}

impl<T: PartialEq, G> PartialEq for Chunk<T, G> {
    fn eq(&self, other: &Self) -> bool {
        if self.hash != other.hash || self.bs != other.bs || self.len != other.len {
            return false;
        }
        for i in 0..ARITY {
            match self.get_kind(i) {
                Kind::Null => {}
                Kind::Leaf => {
                    if self.get_leaf(i) != other.get_leaf(i) {
                        return false;
                    }
                }
                Kind::Collision => {
                    let l = self.get_collision(i);
                    let r = other.get_collision(i);
                    if Rc::ptr_eq(l, r) {
                        continue;
                    }
                    if l != r {
                        return false;
                    }
                }
                Kind::Inner => {
                    let inner_l = self.get_inner(i);
                    let inner_r = other.get_inner(i);
                    if !Rc::ptr_eq(inner_l, inner_r) && inner_l != inner_r {
                        return false;
                    }
                }
            }
        }
        true
    }
}

impl<T: Clone, G: Clone> Clone for Chunk<T, G> {
    fn clone(&self) -> Chunk<T, G> {
        let mut res = Chunk {
            bs: self.bs,
            hash: self.hash,
            len: self.len,
            children: MaybeUninit::uninit(),
            agg: self.agg.clone(),
        };

        for i in 0..ARITY {
            let ptr = unsafe { res.child_ptr_mut(i) };
            let child = match self.get_kind(i) {
                Kind::Null => continue,
                Kind::Leaf => Child {
                    leaf: ManuallyDrop::new(self.get_leaf(i).clone()),
                },
                Kind::Collision => Child {
                    collision: ManuallyDrop::new(self.get_collision(i).clone()),
                },
                Kind::Inner => Child {
                    inner: ManuallyDrop::new(self.get_inner(i).clone()),
                },
            };
            unsafe { ptr.write(child) }
        }
        res
    }
}
impl<T: Eq, G> Eq for Chunk<T, G> {}

impl<T, G> Drop for Chunk<T, G> {
    fn drop(&mut self) {
        for i in 0..ARITY {
            match self.get_kind(i) {
                Kind::Null => continue,
                Kind::Leaf => unsafe {
                    let child = &mut *self.child_ptr_mut(i);
                    ManuallyDrop::drop(&mut child.leaf);
                },
                Kind::Collision => unsafe {
                    let child = &mut *self.child_ptr_mut(i);
                    ManuallyDrop::drop(&mut child.collision);
                },
                Kind::Inner => unsafe {
                    let child = &mut *self.child_ptr_mut(i);
                    ManuallyDrop::drop(&mut child.inner);
                },
            }
        }
    }
}

pub(crate) fn hash_value(k: &impl Hash) -> HashBits {
    let mut hasher = FxHasher::default();
    k.hash(&mut hasher);
    hasher.finish() as HashBits
}

impl<T: fmt::Debug, G> fmt::Debug for Chunk<T, G> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Chunk{{")?;
        write!(f, "len: {:?}, ", self.len)?;
        write!(f, "hash: {:?}, ", self.hash)?;
        write!(f, "bs: {:064b}, ", self.bs)?;
        writeln!(f, "children: [")?;
        for i in 0..ARITY {
            let is_last = i == ARITY - 1;
            let suffix = if is_last { "]" } else { ", " };
            match self.get_kind(i) {
                Kind::Null => write!(f, "Null{suffix}")?,
                Kind::Leaf => write!(f, "<{:?}>{suffix}", self.get_leaf(i))?,
                Kind::Collision => {
                    let collision = self.get_collision(i);
                    write!(
                        f,
                        "<hash:{:?}, {:?}>{suffix}",
                        collision.hash, &collision.data
                    )?;
                }
                Kind::Inner => {
                    write!(f, "{:?}{suffix}", self.get_inner(i))?;
                }
            }
        }
        write!(f, "}}")
    }
}

fn unwrap_or_clone<T: Clone>(rc: Rc<T>) -> T {
    Rc::try_unwrap(rc).unwrap_or_else(|mut ptr| {
        Rc::make_mut(&mut ptr);
        if let Ok(x) = Rc::try_unwrap(ptr) {
            x
        } else {
            unreachable!()
        }
    })
}
