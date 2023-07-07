//! A persistent set data-structure optimized for efficient hashing/comparisons
//! of the structures themselves.
//!
//! The hash sets and maps in this crate are very similar to the HAMTs used in
//! the `im` crate. There are a few differences in these hash tables that
//! make them a better fit for use in the egglog implementation.
//!
//! # Hashing
//! These hash tables do not allow callers to customize the hash function. We
//! hard-code a globally unique hash function for all of these tables. The lack
//! of a custom hash function allows us to store intermediate hashes (a la
//! merkle trees) at intermediate nodes, which speeds up hashing and comparison
//! of unequal tables. We use a commutative hash combiner to make updates
//! efficient.
//!
//! This approach to hashing makes these table unsuitable as a general-purpose
//! data-structure, but egglog already uses a fairly weak hash function for all
//! of its tables and doesn't expose this map type externally.
//!
//! # Performance
//! Standard lookups and mutations perform comparably-but-worse in terms of time
//! when compared with `im`. While some of this gap can be addressed with
//! further optimization here, we do not expect to fully match `im` here,
//! particularly when its custom memory pools are used.
//!
//! On the other hand, the tables in this crate have much faster comparison and
//! hashing routines than their counterparts in `im`, and we have other
//! egglog-specific optimizations planned as well. The interior for our tables
//! are also more space-efficient than `im`'s: interior nodes with 8-byte
//! payloads are almost half as large.
//!
//! # Unions
//! The tables here support efficient, patricia-trie-style merge operations.
//! This is not something that is possible without a "global" hash function.
//!
//! The current representation of this data-structure does not support lengths
//! greater than 2^32. This is not a fundamental limitation, but it does save us
//! some space.

pub(crate) mod group;
pub(crate) mod map;
pub(crate) mod node;
pub(crate) mod set;
#[cfg(test)]
pub(crate) mod test_workloads;

pub use group::Group;
pub use map::HashMap;
pub use set::HashSet;
