`syncthreads` is a library providing synchronous and asynchronous barrier types.
These can be constructed using `BarrierInit` and `AsyncBarrierInit`.

# Example
Sequential code:
```rust
let n = 1000;
let x = &mut *vec![1.0; n];
for i in 0..n / 2 {
    let head = x[i];
    for x in &mut x[i + 1..] {
        *x += head;
    }
}
for i in 0..n / 2 {
    assert_eq!(x[i], 2.0f64.powi(i as _));
}
```

Multithreaded code using `Barrier`.
```rust
use syncthreads::{iter, sync_blocking, BarrierInit};
let n = 1000;
let x = &mut *vec![1.0; n];
let nthreads = 3;
let init = BarrierInit::new(&mut *x, nthreads, Default::default(), Default::default());
std::thread::scope(|scope| {
    for _ in 0..nthreads {
        scope.spawn(|| {
            let mut barrier = init.barrier_ref();
            for i in 0..n / 2 {
                let Ok((head, data)) = sync_blocking!(barrier, |x| {
                    let (head, x) = x[i..].split_at_mut(1);
                    (head[0], iter::split_mut(x, nthreads))
                }) else {
                    break;
                };
                let head = *head;
                let mine = &mut **data;
                for x in mine.iter_mut() {
                    *x += head;
                }
            }
        });
    }
});
for i in 0..n / 2 {
    assert_eq!(x[i], 2.0f64.powi(i as _));
}
```
