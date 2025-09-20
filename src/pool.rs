use std::cmp::max;
use std::sync::Arc;
use std::sync::atomic::Ordering::*;
use std::sync::atomic::{AtomicBool, AtomicUsize};

use crossbeam_queue::ArrayQueue;

use crate::entry::Prc;
use crate::{Entry, OwnedEntry};

/// A concurrent object pool.
///
pub struct Pool<T: Default> {
    /// Configuration of the pool.
    config: Config<T>,
    /// Inner queue holding the pooled items.
    queue: ArrayQueue<Prc<T>>,
    /// Number of items currently allocated.
    allocated: AtomicUsize,
    /// Number of currently continues `fast-pull` times
    fastpulls: AtomicUsize,
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
    pub fn new(prealloc: usize, capacity: usize) -> Self {
        Self::with_config(Config {
            capacity,
            prealloc,
            ..Default::default()
        })
    }

    /// Create a new pool with the given capacity.
    pub fn with_capacity(capacity: usize) -> Self {
        Self::new(capacity, capacity)
    }

    /// Create a new pool with half of the capacity preallocated.
    pub fn with_capacity_half_prealloc(capacity: usize) -> Self {
        Self::new(capacity / 2, capacity)
    }

    /// Create a new pool with the given configuration.
    pub fn with_config(mut config: Config<T>) -> Self {
        config.post_process();
        let prealloc = config.prealloc;
        assert!(
            prealloc <= config.capacity,
            "prealloc must be less than or equal to capacity"
        );

        let pool = Self {
            queue: ArrayQueue::new(config.capacity),
            allocated: AtomicUsize::new(prealloc),
            fastpulls: AtomicUsize::new(0),
            additional_allocated: AtomicBool::new(false),
            config,
        };
        let mut items = Vec::with_capacity(prealloc);
        for _ in 0..prealloc {
            items.push(T::default());
        }
        while let Some(item) = items.pop() {
            let _ = pool.queue.push(Prc::new(item));
        }
        pool
    }

    /// Get in used items count.
    pub fn in_use(&self) -> usize {
        self.allocated.load(Relaxed) - self.queue.len()
    }

    /// Get available items count.
    pub fn available(&self) -> usize {
        self.config.capacity - self.in_use()
    }

    /// Get available items count without allocation.
    pub fn available_noalloc(&self) -> usize {
        self.queue.len()
    }

    /// Check if the pool is empty.
    pub fn is_empty(&self) -> bool {
        self.available() == 0
    }

    pub fn pull(&self) -> Option<Entry<'_, T>> {
        self.pull_inner().map(|item| Entry {
            item: Some(item),
            pool: self,
        })
    }

    pub fn pull_with<F>(&self, func: F) -> Option<Entry<'_, T>>
    where
        F: FnOnce(&mut T),
    {
        self.pull().map(|mut entry| {
            func(unsafe { entry.get_mut_unchecked() });
            entry
        })
    }

    pub fn pull_owned(self: &Arc<Self>) -> Option<OwnedEntry<T>> {
        self.pull_inner().map(|item| crate::OwnedEntry {
            item: Some(item),
            pool: self.clone(),
        })
    }

    pub fn pull_owned_with<F>(self: &Arc<Self>, func: F) -> Option<OwnedEntry<T>>
    where
        F: FnOnce(&mut T),
    {
        self.pull_owned().map(|mut entry| {
            func(unsafe { entry.get_mut_unchecked() });
            entry
        })
    }

    fn pull_inner(&self) -> Option<Prc<T>> {
        match self.queue.pop() {
            None => {
                if !self.additional_allocated.load(Relaxed) {
                    self.additional_allocated.store(true, Relaxed);
                }
                if self.config.need_process_reclamation {
                    self.fastpulls.store(0, SeqCst);
                }
                if self.allocated.fetch_add(1, Relaxed) + 1 <= self.config.capacity {
                    let item = Prc::new(T::default());
                    item.inc_ref();
                    Some(item)
                } else {
                    None
                }
            }
            Some(item) => {
                if self.config.need_process_reclamation {
                    let left = self.queue.len();
                    if left >= self.config.idle_fastpull_threshold {
                        let fastpulls = self.fastpulls.fetch_add(1, Relaxed) + 1;
                        if fastpulls >= self.config.fastpull_reclaim_threshold
                            && self.additional_allocated.load(Relaxed)
                        {
                            self.reclaim();
                        }
                    } else {
                        self.fastpulls.store(0, SeqCst);
                    }
                }
                item.inc_ref();
                Some(item)
            }
        }
    }

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
        let _ = self.queue.push(item);
    }
}

/// Configuration for the pool.
pub struct Config<T: Default> {
    /// Maximum capacity of the pool.
    pub capacity: usize,
    /// Number of items to preallocate.
    pub prealloc: usize,
    /// Whether to automatically reclaim allocated items and free them to reduce memory usage.
    pub auto_reclaim: bool,
    /// Threshold for fast-pull items to trigger reclamation when `auto_reclaim` is enabled.
    pub fastpull_reclaim_threshold: usize,
    /// Threshold for idle items to judge as a fastpull when `auto_reclaim` is enabled.
    pub idle_fastpull_threshold: usize,
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
            fastpull_reclaim_threshold: 0,
            idle_fastpull_threshold: 0,
            need_process_reclamation: false,
        }
    }
}

impl<T: Default> Config<T> {
    pub(crate) fn post_process(&mut self) {
        if self.idle_fastpull_threshold == 0 {
            self.idle_fastpull_threshold = max(1, self.capacity / 20);
        }

        if self.auto_reclaim && self.prealloc != self.capacity {
            self.need_process_reclamation = true;
        } else {
            self.need_process_reclamation = false;
        }
    }
}
