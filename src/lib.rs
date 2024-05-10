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
    ctx: UnsafeCell<DynVec>,
    shared: UnsafeCell<DynVec>,
    exclusive: UnsafeCell<DynVec>,
    tid: AtomicUsize,
    tag: UnsafeCell<pretty::TypeId>,
    __marker: PhantomData<Invariant<'brand>>,
}
#[derive(Debug)]
pub struct Barrier<'brand, 'a, T, Ctx = ()> {
    inner: sync::BarrierRef<'a>,
    data: &'a UnsafeCell<T>,
    ctx: &'a UnsafeCell<DynVec>,
    shared: &'a UnsafeCell<DynVec>,
    exclusive: &'a UnsafeCell<DynVec>,
    tid: ThreadId<'brand>,
    tag: &'a UnsafeCell<pretty::TypeId>,
    __marker: PhantomData<&'a UnsafeCell<Ctx>>,
}
#[derive(Debug)]
pub struct AsyncBarrierInit<'brand, T> {
    inner: sync::AsyncBarrierInit,
    data: UnsafeCell<T>,
    ctx: UnsafeCell<DynVec>,
    shared: UnsafeCell<DynVec>,
    exclusive: UnsafeCell<DynVec>,
    tid: AtomicUsize,
    tag: UnsafeCell<pretty::TypeId>,
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
    tag: &'a UnsafeCell<pretty::TypeId>,
    __marker: PhantomData<&'a UnsafeCell<Ctx>>,
}

unsafe impl<T: Sync + Send> Sync for BarrierInit<'_, T> {}
unsafe impl<T: Sync + Send> Send for BarrierInit<'_, T> {}
unsafe impl<T: Sync + Send> Sync for Barrier<'_, '_, T> {}
unsafe impl<T: Sync + Send> Send for Barrier<'_, '_, T> {}

unsafe impl<T: Sync + Send> Sync for AsyncBarrierInit<'_, T> {}
unsafe impl<T: Sync + Send> Send for AsyncBarrierInit<'_, T> {}
unsafe impl<T: Sync + Send, Ctx: Sync + Send> Sync for AsyncBarrier<'_, '_, T, Ctx> {}
unsafe impl<T: Sync + Send, Ctx: Sync + Send> Send for AsyncBarrier<'_, '_, T, Ctx> {}

#[derive(Debug, Default)]
pub struct AllocHint {
    pub shared: Alloc,
    pub exclusive: Alloc,
    pub ctx: Alloc,
}

#[derive(Debug)]
pub struct Storage {
    alloc: DynVec,
}

#[derive(Debug)]
pub enum Alloc {
    Hint {
        size_bytes: usize,
        align_bytes: usize,
    },
    Storage(Storage),
}

impl Alloc {
    fn make_vec(self) -> DynVec {
        match self {
            Alloc::Hint {
                size_bytes,
                align_bytes,
            } => DynVec::with_capacity(align_bytes, size_bytes),
            Alloc::Storage(storage) => storage.alloc,
        }
    }
}

impl Default for Alloc {
    #[inline]
    fn default() -> Self {
        Self::Hint {
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
            ctx: UnsafeCell::new(hint.ctx.make_vec()),
            shared: UnsafeCell::new(hint.shared.make_vec()),
            exclusive: UnsafeCell::new(hint.exclusive.make_vec()),
            tid: AtomicUsize::new(0),
            tag: UnsafeCell::new(type_id_of_val(&())),
            __marker: PhantomData,
        },
        ThreadCount {
            inner: num_threads,
            __marker: PhantomData,
        },
    )
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
            let mut v = hint.ctx.make_vec();
            v.collect(core::iter::once(UnsafeCell::new(())));
            v
        }),
        shared: UnsafeCell::new(hint.shared.make_vec()),
        exclusive: UnsafeCell::new(hint.exclusive.make_vec()),
        tid: AtomicUsize::new(0),
        tag: UnsafeCell::new(type_id_of_val(&())),
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
            ctx: &self.ctx,
            shared: &self.shared,
            exclusive: &self.exclusive,
            tid: ThreadId {
                inner: self.tid.fetch_add(1, core::sync::atomic::Ordering::Relaxed),
                __no_sync_marker: PhantomData,
            },
            tag: &self.tag,
            __marker: PhantomData,
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

impl<'brand, 'a, T, Ctx> Barrier<'brand, 'a, T, Ctx> {
    pub unsafe fn sync<'b, Shared: Sync, Exclusive: Send, I: IntoIterator<Item = Exclusive>>(
        &'b mut self,
        f: impl FnOnce(&'a mut T) -> (Shared, I),
        tag: impl core::any::Any,
    ) -> Option<(&'b Shared, &'b mut Exclusive)> {
        let tag = type_id_of_val(&tag);
        match self.inner.wait() {
            sync::BarrierWaitResult::Leader => {
                let exclusive = unsafe { &mut *self.exclusive.get() };
                let shared = unsafe { &mut *self.shared.get() };

                let f = f(unsafe { &mut *self.data.get() });
                shared.collect(core::iter::once(f.0));
                exclusive.collect(f.1.into_iter().map(UnsafeCell::new).map(CachePadded::new));
                unsafe { *self.tag.get() = tag };
                self.inner.lead();
            }
            sync::BarrierWaitResult::Follower => {
                self.inner.follow();
            }
            sync::BarrierWaitResult::Dropped => return None,
        }
        let self_tag = unsafe { *self.tag.get() };
        equator::assert!(tag == self_tag);

        Some((
            unsafe { &(&*self.shared.get()).assume_ref::<Shared>()[0] },
            unsafe {
                &mut *((&*self.exclusive.get()).assume_ref::<CachePadded<UnsafeCell<Exclusive>>>()
                    [self.tid.inner]
                    .get())
            },
        ))
    }

