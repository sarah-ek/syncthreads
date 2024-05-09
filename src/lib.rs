use core::any::TypeId;
use core::cell::UnsafeCell;
use core::fmt;
use core::marker::PhantomData;
use core::sync::atomic::AtomicUsize;
use crossbeam::utils::CachePadded;
use reborrow::*;

extern crate alloc;

pub mod iter;
pub mod sync;

mod dyn_vec;
use dyn_vec::DynVec;

#[derive(Copy, Clone)]
struct Invariant<'brand> {
    __invariant: PhantomData<fn(&'brand ()) -> &'brand ()>,
}

#[derive(Copy, Clone)]
pub struct ThreadCount<'brand> {
    inner: usize,
    __marker: PhantomData<Invariant<'brand>>,
}

impl ThreadId<'_> {
    #[inline]
    pub fn inner(self) -> usize {
        self.inner
    }
}

impl ThreadCount<'_> {
    #[inline]
    pub fn inner(self) -> usize {
        self.inner
    }
}

#[derive(Copy, Clone)]
pub struct ThreadId<'brand> {
    inner: usize,
    __no_sync_marker: PhantomData<(*mut (), Invariant<'brand>)>,
}

impl fmt::Debug for ThreadId<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ThreadId")
            .field("inner", &self.inner)
            .finish()
    }
}

impl fmt::Debug for ThreadCount<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ThreadCount")
            .field("inner", &self.inner)
            .finish()
    }
}

#[derive(Debug)]
pub struct BarrierInit<'brand, T> {
    inner: sync::BarrierInit,
    data: UnsafeCell<T>,
    shared: UnsafeCell<DynVec>,
    exclusive: UnsafeCell<DynVec>,

    tid: AtomicUsize,
    __marker: PhantomData<Invariant<'brand>>,
}
#[derive(Debug)]
pub struct Barrier<'brand, 'a, T> {
    inner: sync::BarrierRef<'a>,
    data: &'a UnsafeCell<T>,
    shared: &'a UnsafeCell<DynVec>,
    exclusive: &'a UnsafeCell<DynVec>,

    tid: ThreadId<'brand>,
}
#[derive(Debug)]
pub struct AdaBarrierInit<'brand, T> {
    inner: sync::AdaBarrierInit,
    data: UnsafeCell<T>,
    shared: UnsafeCell<DynVec>,
    exclusive: UnsafeCell<DynVec>,
    tid: AtomicUsize,
    __marker: PhantomData<Invariant<'brand>>,
}
#[derive(Debug)]
pub struct AdaBarrier<'brand, 'a, T> {
    inner: sync::AdaBarrierRef<'a>,
    data: &'a UnsafeCell<T>,
    shared: &'a UnsafeCell<DynVec>,
    exclusive: &'a UnsafeCell<DynVec>,
    id: ThreadId<'brand>,
}
#[derive(Debug)]
pub struct AsyncBarrierInit<'brand, T> {
    inner: sync::AsyncBarrierInit,
    data: UnsafeCell<T>,
    ctx: UnsafeCell<DynVec>,
    shared: UnsafeCell<DynVec>,
    exclusive: UnsafeCell<DynVec>,
    tid: AtomicUsize,
    tag: UnsafeCell<TypeId>,
    __marker: PhantomData<Invariant<'brand>>,
}
#[derive(Debug)]
pub struct AsyncBarrier<'brand, 'a, T, Ctx = ()> {
    inner: sync::AsyncBarrierRef<'a>,
    data: &'a UnsafeCell<T>,
    ctx: &'a UnsafeCell<DynVec>,
    shared: &'a UnsafeCell<DynVec>,
    exclusive: &'a UnsafeCell<DynVec>,
    id: ThreadId<'brand>,
    tag: &'a UnsafeCell<TypeId>,
    __marker: PhantomData<&'a UnsafeCell<Ctx>>,
}

unsafe impl<T: Sync + Send> Sync for BarrierInit<'_, T> {}
unsafe impl<T: Sync + Send> Send for BarrierInit<'_, T> {}
unsafe impl<T: Sync + Send> Sync for Barrier<'_, '_, T> {}
unsafe impl<T: Sync + Send> Send for Barrier<'_, '_, T> {}

