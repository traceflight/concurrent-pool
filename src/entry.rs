use std::fmt::{Debug, Display};
use std::hash::Hash;
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
#[derive(Debug)]
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

impl<'a, T: Default + PartialEq> PartialEq for Entry<'a, T> {
    fn eq(&self, other: &Self) -> bool {
        self.item.eq(&other.item)
    }
}

impl<'a, T: Default + Eq> Eq for Entry<'a, T> {}

impl<'a, T: Default + PartialOrd> PartialOrd for Entry<'a, T> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.item.partial_cmp(&other.item)
    }
}

impl<'a, T: Default + Ord> Ord for Entry<'a, T> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.item.cmp(&other.item)
    }
}

impl<'a, T: Default + Hash> Hash for Entry<'a, T> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.item.hash(state)
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

#[cfg(feature = "serde")]
impl<'a, T: Default + serde::Serialize> serde::Serialize for Entry<'a, T> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.get().serialize(serializer)
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

impl<T: Default + PartialEq> PartialEq for OwnedEntry<T> {
    fn eq(&self, other: &Self) -> bool {
        self.item.eq(&other.item)
    }
}

impl<T: Default + Eq> Eq for OwnedEntry<T> {}

impl<T: Default + PartialOrd> PartialOrd for OwnedEntry<T> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.item.partial_cmp(&other.item)
    }
}

impl<T: Default + Ord> Ord for OwnedEntry<T> {
    /// Comparison for two `OwnedEntry`
    ///
    /// # Example
    ///
    /// ```rust
    /// use concurrent_pool::Pool;
    /// use std::sync::Arc;
    ///
    /// let pool = Arc::new(Pool::<usize>::with_capacity(2));
    /// let item1 = pool.pull_owned_with(|i| *i = 1).unwrap();
    /// let item2 = pool.pull_owned_with(|i| *i = 2).unwrap();
    /// assert!(item1 < item2);
    /// ```
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.item.cmp(&other.item)
    }
}

impl<T: Default + Hash> Hash for OwnedEntry<T> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.item.hash(state)
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

#[cfg(feature = "serde")]
impl<T: Default + serde::Serialize> serde::Serialize for OwnedEntry<T> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.get().serialize(serializer)
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

impl<T: ?Sized + PartialEq> PartialEq for Prc<T> {
    fn eq(&self, other: &Self) -> bool {
        self.inner().data.eq(other)
    }
}
impl<T: ?Sized + Eq> Eq for Prc<T> {}

impl<T: ?Sized + PartialOrd> PartialOrd for Prc<T> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.inner().data.partial_cmp(other)
    }
}

impl<T: ?Sized + Ord> Ord for Prc<T> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.inner().data.cmp(other)
    }
}

impl<T: ?Sized + Hash> Hash for Prc<T> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.inner().data.hash(state)
    }
}

impl<T: ?Sized + Debug> Debug for Prc<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.inner().data.fmt(f)
    }
}

impl<T: ?Sized + Display> Display for Prc<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.inner().data.fmt(f)
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
        unsafe {
            drop(Box::from_raw(self.ptr.as_ptr()));
        }
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