    pub unsafe fn map_mut<'b, NewCtx: 'a>(
        &'b mut self,
        f: impl FnOnce(Ctx) -> NewCtx,
        tag: impl core::any::Any,
    ) -> Option<AsyncBarrier<'b, 'a, T, NewCtx>> {
        let tag = type_id_of_val(&tag);
        match self.inner.wait() {
            sync::BarrierWaitResult::Leader => {
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
            sync::BarrierWaitResult::Follower => {
                self.inner.follow();
            }
            sync::BarrierWaitResult::Dropped => return None,
        }
        let self_tag = unsafe { *self.tag.get() };
        equator::assert!(tag == self_tag);

        Some(unsafe {
            core::mem::transmute(Barrier::<'brand, 'b, T, NewCtx> {
                inner: self.inner.rb_mut(),
                data: self.data,
                ctx: self.ctx,
                shared: self.shared,
                exclusive: self.exclusive,
                tid: self.tid,
                tag: self.tag,
                __marker: PhantomData,
            })
        })
    }

    #[inline]
    pub fn id(&self) -> ThreadId<'brand> {
        self.tid
    }
}

fn type_id_of_val<T: 'static>(_: &T) -> pretty::TypeId {
    pretty::TypeId {
        id: core::any::TypeId::of::<T>(),
        name: core::any::type_name::<T>(),
    }
}

#[macro_export]
macro_rules! sync {
    ($bar: expr, $f:expr) => {{
        #[allow(unused_unsafe)]
        let x = unsafe { ($bar).sync($f, || {}) };
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

mod pretty {
    #[derive(Copy, Clone, Debug)]
    pub struct TypeId {
        pub id: core::any::TypeId,
        #[allow(dead_code)]
        pub name: &'static str,
    }

    impl PartialEq for TypeId {
        #[inline]
        fn eq(&self, other: &Self) -> bool {
            self.id == other.id
        }
    }
}

impl<'brand, 'a, T, Ctx> AsyncBarrier<'brand, 'a, T, Ctx> {
    pub async unsafe fn sync<
        'b,
        Shared: 'b + Sync,
        Exclusive: 'b + Send,
        I: IntoIterator<Item = Exclusive>,
    >(
        &'b mut self,
        f: impl FnOnce(&'a mut T, &'a mut Ctx) -> (Shared, I),
        tag: impl core::any::Any,
    ) -> Option<(&'b Shared, &'b mut Exclusive)> {
        let tag = type_id_of_val(&tag);
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
        tag: impl core::any::Any,
    ) -> Option<AsyncBarrier<'b, 'a, T, NewCtx>> {
        let tag = type_id_of_val(&tag);
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

    const N: usize = 1000;
    const ITERS: usize = 1;
    const SAMPLES: usize = 1;

    fn default<T: Default>() -> T {
        T::default()
    }

    #[test]
    fn test_barrier() {
        let n = N;
        let x = &mut *vec![1.0; n];
        let nthreads = rayon::current_num_threads();

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
                                    let Some((head, data)) = sync!(barrier, |x| {
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
                });
            }
            dbg!(now.elapsed());
        }
    }

    #[test]
    fn test_barrier_threadpool() {
        let n = N;
        let nthreads = rayon::current_num_threads();
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
                let nthreads = rayon::current_num_threads();
                with_barrier_init(&mut *x, nthreads, default(), |init, nthreads| {
                    let init = &init;
                    threadpool_scope::scope_with(&pool, |scope| {
                        for _ in 0..nthreads.inner() {
                            scope.execute(move || {
                                let mut barrier = init.barrier_ref();

                                for i in 0..n / 2 {
                                    let Some((&head, mine)) = sync!(barrier, |x| {
                                        let (head, x) = x[i..].split_at_mut(1);
                                        (head[0], iter::split_mut(x, nthreads.inner()))
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
    fn oversubscription() {
        use rayon::prelude::*;

        dbg!("tokio");
        static TID: AtomicUsize = AtomicUsize::new(0);
        {
            let runtime = &tokio::runtime::Builder::new_multi_thread()
                .worker_threads(rayon::current_num_threads())
                .on_thread_start(|| {
                    core_affinity::set_for_current(CoreId {
                        id: TID.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
                    });
                })
                .build()
                .unwrap();
            tokio_scoped::scoped(runtime.handle()).scope(|scope| {
                for _ in 0..rayon::current_num_threads() {
                    scope.spawn(async move {
                        with_runtime(runtime);
                    });
                }
            });
        }
        dbg!("tokio");
        (0..rayon::current_num_threads())
            .into_par_iter()
            .for_each(|_| {
                let runtime = &tokio::runtime::Builder::new_multi_thread()
                    .worker_threads(rayon::current_num_threads())
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
        (0..rayon::current_num_threads())
            .into_par_iter()
            .for_each(|_| test_barrier_rayon());
        dbg!("seq");
        (0..rayon::current_num_threads())
            .into_par_iter()
            .for_each(|_| test_seq());
    }

    #[test]
    fn test_barrier_rayon() {
        let n = N;
        let nthreads = rayon::current_num_threads();

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
                                    let Some((head, mine)) = sync!(barrier, |x| {
                                        let (head, x) = x[i..].split_at_mut(1);
                                        (head[0], iter::split_mut(x, nthreads.inner()))
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
            .worker_threads(rayon::current_num_threads())
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
        let nthreads = rayon::current_num_threads();

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
            }
            dbg!(now.elapsed());
        }
    }

    #[test]
    fn test_pollster() {
        let nthreads = rayon::current_num_threads();

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
                    })));
                });
            }
            dbg!(now.elapsed());
        }
    }
}