unsafe impl<T: Sync + Send> Sync for AdaBarrierInit<'_, T> {}
unsafe impl<T: Sync + Send> Send for AdaBarrierInit<'_, T> {}
unsafe impl<T: Sync + Send> Sync for AdaBarrier<'_, '_, T> {}
unsafe impl<T: Sync + Send> Send for AdaBarrier<'_, '_, T> {}

unsafe impl<T: Sync + Send> Sync for AsyncBarrierInit<'_, T> {}
unsafe impl<T: Sync + Send> Send for AsyncBarrierInit<'_, T> {}
unsafe impl<T: Sync + Send, Ctx: Sync + Send> Sync for AsyncBarrier<'_, '_, T, Ctx> {}
unsafe impl<T: Sync + Send, Ctx: Sync + Send> Send for AsyncBarrier<'_, '_, T, Ctx> {}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Default)]
pub struct AllocHint {
    pub shared: AllocLayout,
    pub exclusive: AllocLayout,
    pub ctx: AllocLayout,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct AllocLayout {
    pub size_bytes: usize,
    pub align_bytes: usize,
}

impl Default for AllocLayout {
    #[inline]
    fn default() -> Self {
        Self {
            size_bytes: 0,
            align_bytes: 1,
        }
    }
}

#[inline]
pub fn with_barrier_init<T, R>(
    value: T,
    num_threads: usize,
    hint: AllocHint,
    f: impl for<'brand> FnOnce(BarrierInit<'brand, T>, ThreadCount) -> R,
) -> R {
    f(
        BarrierInit {
            inner: sync::BarrierInit::new(num_threads, Default::default()),
            data: UnsafeCell::new(value),
            shared: UnsafeCell::new(DynVec::with_capacity(
                hint.shared.align_bytes,
                hint.shared.size_bytes,
            )),
            exclusive: UnsafeCell::new(DynVec::with_capacity(
                hint.exclusive.align_bytes,
                hint.exclusive.size_bytes,
            )),

            tid: AtomicUsize::new(0),
            __marker: PhantomData,
        },
        ThreadCount {
            inner: num_threads,
            __marker: PhantomData,
        },
    )
}

#[inline]
pub fn with_adaptive_barrier_init<T, R>(
    value: T,
    hint: AllocHint,
    f: impl for<'brand> FnOnce(AdaBarrierInit<'brand, T>) -> R,
) -> R {
    f(AdaBarrierInit {
        inner: sync::AdaBarrierInit::new(Default::default()),
        data: UnsafeCell::new(value),
        shared: UnsafeCell::new(DynVec::with_capacity(
            hint.shared.align_bytes,
            hint.shared.size_bytes,
        )),
        exclusive: UnsafeCell::new(DynVec::with_capacity(
            hint.exclusive.align_bytes,
            hint.exclusive.size_bytes,
        )),
        tid: AtomicUsize::new(0),
        __marker: PhantomData,
    })
}

#[inline]
pub fn with_async_barrier_init<T, R>(
    value: T,
    num_threads: usize,
    hint: AllocHint,
    f: impl for<'brand> FnOnce(AsyncBarrierInit<'brand, T>) -> R,
) -> R {
    f(AsyncBarrierInit {
        inner: sync::AsyncBarrierInit::new(num_threads, Default::default()),
        data: UnsafeCell::new(value),
        ctx: UnsafeCell::new({
            let mut v = DynVec::with_capacity(hint.ctx.align_bytes, hint.ctx.size_bytes);
            v.collect(core::iter::once(UnsafeCell::new(())));
            v
        }),
        shared: UnsafeCell::new(DynVec::with_capacity(
            hint.shared.align_bytes,
            hint.shared.size_bytes,
        )),
        exclusive: UnsafeCell::new(DynVec::with_capacity(
            hint.exclusive.align_bytes,
            hint.exclusive.size_bytes,
        )),
        tid: AtomicUsize::new(0),
        tag: UnsafeCell::new(TypeId::of::<()>()),
        __marker: PhantomData,
    })
}

impl<'brand, T> BarrierInit<'brand, T> {
    pub fn into_inner(self) -> T {
        self.data.into_inner()
    }

