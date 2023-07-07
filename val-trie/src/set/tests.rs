use crate::{
    group::Group,
    test_workloads::{self, test_hash_set, test_hash_set_collision},
    HashSet,
};

#[derive(Default, Clone, Debug)]
struct AddSum(usize);

impl Group for AddSum {
    fn add(&mut self, other: &Self) {
        self.0 = self.0.wrapping_add(other.0);
    }

    fn sub(&mut self, other: &Self) {
        self.0 = self.0.wrapping_sub(other.0);
    }

    fn inverse(&self) -> Self {
        AddSum(0usize.wrapping_sub(self.0))
    }
}

#[test]
fn inc_sum() {
    const N: usize = 1000;
    let mut set = HashSet::<usize, AddSum>::default();
    let mut to_union = HashSet::<usize, AddSum>::default();

    let mut expected = 0;

    for i in 0..N {
        set.insert_agg(i, |x| AddSum(*x));
        expected += i;
    }
    assert_eq!(expected, set.agg().0);

    for i in (N / 2)..(N * 2) {
        if i >= N {
            expected += i;
        }
        to_union.insert_agg(i, |x| AddSum(*x));
    }
    set.union_agg(&to_union, |x| AddSum(*x));
    assert_eq!(expected, set.agg().0);

    for i in 0..(N * 2) {
        set.remove_agg(&i, |x| AddSum(*x));
    }

    assert_eq!(0, set.agg().0);
}

#[test]
fn insert_remove_hash_dense() {
    test_hash_set(test_workloads::insert_remove_dense())
}

#[test]
fn insert_remove_hash_sparse() {
    test_hash_set(test_workloads::insert_remove_sparse())
}

#[test]
fn insert_remove_hash_dense_collisions() {
    test_hash_set_collision(test_workloads::insert_remove_dense())
}

#[test]
fn insert_remove_hash_sparse_collisions() {
    test_hash_set_collision(test_workloads::insert_remove_sparse())
}

#[test]
fn union_no_overlap() {
    test_hash_set(test_workloads::union_no_overlap())
}

#[test]
fn union_no_overlap_collisions() {
    test_hash_set_collision(test_workloads::union_no_overlap())
}

#[test]
fn union_all_overlap() {
    test_hash_set(test_workloads::union_all_overlap())
}

#[test]
fn union_all_overlap_collisions() {
    test_hash_set_collision(test_workloads::union_all_overlap())
}

#[test]
fn union_partial_overlap() {
    test_hash_set(test_workloads::union_partial_overlap())
}

#[test]
fn union_partial_collisions() {
    test_hash_set_collision(test_workloads::union_partial_overlap())
}
