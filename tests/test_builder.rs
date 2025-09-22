use concurrent_pool::Builder;

#[test]
fn build_pool() {
    let mut builder = Builder::<usize>::new();
    let pool = builder.capacity(10).prealloc(5).build();
    assert_eq!(pool.capacity(), 10);
}

#[test]
fn build_with_clear_func() {
    let mut builder: Builder<String> = Builder::new();
    fn clear_func(item: &mut String) {
        item.clear();
    }
    builder.clear_func(clear_func);
    builder.capacity(2);
    let pool = builder.build();
    let item1 = pool.pull_with(|i| i.push_str("hello")).unwrap();
    assert_eq!(item1.as_str(), "hello");
    let item2 = pool.pull_with(|i| i.push_str("world")).unwrap();
    assert_eq!(item2.as_str(), "world");

    assert_eq!(pool.available(), 0);
    drop(item1);
    assert_eq!(pool.available(), 1);
    let item3 = pool.pull().unwrap();
    assert_eq!(item3.as_str(), "");
}

#[test]
fn build_with_auto_reclaim() {
    let mut builder = Builder::<usize>::new();
    let pool = builder
        .capacity(5)
        .prealloc(2)
        .enable_auto_reclaim()
        .surpluspull_threshold_for_reclaim(3)
        .idle_threshold_for_surpluspull(2)
        .build();
    assert_eq!(pool.capacity(), 5);
    assert_eq!(pool.allocated(), 2);
    let item1 = pool.pull().unwrap();
    let item2 = pool.pull().unwrap();
    let item3 = pool.pull().unwrap();
    let item4 = pool.pull().unwrap();
    let item5 = pool.pull().unwrap();
    assert_eq!(pool.available(), 0);
    drop(item1);
    drop(item2);
    drop(item3);
    drop(item4);
    drop(item5);
    assert_eq!(pool.allocated(), 5);

    // first surplus-pull
    let _item1 = pool.pull().unwrap();
    // second surplus-pull
    let _item2 = pool.pull().unwrap();
    assert_eq!(pool.allocated(), 5);
    // third surplus-pull, triger reclaim
    let _item3 = pool.pull().unwrap();
    assert_eq!(pool.allocated(), 4);
}