    pub fn barrier_ref<'a>(&'a self) -> Barrier<'brand, 'a, T> {
        Barrier {
            inner: self.inner.barrier_ref(),
            data: &self.data,
            shared: &self.shared,
            exclusive: &self.exclusive,

            tid: ThreadId {
                inner: self.tid.fetch_add(1, core::sync::atomic::Ordering::Relaxed),
                __no_sync_marker: PhantomData,
            },
        }
    }
}

impl<'brand, T> AdaBarrierInit<'brand, T> {
    pub fn into_inner(self) -> T {
        self.data.into_inner()
    }

    pub fn barrier_ref(&self) -> AdaBarrier<'brand, '_, T> {
        AdaBarrier {
            inner: self.inner.barrier_ref(),
            data: &self.data,
            shared: &self.shared,
            exclusive: &self.exclusive,
            id: ThreadId {
                inner: self.tid.fetch_add(1, core::sync::atomic::Ordering::Relaxed),
                __no_sync_marker: PhantomData,
            },
        }
    }
}

impl<'brand, T> AsyncBarrierInit<'brand, T> {
    pub fn into_inner(self) -> T {
        self.data.into_inner()
    }

    pub fn barrier_ref(&self) -> AsyncBarrier<'brand, '_, T> {
        AsyncBarrier {
            inner: self.inner.barrier_ref(),
            data: &self.data,
            ctx: &self.ctx,
            shared: &self.shared,
            exclusive: &self.exclusive,
            id: ThreadId {
                inner: self.tid.fetch_add(1, core::sync::atomic::Ordering::Relaxed),
                __no_sync_marker: PhantomData,
            },
            tag: &self.tag,
            __marker: PhantomData,
        }
    }

    #[inline]
    pub fn thread_count(&self) -> ThreadCount<'brand> {
        ThreadCount {
            inner: self.inner.num_threads(),
            __marker: PhantomData,
        }
    }

    pub fn reset(&mut self) {
        *self.tid.get_mut() = 0;
    }

    pub fn map<U>(self, f: impl FnOnce(T) -> U) -> AsyncBarrierInit<'brand, U> {
        let mut this = self;
        this.reset();
        AsyncBarrierInit {
            inner: this.inner,
            data: UnsafeCell::new(f(this.data.into_inner())),
            ctx: this.ctx,
            shared: this.shared,
            exclusive: this.exclusive,
            tid: this.tid,
            tag: this.tag,
            __marker: PhantomData,
        }
    }
}

impl<'brand, 'a, T> Barrier<'brand, 'a, T> {
    pub unsafe fn sync<'b, Shared, Exclusive, I: IntoIterator<Item = Exclusive>>(
        &'b mut self,
        f: impl FnOnce(&'a mut T) -> (Shared, I),
    ) -> Option<(&'b Shared, &'b mut Exclusive)> {
        match self.inner.wait() {
            sync::BarrierWaitResult::Leader => {
                let exclusive = unsafe { &mut *self.exclusive.get() };
                let shared = unsafe { &mut *self.shared.get() };

                let f = f(unsafe { &mut *self.data.get() });
                shared.collect(core::iter::once(f.0));
                exclusive.collect(f.1.into_iter().map(UnsafeCell::new).map(CachePadded::new));
                self.inner.lead();
            }
            sync::BarrierWaitResult::Follower => {
                self.inner.follow();
            }
            sync::BarrierWaitResult::Dropped => return None,
        }
        Some((
            unsafe { &(&*self.shared.get()).assume_ref::<Shared>()[0] },
            unsafe {
                &mut *((&*self.exclusive.get()).assume_ref::<CachePadded<UnsafeCell<Exclusive>>>()
                    [self.tid.inner]
                    .get())
            },
        ))
    }

    #[inline]
    pub fn id(&self) -> ThreadId<'brand> {
        self.tid
    }
}

