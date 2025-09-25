use std::cmp::max;
use std::sync::Arc;
use std::sync::atomic::Ordering::*;
use std::sync::atomic::{AtomicBool, AtomicUsize};

use crossbeam_queue::ArrayQueue;

use crate::entry::Prc;
use crate::{Entry, OwnedEntry};

/// A concurrent object pool.
///
/// # Examples
///
/// ```rust
/// use concurrent_pool::{Pool, Builder};
/// use std::sync::{Arc, mpsc};
///
/// let mut builder = Builder::new();
///
/// let pool: Arc<Pool<String>> = Arc::new(builder.capacity(10).clear_func(String::clear).build());
///
///
/// let (tx, rx) = mpsc::channel();
/// let clone_pool = pool.clone();
/// let tx1 = tx.clone();
/// let sender1 = std::thread::spawn(move || {
///     let item = clone_pool.pull_owned_with(|x| x.push_str("1")).unwrap();
///     tx1.send((1, item)).unwrap();
/// });
///
/// let clone_pool = pool.clone();
/// let sender2 = std::thread::spawn(move || {
///     let item = clone_pool.pull_owned_with(|x| x.push_str("2")).unwrap();
///     tx.send((2, item)).unwrap();
/// });
///
/// let receiver = std::thread::spawn(move || {
///     for _ in 0..2 {
///         let (id, item) = rx.recv().unwrap();
///         if id == 1 {
///             assert_eq!(*item, "1");
///         } else {
///             assert_eq!(*item, "2");
///         }
///     }
/// });
///
/// sender1.join().unwrap();
/// sender2.join().unwrap();
/// receiver.join().unwrap();
/// ```
#[derive(Debug)]
pub struct Pool<T: Default> {
    /// Configuration of the pool.
    config: Config<T>,
    /// Inner queue holding the pooled items.
    queue: ArrayQueue<Prc<T>>,
    /// Number of items currently allocated.
    allocated: AtomicUsize,
    /// Number of currently continues `surplus-pull` times
    surpluspulls: AtomicUsize,
    /// Whether an additional item has been allocated beyond the preallocated items.
    additional_allocated: AtomicBool,
}

impl<T: Default> Drop for Pool<T> {
    fn drop(&mut self) {
        while let Some(item) = self.queue.pop() {
            unsafe { item.drop_slow() };
        }
    }
}

impl<T: Default> Pool<T> {
    /// Create a new pool with the given preallocation and capacity.
    ///
    /// # Example
    ///
    /// ```rust
    /// use concurrent_pool::Pool;
    ///
    /// let pool: Pool<u32> = Pool::new(2, 5);
    /// assert_eq!(pool.available(), 5);
    /// assert_eq!(pool.available_noalloc(), 2);
    /// let item = pool.pull().unwrap();
    /// assert_eq!(pool.available_noalloc(), 1);
    /// ```
    pub fn new(prealloc: usize, capacity: usize) -> Self {
        Self::with_config(Config {
            capacity,
            prealloc,
            ..Default::default()
        })
    }

    /// Create a new pool with the given capacity.
    ///
    /// # Example
    ///
    /// ```rust
    /// use concurrent_pool::Pool;
    ///
    /// let pool: Pool<u32> = Pool::with_capacity(10);
    /// assert_eq!(pool.available(), 10);
    /// assert_eq!(pool.available_noalloc(), 10);
    /// let item = pool.pull().unwrap();
    /// assert_eq!(pool.available(), 9);
    /// ```
    pub fn with_capacity(capacity: usize) -> Self {
        Self::new(capacity, capacity)
    }

    /// Create a new pool with half of the capacity preallocated.
    ///
    /// # Example
    ///
    /// ```rust
    /// use concurrent_pool::Pool;
    ///
    /// let pool: Pool<u32> = Pool::with_capacity_half_prealloc(10);
    /// assert_eq!(pool.available(), 10);
    /// assert_eq!(pool.available_noalloc(), 5);
    /// let item = pool.pull().unwrap();
    /// assert_eq!(pool.available_noalloc(), 4);
    /// assert_eq!(pool.in_use(), 1);
    /// ```
    pub fn with_capacity_half_prealloc(capacity: usize) -> Self {
        Self::new(capacity / 2, capacity)
    }

    /// Create a new pool with the given configuration.
    ///
    /// # Example
    ///
    /// ```rust
    /// use concurrent_pool::{Pool, Config};
    ///
    /// fn clear_func(x: &mut String) {
    ///     x.clear();
    /// }
    ///
    /// let mut config = Config::default();
    /// config.capacity = 1;
    /// config.clear_func = Some(clear_func);
    /// let pool: Pool<String> = Pool::with_config(config);
    /// let item = pool.pull_with(|s| s.push_str("Hello, World!")).unwrap();
    /// assert_eq!(&*item, "Hello, World!");
    /// drop(item);
    /// let item2 = pool.pull().unwrap();
    /// assert_eq!(&*item2, "");
    /// ```
    pub fn with_config(mut config: Config<T>) -> Self {
        config.post_process();
        let prealloc = config.prealloc;
        assert!(
            prealloc <= config.capacity,
            "prealloc must be less than or equal to capacity"
        );

        let queue_len = max(1, config.capacity);
        let pool = Self {
            queue: ArrayQueue::new(queue_len),
            allocated: AtomicUsize::new(prealloc),
            surpluspulls: AtomicUsize::new(0),
            additional_allocated: AtomicBool::new(false),
            config,
        };
        let mut items = Vec::with_capacity(prealloc);
        for _ in 0..prealloc {
            items.push(T::default());
        }
        while let Some(item) = items.pop() {
            let _ = pool.queue.push(Prc::new_zero(item));
        }
        pool
    }

