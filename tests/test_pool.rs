use std::sync::{Arc, mpsc};

use concurrent_pool::Pool;

#[derive(Debug)]
struct BigStruct {
    _slice: [u8; 2048],
    _heap: Vec<u8>,
    str: String,
}

impl Default for BigStruct {
    fn default() -> Self {
        Self {
            _slice: [0; 2048],
            _heap: vec![0; 2048],
            str: "Hello".to_string(),
        }
    }
}

#[test]
fn pool_with_big_struct() {
    let pool = Pool::<BigStruct>::with_capacity(10);
    assert_eq!(pool.capacity(), 10);
}

#[test]
fn pool_with_many_big_struct() {
    let pool = Pool::<BigStruct>::with_capacity(100000);
    assert_eq!(pool.capacity(), 100000);
}

#[test]
fn pool_with_zero_capacity() {
    let _pool = Pool::<BigStruct>::with_capacity(0);
}

#[test]
fn single_thread_pull_recycle() {
    let pool = Pool::<BigStruct>::with_capacity(2);
    let mut item1 = pool.pull().unwrap();
    let item1_mut = item1.get_mut().unwrap();
    item1_mut.str.push_str(" World");
    assert_eq!(item1_mut.str.as_str(), "Hello World");

    let item2 = pool.pull().unwrap();
    assert_eq!(item2.str.as_str(), "Hello");

    drop(item1);
    // recycled item1
    let item3 = pool.pull().unwrap();
    assert_eq!(item3.str.as_str(), "Hello World");
}

#[test]
fn one_send_thread_one_recv_thread() {
    let (tx, rx) = mpsc::channel();
    let sender_handle = std::thread::spawn(move || {
        let pool = Arc::new(Pool::<BigStruct>::with_capacity(10000));
        let mut counter = 0;
        loop {
            if counter == 10000 {
                break;
            }
            let item = pool
                .pull_owned_with(|item| item.str = counter.to_string())
                .unwrap();
            counter += 1;
            tx.send(item).unwrap();
        }
    });

    let receiver_handle = std::thread::spawn(move || {
        let mut counter = 0;
        while let Ok(item) = rx.recv() {
            assert_eq!(item.str.as_str(), counter.to_string());
            counter += 1;
        }
    });
    sender_handle.join().unwrap();
    receiver_handle.join().unwrap();
}

#[test]
fn two_send_thread_one_recv_thread() {
    let (tx, rx) = mpsc::channel();
    let tx1 = tx.clone();
    let pool = Arc::new(Pool::<BigStruct>::with_capacity(10000));
    let pool_clone = pool.clone();
    let sender1_handle = std::thread::spawn(move || {
        let mut counter = 0;
        loop {
            if counter == 5000 {
                break;
            }
            let item = pool_clone
                .pull_owned_with(|item| item.str = counter.to_string())
                .unwrap();
            counter += 1;
            tx1.send((1, item)).unwrap();
        }
    });
    let sender2_handle = std::thread::spawn(move || {
        let mut counter = 0;
        loop {
            if counter == 5000 {
                break;
            }
            let item = pool
                .pull_owned_with(|item| item.str = counter.to_string())
                .unwrap();
            counter += 1;
            tx.send((2, item)).unwrap();
        }
    });

    let receiver_handle = std::thread::spawn(move || {
        let mut counter1 = 0;
        let mut counter2 = 0;
        while let Ok((thread_id, item)) = rx.recv() {
            if thread_id == 1 {
                assert_eq!(item.str.as_str(), counter1.to_string());
                counter1 += 1;
            } else {
                assert_eq!(item.str.as_str(), counter2.to_string());
                counter2 += 1;
            }
        }
    });
    sender1_handle.join().unwrap();
    sender2_handle.join().unwrap();
    receiver_handle.join().unwrap();
}

#[test]
fn two_send_thread_two_recv_thread() {
    let (tx1, rx1) = mpsc::channel();
    let (tx2, rx2) = mpsc::channel();
    let pool = Arc::new(Pool::<BigStruct>::with_capacity(10000));
    let pool_clone = pool.clone();
    let sender1_handle = std::thread::spawn(move || {
        let mut counter = 0;
        loop {
            if counter == 5000 {
                break;
            }
            let item = pool
                .pull_owned_with(|item| item.str = counter.to_string())
                .unwrap();
            counter += 1;
            tx1.send(item).unwrap();
        }
    });
    let sender2_handle = std::thread::spawn(move || {
        let mut counter = 0;
        loop {
            if counter == 5000 {
                break;
            }
            let item = pool_clone
                .pull_owned_with(|item| item.str = counter.to_string())
                .unwrap();
            counter += 1;
            tx2.send(item).unwrap();
        }
    });

    let receiver1_handle = std::thread::spawn(move || {
        let mut counter = 0;
        while let Ok(item) = rx1.recv() {
            assert_eq!(item.str.as_str(), counter.to_string());
            counter += 1;
        }
    });
    let receiver2_handle = std::thread::spawn(move || {
        let mut counter = 0;
        while let Ok(item) = rx2.recv() {
            assert_eq!(item.str.as_str(), counter.to_string());
            counter += 1;
        }
    });
    sender1_handle.join().unwrap();
    sender2_handle.join().unwrap();
    receiver1_handle.join().unwrap();
    receiver2_handle.join().unwrap();
}