impl<'brand, 'a, T> AdaBarrier<'brand, 'a, T> {
    pub unsafe fn sync<'b, Shared, Exclusive, I: IntoIterator<Item = Exclusive>>(
        &'b mut self,
        f: impl FnOnce(usize, &'a mut T) -> (Shared, I),
    ) -> Option<(&'b Shared, &'b mut Exclusive, ThreadId<'b>, ThreadCount<'b>)> {
        match self.inner.wait() {
            sync::AdaBarrierWaitResult::Leader { num_threads } => {
                let exclusive = unsafe { &mut *self.exclusive.get() };
                let shared = unsafe { &mut *self.shared.get() };

                let f = f(num_threads, unsafe { &mut *self.data.get() });
                shared.collect(core::iter::once((num_threads, f.0)));
                exclusive.collect(f.1.into_iter().map(UnsafeCell::new).map(CachePadded::new));
                self.inner.lead();
            }
            sync::AdaBarrierWaitResult::Follower => {
                self.inner.follow();
            }
            sync::AdaBarrierWaitResult::Dropped => return None,
        }
        let shared = &unsafe { (&*self.shared.get()).assume_ref::<(usize, Shared)>() }[0];
        Some((
            &shared.1,
            unsafe {
                &mut *((&*self.exclusive.get()).assume_ref::<CachePadded<UnsafeCell<Exclusive>>>()
                    [self.id.inner]
                    .get())
            },
            ThreadId {
                inner: self.id.inner,
                __no_sync_marker: PhantomData,
            },
            ThreadCount {
                inner: shared.0,
                __marker: PhantomData,
            },
        ))
    }
}

fn type_id<T: 'static>(_: &T) -> TypeId {
    TypeId::of::<T>()
}

#[macro_export]
macro_rules! sync {
    ($bar: expr, $f:expr) => {{
        #[allow(unused_unsafe)]
        let x = unsafe { ($bar).sync($f, || {}).await };
        x
    }};
}

#[macro_export]
macro_rules! map_mut {
    ($bar: expr, $f:expr) => {{
        #[allow(unused_unsafe)]
        let x = unsafe { ($bar).map_mut($f, || {}).await };
        x
    }};
}

