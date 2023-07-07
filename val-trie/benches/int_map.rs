use std::hash::BuildHasherDefault;

use criterion::{black_box, criterion_group, criterion_main, Criterion, Throughput};
use hashbrown::{HashMap, HashSet};
use rand::{distributions::Uniform, prelude::Distribution, Rng};
use rustc_hash::FxHasher;

fn lookup_test_dense<M: MapLike>(c: &mut Criterion) {
    let mut group = c.benchmark_group(format!("Lookups (Dense, {})", M::NAME));
    let mut rng = rand::thread_rng();
    const BATCH_SIZE: usize = 1024;
    for map_size in [1u64 << 10, 1 << 17, 1 << 25] {
        let mut map = M::default();
        for i in 0..map_size {
            map.add(i, i);
        }

        group.throughput(Throughput::Elements(BATCH_SIZE as u64));
        group.bench_with_input(format!("hits, size={map_size}"), &map, |b, i| {
            let mut elts = Vec::with_capacity(BATCH_SIZE);
            let between = Uniform::from(0..map_size);
            for _ in 0..BATCH_SIZE {
                elts.push(between.sample(&mut rng));
            }
            b.iter(|| {
                for elt in &elts {
                    black_box(i.lookup(*elt));
                }
            })
        });
        group.bench_with_input(format!("misses, size={map_size}"), &map, |b, i| {
            let mut elts = Vec::with_capacity(BATCH_SIZE);
            let between = Uniform::from(map_size..u64::MAX);
            for _ in 0..BATCH_SIZE {
                elts.push(between.sample(&mut rng));
            }
            b.iter(|| {
                for elt in &elts {
                    black_box(i.lookup(*elt));
                }
            })
        });
    }
}

fn lookup_test_random<M: MapLike>(c: &mut Criterion) {
    let mut group = c.benchmark_group(format!("Lookups (Random, {})", M::NAME));
    let mut rng = rand::thread_rng();
    const BATCH_SIZE: usize = 1024;
    for map_size in [1u64 << 10, 1 << 17, 1 << 25] {
        // Generate `map_size` unique integers
        let mut set: HashSet<u64> = HashSet::with_capacity(map_size as usize);
        while set.len() < map_size as usize {
            set.insert(rng.gen());
        }
        let mut map = M::default();
        for i in &set {
            map.add(*i, *i);
        }

        group.throughput(Throughput::Elements(BATCH_SIZE as u64));
        group.bench_with_input(format!("hits, size={map_size}"), &map, |b, i| {
            let mut elts = Vec::with_capacity(BATCH_SIZE);
            for elt in set.iter().take(BATCH_SIZE) {
                elts.push(*elt);
            }
            b.iter(|| {
                for elt in &elts {
                    black_box(i.lookup(*elt));
                }
            })
        });
        group.bench_with_input(format!("misses, size={map_size}"), &map, |b, i| {
            let mut elts = Vec::with_capacity(BATCH_SIZE);
            for _ in 0..BATCH_SIZE {
                let mut candidate = rng.gen();
                while set.contains(&candidate) {
                    candidate = rng.gen();
                }
                elts.push(candidate);
            }
            b.iter(|| {
                for elt in &elts {
                    black_box(i.lookup(*elt));
                }
            })
        });
    }
}

fn comparison<M: MapLike>(c: &mut Criterion) {
    let mut group = c.benchmark_group(format!("Comparisons ({})", M::NAME));
    let mut rng = rand::thread_rng();
    for map_size in [1u64 << 10, 1 << 17] {
        // Generate `map_size` unique integers
        let mut set: HashSet<u64> = HashSet::with_capacity(map_size as usize);
        while set.len() < map_size as usize {
            set.insert(rng.gen());
        }
        let mut extra = rng.gen();
        while set.contains(&extra) {
            extra = rng.gen();
        }
        let mut map1 = M::default();
        let mut map2 = M::default();
        for i in &set {
            map1.add(*i, *i);
            map2.add(*i, *i);
        }
        let mut map3 = map1.clone();
        map3.remove(*set.iter().next().unwrap());
        map3.add(extra, extra);

        let mut map4 = map1.clone();
        map4.add(extra, extra);
        map4.remove(extra);

        group.bench_function(format!("equal, no sharing, size={map_size}"), |b| {
            b.iter(|| black_box(map1 == map2))
        });
        group.bench_function(format!("equal, sharing, size={map_size}"), |b| {
            b.iter(|| black_box(map1 == map4))
        });
        group.bench_function(format!("unequal, sharing, size={map_size}"), |b| {
            b.iter(|| black_box(map1 == map3))
        });
        group.bench_function(format!("unequal, no sharing, size={map_size}"), |b| {
            b.iter(|| black_box(map2 == map3))
        });
    }
}

// Benchmarks:
// * test insertions (similar to lookups: dense and sparse)

trait MapLike: Clone + Eq + Default {
    const NAME: &'static str;
    fn add(&mut self, k: u64, v: u64);
    fn lookup(&self, k: u64) -> bool;
    fn remove(&mut self, k: u64);
}

criterion_group!(
    benches,
    comparison::<HashBrown>,
    comparison::<ImMap>,
    comparison::<VHashMap>,
    lookup_test_dense::<HashBrown>,
    lookup_test_dense::<ImMap>,
    lookup_test_dense::<VHashMap>,
    lookup_test_random::<HashBrown>,
    lookup_test_random::<ImMap>,
    lookup_test_random::<VHashMap>,
);

criterion_main!(benches);

type HashBrown = HashMap<u64, u64, BuildHasherDefault<FxHasher>>;
type ImMap = im::HashMap<u64, u64, BuildHasherDefault<FxHasher>>;
type VHashMap = val_trie::HashMap<u64, u64>;

impl MapLike for HashBrown {
    const NAME: &'static str = "hashbrown";
    fn add(&mut self, k: u64, v: u64) {
        self.insert(k, v);
    }

    fn lookup(&self, k: u64) -> bool {
        self.contains_key(&k)
    }
    fn remove(&mut self, k: u64) {
        self.remove(&k);
    }
}

impl MapLike for ImMap {
    const NAME: &'static str = "im";
    fn add(&mut self, k: u64, v: u64) {
        self.insert(k, v);
    }

    fn lookup(&self, k: u64) -> bool {
        self.contains_key(&k)
    }
    fn remove(&mut self, k: u64) {
        self.remove(&k);
    }
}

impl MapLike for VHashMap {
    const NAME: &'static str = "val-hashset";
    fn add(&mut self, k: u64, v: u64) {
        self.insert(k, v);
    }

    fn lookup(&self, k: u64) -> bool {
        self.contains_key(&k)
    }
    fn remove(&mut self, k: u64) {
        self.remove(&k);
    }
}
