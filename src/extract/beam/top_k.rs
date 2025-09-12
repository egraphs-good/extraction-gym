use arrayvec::ArrayVec;

/// A simple data structure to keep the top-k unique elements seen so far.
/// Orders elements by their `Ord` implementation, smallest first.
#[derive(Clone, Debug)]
pub struct TopK<T: Ord, const BEAM: usize>(ArrayVec<T, BEAM>);

impl<T: Ord, const BEAM: usize> TopK<T, BEAM> {
    pub fn new() -> Self {
        Self(ArrayVec::new())
    }

    pub fn singleton(candidate: T) -> Self {
        let mut result = Self::new();
        result.0.push(candidate);
        result
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn best(&self) -> Option<&T> {
        self.0.first()
    }

    pub fn cutoff(&self) -> Option<&T> {
        self.0.get(BEAM - 1)
    }

    pub fn candidates(&self) -> &[T] {
        self.0.as_slice()
    }

    /// *Warning*: Caller is responsible for maintaining the ordering invariant.
    pub fn candidates_mut(&mut self) -> &mut [T] {
        self.0.as_mut_slice()
    }

    /// Consider a new candidate, return true if kept
    pub fn consider(&mut self, item: T) -> bool {
        match self.0.binary_search(&item) {
            Ok(_) => false, // Duplicate
            Err(index) if index < BEAM => {
                if self.0.len() == BEAM {
                    self.0.pop();
                }
                self.0.insert(index, item);
                true
            }
            Err(_) => false, // Too large
        }
    }

    pub fn merge(&mut self, other: Self) -> bool {
        let mut changed = false;
        // TODO: Merge sort
        for item in other.0 {
            changed |= self.consider(item);
        }
        changed
    }
}

impl<T: Ord, const BEAM: usize> Default for TopK<T, BEAM> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Ord, const BEAM: usize> IntoIterator for TopK<T, BEAM> {
    type Item = T;
    type IntoIter = arrayvec::IntoIter<T, BEAM>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}
