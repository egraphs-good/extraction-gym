use crate::test_workloads::{self, test_hash_map, test_hash_map_collision};

#[test]
fn insert_remove_hash_dense() {
    test_hash_map(test_workloads::insert_remove_dense())
}

#[test]
fn insert_remove_hash_sparse() {
    test_hash_map(test_workloads::insert_remove_sparse())
}

#[test]
fn insert_remove_hash_dense_collisions() {
    test_hash_map_collision(test_workloads::insert_remove_dense())
}

#[test]
fn insert_remove_hash_sparse_collisions() {
    test_hash_map_collision(test_workloads::insert_remove_sparse())
}

#[test]
fn union_no_overlap() {
    test_hash_map(test_workloads::union_no_overlap())
}

#[test]
fn union_no_overlap_collisions() {
    test_hash_map_collision(test_workloads::union_no_overlap())
}

#[test]
fn union_all_overlap() {
    test_hash_map(test_workloads::union_all_overlap())
}

#[test]
fn union_all_overlap_collisions() {
    test_hash_map_collision(test_workloads::union_all_overlap())
}

#[test]
fn union_partial_overlap() {
    test_hash_map(test_workloads::union_partial_overlap())
}

#[test]
fn union_partial_collisions() {
    test_hash_map_collision(test_workloads::union_partial_overlap())
}