    /// Get in used items count.
    ///
    /// # Example
    ///
    /// ```rust
    /// use concurrent_pool::Pool;
    ///
    /// let pool: Pool<u32> = Pool::with_capacity(10);
    /// assert_eq!(pool.in_use(), 0);
    /// let item = pool.pull().unwrap();
    /// assert_eq!(pool.in_use(), 1);
    /// let item2 = pool.pull().unwrap();
    /// assert_eq!(pool.in_use(), 2);
    /// ```
    pub fn in_use(&self) -> usize {
        self.allocated.load(Relaxed) - self.queue.len()
    }

    /// Get allocated items count.
    ///
    /// # Example
    ///
    /// ```rust
    /// use concurrent_pool::Pool;
    ///
    /// let pool = Pool::<usize>::new(2, 5);
    /// assert_eq!(pool.allocated(), 2);
    /// ```
    pub fn allocated(&self) -> usize {
        self.allocated.load(Acquire)
    }

    /// Get available items count.
    ///
    /// # Example
    ///
    /// ```rust
    /// use concurrent_pool::Pool;
    ///
    /// let pool: Pool<u32> = Pool::with_capacity(10);
    /// assert_eq!(pool.available(), 10);
    /// let item = pool.pull().unwrap();
    /// assert_eq!(pool.available(), 9);
    /// ```
    pub fn available(&self) -> usize {
        self.config.capacity - self.in_use()
    }

    /// Get available items count without allocation.
    ///
    /// # Example
    ///
    /// ```rust
    /// use concurrent_pool::Pool;
    ///
    /// let pool: Pool<u32> = Pool::new(2, 5);
    /// assert_eq!(pool.available_noalloc(), 2);
    /// let item = pool.pull().unwrap();
    /// assert_eq!(pool.available_noalloc(), 1);
    /// let item2 = pool.pull().unwrap();
    /// assert_eq!(pool.available_noalloc(), 0);
    /// let item3 = pool.pull().unwrap();
    /// assert_eq!(pool.available_noalloc(), 0);
    /// drop(item);
    /// assert_eq!(pool.available_noalloc(), 1);
    /// ```
    pub fn available_noalloc(&self) -> usize {
        self.queue.len()
    }

    /// Check if the pool is empty.
    ///
    /// # Example
    ///
    /// ```rust
    /// use concurrent_pool::Pool;
    ///
    /// let pool: Pool<u32> = Pool::with_capacity(2);
    /// assert!(!pool.is_empty());
    /// let item1 = pool.pull().unwrap();
    /// assert!(!pool.is_empty());
    /// let item2 = pool.pull().unwrap();
    /// assert!(pool.is_empty());
    /// drop(item1);
    /// assert!(!pool.is_empty());
    /// ```
    pub fn is_empty(&self) -> bool {
        self.available() == 0
    }

    /// Get the capacity of the pool.
    ///
    /// # Example
    ///
    /// ```rust
    /// use concurrent_pool::Pool;
    ///
    /// let pool: Pool<u32> = Pool::with_capacity(10);
    /// assert_eq!(pool.capacity(), 10);
    /// ```
    pub fn capacity(&self) -> usize {
        self.config.capacity
    }