impl<'brand, 'a, T, Ctx> AsyncBarrier<'brand, 'a, T, Ctx> {
    pub async unsafe fn sync<'b, Shared: 'b, Exclusive: 'b, I: IntoIterator<Item = Exclusive>>(
        &'b mut self,
        f: impl FnOnce(&'a mut T, &'a mut Ctx) -> (Shared, I),
        tag: impl 'static + Sized,
    ) -> Option<(&'b Shared, &'b mut Exclusive)> {
        let tag = type_id(&tag);
        match self.inner.wait().await {
            sync::AsyncBarrierWaitResult::Leader => {
                let exclusive = unsafe { &mut *self.exclusive.get() };
                let shared = unsafe { &mut *self.shared.get() };
                let ctx = unsafe { &*self.ctx.get() };

                let f = f(unsafe { &mut *self.data.get() }, unsafe {
                    &mut *((&ctx.assume_ref::<UnsafeCell<Ctx>>()[0]).get())
                });
                shared.collect(core::iter::once(f.0));
                exclusive.collect(f.1.into_iter().map(UnsafeCell::new).map(CachePadded::new));
                unsafe { *self.tag.get() = tag };
                self.inner.lead();
            }
            sync::AsyncBarrierWaitResult::Follower => {
                self.inner.follow().await;
            }
            sync::AsyncBarrierWaitResult::Dropped => return None,
        }

        let self_tag = unsafe { *self.tag.get() };
        equator::assert!(tag == self_tag);
        let shared = &unsafe { (&*self.shared.get()).assume_ref::<Shared>() }[0];
        Some((shared, unsafe {
            &mut *((&*self.exclusive.get()).assume_ref::<CachePadded<UnsafeCell<Exclusive>>>()
                [self.id.inner]
                .get())
        }))
    }

    pub async unsafe fn map_mut<'b, NewCtx: 'a>(
        &'b mut self,
        f: impl FnOnce(Ctx) -> NewCtx,
        tag: impl 'static + Sized,
    ) -> Option<AsyncBarrier<'b, 'a, T, NewCtx>> {
        let tag = type_id(&tag);
        match self.inner.wait().await {
            sync::AsyncBarrierWaitResult::Leader => {
                let ctx_vec = unsafe { &mut *self.ctx.get() };
                let ctx = unsafe {
                    core::ptr::from_ref(&ctx_vec.assume_ref::<UnsafeCell<Ctx>>()[0]).read()
                };
                ctx_vec.len = 0;

                let ctx = f(ctx.into_inner());
                ctx_vec.collect(core::iter::once(UnsafeCell::new(ctx)));

                unsafe { *self.tag.get() = tag };
                self.inner.lead();
            }
            sync::AsyncBarrierWaitResult::Follower => {
                self.inner.follow().await;
            }
            sync::AsyncBarrierWaitResult::Dropped => return None,
        }
        let self_tag = unsafe { *self.tag.get() };
        equator::assert!(tag == self_tag);

        Some(unsafe {
            core::mem::transmute(AsyncBarrier::<'brand, 'b, T, NewCtx> {
                inner: self.inner.rb_mut(),
                data: self.data,
                ctx: self.ctx,
                shared: self.shared,
                exclusive: self.exclusive,
                id: self.id,
                tag: self.tag,
                __marker: PhantomData,
            })
        })
    }

    #[inline]
    pub fn id(&self) -> ThreadId<'brand> {
        self.id
    }

    #[inline]
    pub fn thread_count(&self) -> ThreadCount<'brand> {
        ThreadCount {
            inner: self.inner.num_threads(),
            __marker: PhantomData,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core_affinity::CoreId;
    use futures::future::join_all;

    const N: usize = 10;
    const ITERS: usize = 1;
    const SAMPLES: usize = 1;

    fn default<T: Default>() -> T {
        T::default()
    }

    #[test]
    fn test_barrier() {
        let n = N;
        let x = &mut *vec![1.0; n];
        let nthreads = 6;

        for _ in 0..SAMPLES {
            let now = std::time::Instant::now();
            for _ in 0..ITERS {
                with_barrier_init(&mut *x, nthreads, default(), |init, _| {
                    let init = &init;
                    std::thread::scope(|s| {
                        for _ in 0..nthreads {
                            s.spawn(move || {
                                let mut barrier = init.barrier_ref();

                                for i in 0..n / 2 {
                                    let Some((head, data)) = (unsafe {
                                        barrier.sync(|x| {
                                            let (head, x) = x[i..].split_at_mut(1);

                                            (head[0], iter::split_mut(x, nthreads))
                                        })
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
                });
            }
            dbg!(now.elapsed());
        }
    }

    #[test]
    fn test_barrier_threadpool() {
        let n = N;
        let nthreads = 6;
        let pool = threadpool::ThreadPool::new(nthreads);
        for tid in 0..nthreads {
            pool.execute(move || {
                core_affinity::set_for_current(CoreId { id: tid });
                std::thread::sleep(std::time::Duration::from_secs_f64(0.1));
            });
        }
        pool.join();

        for _ in 0..SAMPLES {
            let now = std::time::Instant::now();
            for _ in 0..ITERS {
                let x = &mut *vec![1.0; n];
                let nthreads = 6;
                with_barrier_init(&mut *x, nthreads, default(), |init, nthreads| {
                    let init = &init;
                    threadpool_scope::scope_with(&pool, |scope| {
                        for _ in 0..nthreads.inner() {
                            scope.execute(move || {
                                let mut barrier = init.barrier_ref();

                                for i in 0..n / 2 {
                                    let Some((&head, mine)) = (unsafe {
                                        barrier.sync(|x| {
                                            let (head, x) = x[i..].split_at_mut(1);
                                            (head[0], iter::split_mut(x, nthreads.inner()))
                                        })
                                    }) else {
                                        break;
                                    };
                                    let mine = &mut **mine;
                                    for x in mine.iter_mut() {
                                        *x += head;
                                    }
                                }
                            });
                        }
                    });
                });
            }
            dbg!(now.elapsed());
        }
    }

    #[test]
    fn test_ada_barrier_rayon() {
        let n = N;
        let x = &mut *vec![1.0; n];
        let nthreads = 6;

        let pool = rayon::ThreadPoolBuilder::new()
            .start_handler(|tid| {
                core_affinity::set_for_current(CoreId { id: tid });
            })
            .num_threads(nthreads)
            .build()
            .unwrap();

        for _ in 0..SAMPLES {
            let now = std::time::Instant::now();
            for _ in 0..ITERS {
                enum Phase {
                    Init,
                    Iter(usize),
                }
                with_adaptive_barrier_init((Phase::Init, &mut *x), default(), |init| {
                    let init = &init;

                    pool.in_place_scope(|scope| {
                        for _ in 0..nthreads {
                            scope.spawn(move |_| {
                                let mut barrier = init.barrier_ref();

                                loop {
                                    let Some((&(i, head), mine, _id, _nthreads)) = (unsafe {
                                        barrier.sync(|nthreads, (phase, x)| {
                                            let i = match phase {
                                                Phase::Init => {
                                                    *phase = Phase::Iter(0);
                                                    0
                                                }
                                                Phase::Iter(i) => {
                                                    *i = *i + 1;
                                                    *i
                                                }
                                            };
                                            let (head, x) = x[i..].split_at_mut(1);
                                            ((i, head[0]), iter::split_mut(x, nthreads))
                                        })
                                    }) else {
                                        break;
                                    };

                                    if i >= n / 2 {
                                        break;
                                    }

                                    for x in &mut **mine {
                                        *x += head;
                                    }
                                }
                            });
                        }
                    });
                });
            }
            dbg!(now.elapsed());
        }
    }

    #[test]
    fn test_ada_barrier_threadpool() {
        let n = N;
        let x = &mut *vec![1.0; n];
        let nthreads = 6;

        let pool = threadpool::ThreadPool::new(nthreads);
        for tid in 0..nthreads {
            pool.execute(move || {
                core_affinity::set_for_current(CoreId { id: tid });
                std::thread::sleep(std::time::Duration::from_secs_f64(0.1));
            });
        }
        pool.join();

        for _ in 0..SAMPLES {
            let now = std::time::Instant::now();
            for _ in 0..ITERS {
                enum Phase {
                    Init,
                    Iter(usize),
                }
                with_adaptive_barrier_init((Phase::Init, &mut *x), default(), |init| {
                    let init = &init;

                    threadpool_scope::scope_with(&pool, |scope| {
                        for _ in 0..nthreads {
                            scope.execute(move || {
                                let mut barrier = init.barrier_ref();

                                loop {
                                    let Some((&(i, head), mine, _id, _nthreads)) = (unsafe {
                                        barrier.sync(|nthreads, (phase, x)| {
                                            let i = match phase {
                                                Phase::Init => {
                                                    *phase = Phase::Iter(0);
                                                    0
                                                }
                                                Phase::Iter(i) => {
                                                    *i = *i + 1;
                                                    *i
                                                }
                                            };
                                            let (head, x) = x[i..].split_at_mut(1);
                                            ((i, head[0]), iter::split_mut(x, nthreads))
                                        })
                                    }) else {
                                        break;
                                    };

                                    if i >= n / 2 {
                                        break;
                                    }

                                    for x in &mut **mine {
                                        *x += head;
                                    }
                                }
                            });
                        }
                    });
                });
            }
            dbg!(now.elapsed());
        }
    }

    #[test]
    fn oversubscription() {
        use rayon::prelude::*;

        dbg!("tokio");
        static TID: AtomicUsize = AtomicUsize::new(0);
        {
            let runtime = &tokio::runtime::Builder::new_multi_thread()
                .worker_threads(6)
                .on_thread_start(|| {
                    core_affinity::set_for_current(CoreId {
                        id: TID.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
                    });
                })
                .build()
                .unwrap();
            tokio_scoped::scoped(runtime.handle()).scope(|scope| {
                for _ in 0..6 {
                    scope.spawn(async move {
                        with_runtime(runtime);
                    });
                }
            });
        }
        dbg!("tokio");
        (0..6).into_par_iter().for_each(|_| {
            let runtime = &tokio::runtime::Builder::new_multi_thread()
                .worker_threads(6)
                .on_thread_start(|| {
                    core_affinity::set_for_current(CoreId {
                        id: TID.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
                    });
                })
                .build()
                .unwrap();
            tokio_scoped::scoped(runtime.handle()).scope(|scope| {
                scope.spawn(async move {
                    with_runtime(runtime);
                });
            });
        });

        dbg!("rayon");
        (0..6).into_par_iter().for_each(|_| test_barrier_rayon());
        dbg!("ada rayon");
        (0..6)
            .into_par_iter()
            .for_each(|_| test_ada_barrier_rayon());
        dbg!("seq");
        (0..6).into_par_iter().for_each(|_| test_seq());
    }

    #[test]
    fn test_barrier_rayon() {
        let n = N;
        let nthreads = 6;

        let pool = rayon::ThreadPoolBuilder::new()
            .start_handler(|tid| {
                core_affinity::set_for_current(CoreId { id: tid });
            })
            .num_threads(nthreads)
            .build()
            .unwrap();

        for _ in 0..SAMPLES {
            let now = std::time::Instant::now();
            for _ in 0..ITERS {
                let x = &mut *vec![1.0; n];
                with_barrier_init(&mut *x, nthreads, default(), |init, nthreads| {
                    let init = &init;

                    pool.in_place_scope(|s| {
                        for _ in 0..nthreads.inner() {
                            s.spawn(move |_| {
                                let mut barrier = init.barrier_ref();

                                for i in 0..n / 2 {
                                    let Some((head, mine)) = (unsafe {
                                        barrier.sync(|x| {
                                            let (head, x) = x[i..].split_at_mut(1);
                                            (head[0], iter::split_mut(x, nthreads.inner()))
                                        })
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
            }
            dbg!(now.elapsed());
        }
    }

    #[test]
    fn test_rayon() {
        use rayon::prelude::*;
        let n = N;
        let x = &mut *vec![1.0; n];
        for _ in 0..SAMPLES {
            let now = std::time::Instant::now();
            for _ in 0..ITERS {
                for i in 0..n / 2 {
                    let [head, x @ ..] = &mut x[i..] else {
                        panic!()
                    };

                    let head = *head;
                    let len = x.len();
                    if len > 0 {
                        x.par_chunks_mut(len.div_ceil(rayon::current_num_threads()))
                            .for_each(|x| {
                                for x in x {
                                    *x += head;
                                }
                            })
                    }
                }
            }
            dbg!(now.elapsed());
        }
    }

    #[test]
    fn test_seq() {
        let n = N;
        let x = &mut *vec![1.0; n];
        for _ in 0..SAMPLES {
            let now = std::time::Instant::now();
            for _ in 0..ITERS {
                for i in 0..n / 2 {
                    let head = x[i];
                    for x in &mut x[i + 1..] {
                        *x += head;
                    }
                }
            }
            dbg!(now.elapsed());
        }
    }

    #[test]
    fn test_tokio() {
        static TID: AtomicUsize = AtomicUsize::new(0);
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(6)
            .on_thread_start(|| {
                core_affinity::set_for_current(CoreId {
                    id: TID.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
                });
            })
            .build()
            .unwrap();
        with_runtime(&runtime);
    }

    fn with_runtime(runtime: &tokio::runtime::Runtime) {
        let nthreads = 6;

        for _ in 0..SAMPLES {
            let now = std::time::Instant::now();
            for _ in 0..ITERS {
                let n = N;
                let x = &mut *vec![1.0; n];

                with_async_barrier_init(&mut *x, nthreads, AllocHint::default(), |init| {
                    tokio_scoped::scoped(runtime.handle()).scope(|scope| {
                        for _ in 0..nthreads {
                            scope.spawn(async {
                                let mut barrier = init.barrier_ref();

                                for i in 0..n / 2 {
                                    let Some((head, mine)) = sync!(barrier, |x, ()| {
                                        let (head, x) = x[i..].split_at_mut(1);
                                        (head[0], iter::split_mut(x, nthreads))
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
            }
            dbg!(now.elapsed());
        }
    }

    #[test]
    fn test_pollster() {
        let nthreads = 6;

        for _ in 0..SAMPLES {
            let now = std::time::Instant::now();
            for _ in 0..ITERS {
                let n = N;
                let x = &mut *vec![1.0; n];

                with_async_barrier_init(&mut *x, nthreads, AllocHint::default(), |init| {
                    pollster::block_on(join_all((0..nthreads).map(|_| async {
                        let mut barrier = init.barrier_ref();

                        for i in 0..n / 2 {
                            let Some((head, mine)) = sync!(barrier, |x, ()| {
                                let (head, x) = x[i..].split_at_mut(1);
                                (head[0], iter::split_mut(x, nthreads))
                            }) else {
                                break;
                            };

                            let head = *head;
                            let mine = &mut **mine;

                            for x in mine.iter_mut() {
                                *x += head;
                            }
                        }
                    })));
                });
            }
            dbg!(now.elapsed());
        }
    }
}
