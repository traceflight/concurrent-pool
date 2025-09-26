# Concurrent Pool

[![Crates.io](https://img.shields.io/crates/v/concurrent-pool.svg)](https://crates.io/crates/concurrent-pool)
[![rustdoc](https://img.shields.io/badge/Doc-concurrent_pool-green.svg)](https://docs.rs/concurrent-pool/)

A concurrent object pool based on [Crossbeam Queue](https://crates.io/crates/crossbeam-queue)

## Features

- Configurable capacity and preallocation.
- Thread-safe: Multiple threads can pull and recycle items concurrently.
- Automatic return of dropped items to the pool for reuse.
- Automatic reclamation of unused item when the continuous occurrence
of `surplus-pull` reaches a certain threshold if `auto_reclaim` is enabled.

**surplus-pull**: After pulling data from the memory pool, available allocated 
entities in the memory pool are exceed a certain threshold. We call this pull 
is a `surplus-pull`.

## Examples

### Local memory pool

```rust
use concurrent_pool::Pool;

let pool: Pool<u32> = Pool::with_capacity(10);
assert_eq!(pool.available(), 10);
let item = pool.pull().unwrap();
assert_eq!(*item, 0);
assert_eq!(pool.available(), 9);
let item_clone = item.clone();
drop(item);
assert_eq!(pool.available(), 9);
drop(item_clone);
assert_eq!(pool.available(), 10);
```

### Multiple threads shared memory pool

```rust
use concurrent_pool::{Pool, Builder};
use std::sync::{Arc, mpsc};

let mut builder = Builder::new();
let pool: Arc<Pool<String>> = Arc::new(builder.capacity(10).clear_func(String::clear).build());


let (tx, rx) = mpsc::channel();
let clone_pool = pool.clone();
let tx1 = tx.clone();
let sender1 = std::thread::spawn(move || {
    let item = clone_pool.pull_owned_with(|x| x.push_str("1")).unwrap();
    tx1.send((1, item)).unwrap();
});

let clone_pool = pool.clone();
let sender2 = std::thread::spawn(move || {
    let item = clone_pool.pull_owned_with(|x| x.push_str("2")).unwrap();
    tx.send((2, item)).unwrap();
});

let receiver = std::thread::spawn(move || {
    for _ in 0..2 {
        let (id, item) = rx.recv().unwrap();
        if id == 1 {
            assert_eq!(*item, "1");
        } else {
            assert_eq!(*item, "2");
        }
    }
});

sender1.join().unwrap();
sender2.join().unwrap();
receiver.join().unwrap();
```
## Performance

These [benchmarks](./benches/bench.rs) are based on [`sharded-slab`](https://crates.io/crates/sharded-slab) repository.

The first shows the results of a benchmark where an increasing number of
items are inserted and then removed into a slab concurrently by five
threads. It compares the performance of the `concurrent pool` implementation
with a `sharded-slab` and a `RwLock<slab::Slab>`:

![multi-threaded](./benches/images/insert_remove_multi_threaded.svg)

The second graph shows the results of a benchmark where an increasing
number of items are inserted and then removed by a _single_ thread. It
compares the performance of the `concurrent pool` with a `sharded-slab`,
an `RwLock<slab::Slab>` and a `mut slab::Slab`.

![single-threaded](./benches/images/insert_remove_single_threaded.svg)

The benchmarks show that concurrent pool performs worse than slab in single-threaded scenarios, but significantly outperforms sharded slab. In multi-threaded scenarios, it is clearly better than slab, and only slightly worse than sharded slab.

## License

This project is licensed under either of

 * Apache License, Version 2.0, ([LICENSE-APACHE](LICENSE-APACHE) or
   http://www.apache.org/licenses/LICENSE-2.0)
 * MIT license ([LICENSE-MIT](LICENSE-MIT) or
   http://opensource.org/licenses/MIT)

at your option.