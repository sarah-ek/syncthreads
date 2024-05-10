use core_affinity::CoreId;
use diol::prelude::*;
use rayon::prelude::*;
use std::sync::atomic::AtomicUsize;
use syncthreads::{with_async_barrier_init, with_barrier_init, AllocHint};

fn sequential(bencher: Bencher, PlotArg(n): PlotArg) {
    let x = &mut *vec![1.0; n];

    bencher.bench(|| {
        x.fill(1.0);
        for i in 0..n / 2 {
            let head = x[i];

            x[i + 1..].iter_mut().for_each(|x| {
                *x += head;
            });
        }
    })
}

fn rayon(bencher: Bencher, PlotArg(n): PlotArg) {
    let x = &mut *vec![1.0; n];

    bencher.bench(|| {
        x.fill(1.0);
        for i in 0..n / 2 {
            let head = x[i];

            x[i + 1..].par_iter_mut().for_each(|x| {
                *x += head;
            });
        }
    })
}

fn rayon_chunk(bencher: Bencher, PlotArg(n): PlotArg) {
    let nthreads = rayon::current_num_threads();
    let x = &mut *vec![1.0; n];

    bencher.bench(|| {
        x.fill(1.0);
        for i in 0..n / 2 {
            let head = x[i];
            let len = x[i + 1..].len();

            if len > 0 {
                x[i + 1..]
                    .par_chunks_mut(len.div_ceil(nthreads))
                    .for_each(|x| {
                        for x in x {
                            *x += head;
                        }
                    });
            }
        }
    })
}

fn barrier(bencher: Bencher, PlotArg(n): PlotArg) {
    let nthreads = rayon::current_num_threads();
    let x = &mut *vec![1.0; n];

    bencher.bench(|| {
        x.fill(1.0);
        with_barrier_init(&mut *x, nthreads, AllocHint::default(), |init, nthreads| {
            let init = &init;

            rayon::in_place_scope(|s| {
                for _ in 0..nthreads.inner() {
                    s.spawn(move |_| {
                        let mut barrier = init.barrier_ref();

                        for i in 0..n / 2 {
                            let Some((head, mine)) = syncthreads::sync!(barrier, |x| {
                                let (head, x) = x[i..].split_at_mut(1);
                                (head[0], syncthreads::iter::split_mut(x, nthreads.inner()))
                            }) else {
                                break;
                            };

                            let head = *head;
                            let mine = &mut **mine;

                            for x in mine.iter_mut() {
                                *x += head;
                            }
                        }
                    });
                }
            });
        });
    })
}

fn async_barrier(bencher: Bencher, PlotArg(n): PlotArg) {
    let nthreads = rayon::current_num_threads();

    static TID: AtomicUsize = AtomicUsize::new(0);
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(nthreads)
        .on_thread_start(|| {
            core_affinity::set_for_current(CoreId {
                id: TID.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            });
        })
        .build()
        .unwrap();
    runtime.spawn(async {});

    let x = &mut *vec![1.0; n];

    bencher.bench(|| {
        with_async_barrier_init(&mut *x, nthreads, AllocHint::default(), |init| {
            tokio_scoped::scoped(runtime.handle()).scope(|scope| {
                for _ in 0..nthreads {
                    scope.spawn(async {
                        let mut barrier = init.barrier_ref();

                        for i in 0..n / 2 {
                            let Some((head, mine)) = syncthreads::sync!(barrier, |x, ()| {
                                let (head, x) = x[i..].split_at_mut(1);
                                (head[0], syncthreads::iter::split_mut(x, nthreads))
                            })
                            .await
                            else {
                                break;
                            };

                            let head = *head;
                            let mine = &mut **mine;

                            for x in mine.iter_mut() {
                                *x += head;
                            }
                        }
                    });
                }
            });
        });
    })
}

fn main() -> std::io::Result<()> {
    rayon::ThreadPoolBuilder::new()
        .start_handler(|tid| {
            core_affinity::set_for_current(CoreId { id: tid });
        })
        .num_threads(core_affinity::get_core_ids().unwrap().len())
        .build_global()
        .unwrap();

    let mut bench = Bench::new(BenchConfig::from_args()?);
    bench.register_many(
        list![async_barrier, barrier, rayon, rayon_chunk, sequential],
        [1_000, 10_000, 100_000, 400_000].map(PlotArg),
    );
    bench.run()?;

    Ok(())
}
