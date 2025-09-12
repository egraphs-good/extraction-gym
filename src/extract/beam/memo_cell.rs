//! Unified interface over `RwLock` and `RefCell` for memoization.

use parking_lot::{RwLock, RwLockReadGuard, RwLockWriteGuard};
use std::ops::{Deref, DerefMut};

pub trait MemoCell<T> {
    type Read<'a>: Deref<Target = T>
    where
        Self: 'a;
    type Write<'a>: Deref<Target = T> + DerefMut
    where
        Self: 'a;

    fn read(&self) -> Self::Read<'_>;
    fn write(&self) -> Self::Write<'_>;
}

impl<T: Default> MemoCell<T> for RwLock<T> {
    type Read<'a>
        = RwLockReadGuard<'a, T>
    where
        Self: 'a;
    type Write<'a>
        = RwLockWriteGuard<'a, T>
    where
        Self: 'a;

    fn read(&self) -> Self::Read<'_> {
        self.read()
    }
    fn write(&self) -> Self::Write<'_> {
        self.write()
    }
}

impl<T: Default> MemoCell<T> for std::cell::RefCell<T> {
    type Read<'a>
        = std::cell::Ref<'a, T>
    where
        Self: 'a;
    type Write<'a>
        = std::cell::RefMut<'a, T>
    where
        Self: 'a;

    fn read(&self) -> Self::Read<'_> {
        self.borrow()
    }
    fn write(&self) -> Self::Write<'_> {
        self.borrow_mut()
    }
}
