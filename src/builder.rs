use crate::{Config, Pool};

/// A builder for creating a [`Pool`] with custom configuration.
///
/// # Example
///
/// ```rust
/// use concurrent_pool::Builder;
///
/// let mut builder = Builder::<usize>::new();
/// let pool = builder.capacity(10).prealloc(5).build();
/// assert_eq!(pool.capacity(), 10);
/// ```
pub struct Builder<T: Default> {
    /// Configuration of the pool.
    config: Config<T>,
}

impl<T: Default> Builder<T> {
    /// Create a new builder with default configuration.
    pub fn new() -> Self {
        Self {
            config: Config::default(),
        }
    }

    /// Set the number of preallocated items in the pool.
    pub fn prealloc(&mut self, prealloc: usize) -> &mut Self {
        self.config.prealloc = prealloc;
        self
    }

    /// Set the maximum capacity of the pool.
    pub fn capacity(&mut self, capacity: usize) -> &mut Self {
        self.config.capacity = capacity;
        self
    }

    /// Set the function to clear an item before it is returned to the pool.
    pub fn clear_func(&mut self, func: fn(&mut T)) -> &mut Self {
        self.config.clear_func = Some(func);
        self
    }

    /// Enable or disable auto reclaiming allocated items and free them to reduce memory usage.
    pub fn auto_reclaim(&mut self, enable: bool) -> &mut Self {
        self.config.auto_reclaim = enable;
        self
    }

    /// Enable auto reclaiming allocated items and free them to reduce memory usage.
    pub fn enable_auto_reclaim(&mut self) -> &mut Self {
        self.auto_reclaim(true)
    }

    /// Set the threshold of `fast-pull` continuous occurrence to trigger reclamation
    /// when `auto_reclaim` is enabled.
    pub fn fastpull_threshold_for_reclaim(&mut self, threshold: usize) -> &mut Self {
        self.config.fastpull_threshold_for_reclaim = threshold;
        self
    }

    /// Set the threshold for idle items to judge as a `fast-pull` when `auto_reclaim` is enabled.
    pub fn idle_threshold_for_fastpull(&mut self, threshold: usize) -> &mut Self {
        self.config.idle_threshold_for_fastpull = threshold;
        self
    }

    /// Build the pool with the current configuration.
    pub fn build(&mut self) -> Pool<T> {
        let config = std::mem::take(&mut self.config);
        Pool::with_config(config)
    }
}
