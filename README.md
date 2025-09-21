# concurrent-pool

[![Crates.io](https://img.shields.io/crates/v/concurrent-pool.svg)](https://crates.io/crates/concurrent-pool)
[![rustdoc](https://img.shields.io/badge/Doc-concurrent_pool-green.svg)](https://docs.rs/concurrent-pool/)

A concurrent object pool.

## Features

- Configurable capacity and preallocation.
- Thread-safe: Multiple threads can pull and recycle items concurrently.
- Automatic reclamation of unused item when the continuous occurrence
of `fast-pull` reaches a certain threshold if `auto_reclaim` is enabled.

**fast-pull**: After pulling data from the memory pool, available allocated 
entities in the memory pool are exceed a certain threshold. We call this pull 
is a `fast-pull`.

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
use concurrent_pool::Pool;
use std::sync::{Arc, mpsc};

let pool: Arc<Pool<u32>> = Arc::new(Pool::with_capacity(10));

let (tx, rx) = mpsc::channel();
let clone_pool = pool.clone();
let tx1 = tx.clone();
let sender1 = std::thread::spawn(move || {
    let item = clone_pool.pull_owned_with(|x| *x = 1).unwrap();
    tx1.send((1, item)).unwrap();
});

let clone_pool = pool.clone();
let sender2 = std::thread::spawn(move || {
    let item = clone_pool.pull_owned_with(|x| *x = 2).unwrap();
    tx.send((2, item)).unwrap();
});

let receiver = std::thread::spawn(move || {
    for _ in 0..2 {
        let (id, item) = rx.recv().unwrap();
        if id == 1 {
            assert_eq!(*item, 1);
        } else {
            assert_eq!(*item, 2);
        }
    }
});

sender1.join().unwrap();
sender2.join().unwrap();
receiver.join().unwrap();
```

## License

This project is licensed under either of

 * Apache License, Version 2.0, ([LICENSE-APACHE](LICENSE-APACHE) or
   http://www.apache.org/licenses/LICENSE-2.0)
 * MIT license ([LICENSE-MIT](LICENSE-MIT) or
   http://opensource.org/licenses/MIT)

at your option.