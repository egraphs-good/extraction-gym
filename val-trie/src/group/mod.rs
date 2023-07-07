//! Basic definitions for groups. Groups are the kind of aggregation that we
//! efficiently support for the sets and maps defined in this crate.

/// A group is a type with an associative and (in this case) commutative binary
/// operation. This type must have a distinguished identity element, and all
/// elements must have an inverse.
pub trait Group: Sized + Default {
    fn identity() -> Self {
        Self::default()
    }
    fn inverse(&self) -> Self;
    fn add(&mut self, other: &Self);
    fn sub(&mut self, other: &Self) {
        self.add(&other.inverse());
    }
}

impl<S: Group, T: Group> Group for (S, T) {
    fn identity() -> Self {
        (S::identity(), T::identity())
    }

    fn inverse(&self) -> Self {
        (self.0.inverse(), self.1.inverse())
    }

    fn add(&mut self, other: &Self) {
        self.0.add(&other.0);
        self.1.add(&other.1);
    }

    fn sub(&mut self, other: &Self) {
        self.0.sub(&other.0);
        self.1.sub(&other.1);
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, Default)]
pub(crate) struct XorU32(u32);

impl Group for XorU32 {
    fn inverse(&self) -> Self {
        *self
    }

    fn add(&mut self, other: &Self) {
        self.0 ^= other.0;
    }
}

impl Group for () {
    fn identity() {}

    fn inverse(&self) {}

    fn add(&mut self, _other: &Self) {}
}

/// Types that can be "projected" into a group.
pub trait AsGroup<G: Group> {
    fn as_group(&self) -> G;
}

impl<T> AsGroup<()> for T {
    fn as_group(&self) {}
}