    /// Pull an item from the pool. Return `None` if the pool is empty.
    ///
    /// # Example
    ///
    /// ```rust
    /// use concurrent_pool::Pool;
    ///
    /// let pool: Pool<u32> = Pool::with_capacity(2);
    /// let item1 = pool.pull().unwrap();
    /// assert_eq!(*item1, 0);
    /// ```
    pub fn pull(&self) -> Option<Entry<'_, T>> {
        self.pull_inner().map(|item| Entry {
            item: Some(item),
            pool: self,
        })
    }

    /// Pull an item from the pool and apply a function to it. Return `None` if the pool is empty.
    ///
    /// # Example
    ///
    /// ```rust
    /// use concurrent_pool::Pool;
    ///
    /// let pool: Pool<u32> = Pool::with_capacity(2);
    /// let item1 = pool.pull_with(|x| *x = 42).unwrap();
    /// assert_eq!(*item1, 42);
    /// ```
    pub fn pull_with<F>(&self, func: F) -> Option<Entry<'_, T>>
    where
        F: FnOnce(&mut T),
    {
        self.pull().map(|mut entry| {
            func(unsafe { entry.get_mut_unchecked() });
            entry
        })
    }

    /// Pull an owned item from the pool. Return `None` if the pool is empty.
    ///
    /// # Example
    ///
    /// ```rust
    /// use concurrent_pool::Pool;
    /// use std::sync::Arc;
    ///
    /// let pool: Arc<Pool<u32>> = Arc::new(Pool::with_capacity(2));
    /// let item1 = pool.pull_owned().unwrap();
    /// assert_eq!(*item1, 0);
    /// ```
    pub fn pull_owned(self: &Arc<Self>) -> Option<OwnedEntry<T>> {
        self.pull_inner().map(|item| crate::OwnedEntry {
            item: Some(item),
            pool: self.clone(),
        })
    }

    /// Pull an owned item from the pool and apply a function to it. Return `None` if the pool is empty.
    ///
    /// # Example
    ///
    /// ```rust
    /// use concurrent_pool::Pool;
    /// use std::sync::Arc;
    ///
    /// let pool: Arc<Pool<u32>> = Arc::new(Pool::with_capacity(2));
    /// let item1 = pool.pull_owned_with(|x| *x = 42).unwrap();
    /// assert_eq!(*item1, 42);
    /// ```
    pub fn pull_owned_with<F>(self: &Arc<Self>, func: F) -> Option<OwnedEntry<T>>
    where
        F: FnOnce(&mut T),
    {
        self.pull_owned().map(|mut entry| {
            func(unsafe { entry.get_mut_unchecked() });
            entry
        })
    }

    /// Internal method to pull an item from the pool.
    fn pull_inner(&self) -> Option<Prc<T>> {
        match self.queue.pop() {
            None => {
                if !self.additional_allocated.load(Relaxed) {
                    self.additional_allocated.store(true, Relaxed);
                }
                if self.config.need_process_reclamation {
                    self.surpluspulls.store(0, SeqCst);
                }
                if self.allocated.load(Acquire) < self.config.capacity {
                    self.allocated.fetch_add(1, Relaxed);
                    Some(Prc::new(T::default()))
                } else {
                    None
                }
            }
            Some(item) => {
                if self.config.need_process_reclamation {
                    let left = self.queue.len();
                    if left >= self.config.idle_threshold_for_surpluspull {
                        let surpluspulls = self.surpluspulls.fetch_add(1, Relaxed) + 1;
                        if surpluspulls >= self.config.surpluspull_threshold_for_reclaim
                            && self.additional_allocated.load(Relaxed)
                        {
                            self.reclaim();
                        }
                    } else {
                        self.surpluspulls.store(0, Relaxed);
                    }
                }
                item.inc_ref();
                Some(item)
            }
        }
    }

    /// Reclaim an item from the pool to reduce memory usage.
    fn reclaim(&self) {
        if let Some(item) = self.queue.pop() {
            unsafe { item.drop_slow() };
            let current = self.allocated.fetch_sub(1, Release) - 1;
            if self.config.need_process_reclamation && current <= self.config.prealloc {
                if self.additional_allocated.load(Relaxed) {
                    self.additional_allocated.store(false, Relaxed);
                }
            }
        }
    }

    /// Recycle an item back into the pool.
    pub(crate) fn recycle(&self, mut item: Prc<T>) {
        if let Some(func) = &self.config.clear_func {
            func(unsafe { Prc::get_mut_unchecked(&mut item) })
        }
        if self.queue.push(item).is_err() {
            panic!("It is imposible that the pool is full when recycling an item");
        }
    }
}

/// Configuration for the pool.
#[derive(Debug)]
pub struct Config<T: Default> {
    /// Maximum capacity of the pool.
    pub capacity: usize,
    /// Number of items to preallocate.
    pub prealloc: usize,
    /// Whether to automatically reclaim allocated items and free them to reduce memory usage.
    pub auto_reclaim: bool,
    /// Threshold of `surplus-pull` continuous occurrence to trigger reclamation
    /// when `auto_reclaim` is enabled.
    pub surpluspull_threshold_for_reclaim: usize,
    /// Threshold for idle items to judge as a surplus-pull when `auto_reclaim` is enabled.
    pub idle_threshold_for_surpluspull: usize,
    /// Optional function to clear or reset an item before it is reused.
    pub clear_func: Option<fn(&mut T)>,
    /// Internal flag to indicate if the pool needs to process reclamation.
    need_process_reclamation: bool,
}

impl<T: Default> Default for Config<T> {
    fn default() -> Self {
        Self {
            capacity: 1024,
            prealloc: 0,
            auto_reclaim: false,
            clear_func: None,
            surpluspull_threshold_for_reclaim: 0,
            idle_threshold_for_surpluspull: 0,
            need_process_reclamation: false,
        }
    }
}

impl<T: Default> Config<T> {
    pub(crate) fn post_process(&mut self) {
        if self.idle_threshold_for_surpluspull == 0 {
            self.idle_threshold_for_surpluspull = max(1, self.capacity / 20);
        }

        if self.surpluspull_threshold_for_reclaim == 0 {
            self.surpluspull_threshold_for_reclaim = max(2, self.capacity / 100);
        }

        if self.auto_reclaim && self.prealloc != self.capacity {
            self.need_process_reclamation = true;
        } else {
            self.need_process_reclamation = false;
        }
    }
}
