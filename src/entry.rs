use std::ptr;
use std::sync::Arc;
use std::sync::atomic::Ordering::*;
use std::{ops::Deref, ptr::NonNull, sync::atomic::AtomicUsize};

use crate::Pool;

/// An entry in the pool.
///
/// `Entry` holds a reference pointer to an item from the pool and a reference
/// to the [`Pool`].
/// When the last `Entry` is dropped, the item is returned to the pool.
///
pub struct Entry<'a, T: Default> {
    // When the last reference is dropped, the item is returned to the pool.
    // `item` is always `Some` before the last reference is dropped.
    pub(crate) item: Option<Prc<T>>,
    pub(crate) pool: &'a Pool<T>,
}

impl<'a, T: Default> Clone for Entry<'a, T> {
    /// Makes a clone of the `Entry` that points to the same allocation.
    fn clone(&self) -> Self {
        Self {
            item: self.item.clone(),
            pool: self.pool,
        }
    }
}

impl<'a, T: Default> Drop for Entry<'a, T> {
    fn drop(&mut self) {
        if self.item.as_ref().is_some_and(|i| i.dec_ref() == 1) {
            // This was the last reference, return to the pool.
            let item = self.item.take().unwrap();
            self.pool.recycle(item);
        }
    }
}

impl<'a, T: Default> Deref for Entry<'a, T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        self.item.as_ref().unwrap()
    }
}

impl<'a, T: Default> Entry<'a, T> {
    /// Get reference to the inner item.
    pub fn get(&self) -> &T {
        &self
    }

    /// Get mutable reference to the inner item if there are no other references.
    /// Otherwise, return `None`.
    pub fn get_mut(&mut self) -> Option<&mut T> {
        Prc::get_mut(self.item.as_mut().unwrap())
    }

    /// Get mutable reference to the inner item without checking for other references.
    ///
    pub unsafe fn get_mut_unchecked(&mut self) -> &mut T {
        unsafe { Prc::get_mut_unchecked(self.item.as_mut().unwrap()) }
    }
}

/// An owned entry in the pool.
///
/// `OwnedEntry` holds a reference pointer to an item from the pool and a `Arc`
/// reference to the [`Pool`].
/// When the last `OwnedEntry` is dropped, the item is returned to the pool.
///
pub struct OwnedEntry<T: Default> {
    // When the last reference is dropped, the item is returned to the pool.
    // `item` is always `Some` before the last reference is dropped.
    pub(crate) item: Option<Prc<T>>,
    pub(crate) pool: Arc<Pool<T>>,
}

impl<T: Default> Clone for OwnedEntry<T> {
    /// Makes a clone of the `OwnedEntry` that points to the same allocation.
    fn clone(&self) -> Self {
        Self {
            item: self.item.clone(),
            pool: self.pool.clone(),
        }
    }
}

impl<T: Default> Drop for OwnedEntry<T> {
    fn drop(&mut self) {
        if self.item.as_ref().is_some_and(|i| i.dec_ref() == 1) {
            // This was the last reference, return to the pool.
            let item = self.item.take().unwrap();
            self.pool.recycle(item);
        }
    }
}

impl<T: Default> Deref for OwnedEntry<T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        self.item.as_ref().unwrap()
    }
}

impl<T: Default> OwnedEntry<T> {
    /// Get reference to the inner item.
    pub fn get(&self) -> &T {
        &self
    }

    /// Get mutable reference to the inner item if there are no other references.
    /// Otherwise, return `None`.
    pub fn get_mut(&mut self) -> Option<&mut T> {
        Prc::get_mut(self.item.as_mut().unwrap())
    }

    /// Get mutable reference to the inner item without checking for other references.
    ///
    pub unsafe fn get_mut_unchecked(&mut self) -> &mut T {
        unsafe { Prc::get_mut_unchecked(self.item.as_mut().unwrap()) }
    }
}

/// A thread-safe reference-counting pointer. `Prc` stands for 'Pooled
/// Reference Counted'. This is like `Arc`, but only used in the pool
/// implemented in this crate.
///
/// **Note**: `Drop` is not implemented for `Prc<T>`. The user should carefully
/// manage the memory of `Prc<T>`. The user should call `drop_slow` when the
/// last reference is dropped.
///
/// # Thread Safety
///
/// `Prc<T>` uses atomic operations for its reference counting. This means that
/// it is thread-safe.
///
/// # Cloning references
///
/// Creating a new reference from an existing reference-counted pointer is done using the
/// `Clone` trait implemented for [`Prc<T>`][Prc].
///
pub(crate) struct Prc<T: ?Sized> {
    ptr: NonNull<PrcInner<T>>,
}

unsafe impl<T: ?Sized + Send + Sync> Send for Prc<T> {}
unsafe impl<T: ?Sized + Send + Sync> Sync for Prc<T> {}

impl<T: ?Sized> Deref for Prc<T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        &self.inner().data
    }
}

impl<T: ?Sized> Clone for Prc<T> {
    fn clone(&self) -> Self {
        self.inc_ref();
        Self { ptr: self.ptr }
    }
}

impl<T> Prc<T> {
    /// Starting the pointer count as 0 which means it is in the pool without
    /// any clone instance.
    #[inline]
    pub(crate) fn new_zero(data: T) -> Self {
        let x: Box<_> = Box::new(PrcInner {
            count: AtomicUsize::new(0),
            data,
        });
        Self {
            ptr: Box::leak(x).into(),
        }
    }

    /// Create a new `Prc<T>` with the reference count starting at 1.
    #[inline]
    pub(crate) fn new(data: T) -> Self {
        let x: Box<_> = Box::new(PrcInner {
            count: AtomicUsize::new(1),
            data,
        });
        Self {
            ptr: Box::leak(x).into(),
        }
    }
}

impl<T: ?Sized> Prc<T> {
    /// Increase the reference count and return the previous count.
    #[inline]
    pub(crate) fn inc_ref(&self) -> usize {
        self.inner().count.fetch_add(1, Relaxed)
    }

    /// Decrease the reference count and return the previous count.
    #[inline]
    pub(crate) fn dec_ref(&self) -> usize {
        self.inner().count.fetch_sub(1, Release)
    }

    /// Drops the inner data.
    pub(crate) unsafe fn drop_slow(&self) {
        unsafe { ptr::drop_in_place(&mut (*self.ptr.as_ptr()).data) };
    }

    #[inline]
    pub unsafe fn get_mut_unchecked(this: &mut Self) -> &mut T {
        unsafe { &mut (*this.ptr.as_ptr()).data }
    }

    #[inline]
    pub fn get_mut(this: &mut Self) -> Option<&mut T> {
        // Only one reference exists or in the pool.
        if this.inner().count.load(Acquire) <= 1 {
            unsafe { Some(Prc::get_mut_unchecked(this)) }
        } else {
            None
        }
    }

    #[inline]
    fn inner(&self) -> &PrcInner<T> {
        unsafe { self.ptr.as_ref() }
    }
}

struct PrcInner<T: ?Sized> {
    count: AtomicUsize,
    data: T,
}

unsafe impl<T: ?Sized + Send + Sync> Send for PrcInner<T> {}
unsafe impl<T: ?Sized + Send + Sync> Sync for PrcInner<T> {}
