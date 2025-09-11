use arrayvec::ArrayVec;

/// A simple data structure to keep the top-k unique elements seen so far.
/// Orders elements by their `Ord` implementation, smallest first.
#[derive(Clone, Debug)]
pub struct TopK<T: Ord> {
    k: usize,
    data: Vec<T>,
}

impl<T: Ord> TopK<T> {
    pub fn new(k: usize) -> Self {
        Self {
            k,
            data: Vec::with_capacity(k),
        }
    }

    pub fn empty() -> Self {
        Self { k: 0, data: vec![] }
    }

    pub fn singleton(candidate: T) -> Self {
        Self {
            k: 1,
            data: vec![candidate],
        }
    }

    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    pub fn best(&self) -> Option<&T> {
        self.data.first()
    }

    pub fn cutoff(&self) -> Option<&T> {
        self.data.get(self.k - 1)
    }

    pub fn candidates(&self) -> &[T] {
        &self.data
    }

    /// *Warning*: Caller is responsible for maintaining the ordering invariant.
    pub fn candidates_mut(&mut self) -> &mut [T] {
        &mut self.data
    }

    /// Consider a new candidate, return true if kept
    pub fn consider(&mut self, item: T) -> bool {
        match self.data.binary_search(&item) {
            Ok(_) => false, // Duplicate
            Err(index) if index < self.k => {
                if self.data.len() == self.k {
                    self.data.pop();
                }
                self.data.insert(index, item);
                true
            }
            Err(_) => false, // Too large
        }
    }

    pub fn merge(&mut self, other: Self) -> bool {
        let mut changed = false;
        for item in other.data {
            changed |= self.consider(item);
        }
        changed
    }
}
