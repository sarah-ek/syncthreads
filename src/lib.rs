use core::{cell::UnsafeCell, fmt, marker::PhantomData, sync::atomic::AtomicUsize};
use crossbeam::utils::CachePadded;
use reborrow::*;

extern crate alloc;

/// Iterator utilities for splitting tasks between a fixed number of threads.
pub mod iter;
/// Low level synchronization primitives.
pub mod sync;

mod dyn_vec;
use dyn_vec::DynVec;
use sync::{AsyncBarrierParams, BarrierParams};

#[derive(Copy, Clone, Debug)]
pub struct DropError;

impl fmt::Display for DropError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        <Self as fmt::Debug>::fmt(self, f)
    }
}

impl std::error::Error for DropError {}

/// Structure used to initialize instances of [`Barrier`].
#[derive(Debug)]
pub struct BarrierInit<T> {
    inner: sync::BarrierInit,
    data: UnsafeCell<T>,
    ctx: UnsafeCell<DynVec>,
    shared: UnsafeCell<DynVec>,
    exclusive: UnsafeCell<DynVec>,
    tid: AtomicUsize,
    tag: UnsafeCell<pretty::TypeId>,
}

/// Synchronous barrier for cooperation between multiple independent threads.
#[derive(Debug)]
pub struct Barrier<'a, T, Ctx = ()> {
    inner: sync::BarrierRef<'a>,
    data: &'a UnsafeCell<T>,
    ctx: &'a UnsafeCell<DynVec>,
    shared: &'a UnsafeCell<DynVec>,
    exclusive: &'a UnsafeCell<DynVec>,
    tid: usize,
    tag: &'a UnsafeCell<pretty::TypeId>,
    __marker: PhantomData<&'a UnsafeCell<Ctx>>,
}
/// Structure used to initialize instances of [`AsyncBarrier`].
#[derive(Debug)]
pub struct AsyncBarrierInit<T> {
    inner: sync::AsyncBarrierInit,
    data: UnsafeCell<T>,
    ctx: UnsafeCell<DynVec>,
    shared: UnsafeCell<DynVec>,
    exclusive: UnsafeCell<DynVec>,
    tid: AtomicUsize,
    tag: UnsafeCell<pretty::TypeId>,
}

/// Synchronous barrier for cooperation between multiple independent tasks.
#[derive(Debug)]
pub struct AsyncBarrier<'a, T, Ctx = ()> {
    inner: sync::AsyncBarrierRef<'a>,
    data: &'a UnsafeCell<T>,
    ctx: &'a UnsafeCell<DynVec>,
    shared: &'a UnsafeCell<DynVec>,
    exclusive: &'a UnsafeCell<DynVec>,
    tid: usize,
    tag: &'a UnsafeCell<pretty::TypeId>,
    __marker: PhantomData<&'a UnsafeCell<Ctx>>,
}

unsafe impl<T: Sync + Send> Sync for BarrierInit<T> {}
unsafe impl<T: Sync + Send> Send for BarrierInit<T> {}
unsafe impl<T: Sync + Send> Sync for Barrier<'_, T> {}
unsafe impl<T: Sync + Send> Send for Barrier<'_, T> {}

unsafe impl<T: Sync + Send> Sync for AsyncBarrierInit<T> {}
unsafe impl<T: Sync + Send> Send for AsyncBarrierInit<T> {}
unsafe impl<T: Sync + Send, Ctx: Sync + Send> Sync for AsyncBarrier<'_, T, Ctx> {}
unsafe impl<T: Sync + Send, Ctx: Sync + Send> Send for AsyncBarrier<'_, T, Ctx> {}

/// Allocation hint provided to barrier initializers.
#[derive(Debug, Default)]
pub struct AllocHint {
    pub shared: Alloc,
    pub exclusive: Alloc,
    pub ctx: Alloc,
}

/// Pre-allocated storage.
#[derive(Debug)]
pub struct Storage {
    alloc: DynVec,
}

/// Allocation or hint provided for barrier initializers.
#[derive(Debug)]
pub enum Alloc {
    /// Initial allocation size hint.
    Hint {
        /// Size of the allocation.
        size_bytes: usize,
        /// Alignment of the allocation.
        align_bytes: usize,
    },
    /// Pre-allocated storage.
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

impl<T> BarrierInit<T> {
    /// Creates a new [`BarrierInit`] protecting the given value, with the specified number of
    /// threads.
    pub fn new(value: T, num_threads: usize, hint: AllocHint, params: BarrierParams) -> Self {
        BarrierInit {
            inner: sync::BarrierInit::new(num_threads, params),
            data: UnsafeCell::new(value),
            ctx: UnsafeCell::new(hint.ctx.make_vec()),
            shared: UnsafeCell::new(hint.shared.make_vec()),
            exclusive: UnsafeCell::new(hint.exclusive.make_vec()),
            tid: AtomicUsize::new(0),
            tag: UnsafeCell::new(type_id_of_val(&())),
        }
    }

    /// Consumes `self`, returning the protected value and the current allocation.
    pub fn into_inner(self) -> (T, AllocHint) {
        (
            self.data.into_inner(),
            AllocHint {
                shared: Alloc::Storage(Storage {
                    alloc: self.shared.into_inner(),
                }),
                exclusive: Alloc::Storage(Storage {
                    alloc: self.exclusive.into_inner(),
                }),
                ctx: Alloc::Storage(Storage {
                    alloc: self.ctx.into_inner(),
                }),
            },
        )
    }

    /// Creates a new barrier referencing `self`.
    ///
    /// # Panics
    /// Panics if more than `self.thread_count()` barriers have been created since the creation of
    /// `self`, or the last time [`Self::reset`] was called.
    pub fn barrier_ref(&self) -> Barrier<'_, T> {
        let tid = self.tid.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
        assert!(tid < self.num_threads());
        Barrier {
            inner: self.inner.barrier_ref(),
            data: &self.data,
            ctx: &self.ctx,
            shared: &self.shared,
            exclusive: &self.exclusive,
            tid,
            tag: &self.tag,
            __marker: PhantomData,
        }
    }

    /// Returns the number of threads the barriers need to wait for.
    #[inline]
    pub fn num_threads(&self) -> usize {
        self.inner.num_threads()
    }

    /// Resets the barrier initializer.
    pub fn reset(&mut self) {
        *self.tid.get_mut() = 0;
    }
}

impl<T> AsyncBarrierInit<T> {
    /// Creates a new [`AsyncBarrierInit`] protecting the given value, with the specified number of
    /// threads.
    pub fn new(value: T, num_threads: usize, hint: AllocHint, params: AsyncBarrierParams) -> Self {
        AsyncBarrierInit {
            inner: sync::AsyncBarrierInit::new(num_threads, params),
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
        }
    }

    /// Consumes `self`, returning the protected value and the current allocation.
    pub fn into_inner(self) -> (T, AllocHint) {
        (
            self.data.into_inner(),
            AllocHint {
                shared: Alloc::Storage(Storage {
                    alloc: self.shared.into_inner(),
                }),
                exclusive: Alloc::Storage(Storage {
                    alloc: self.exclusive.into_inner(),
                }),
                ctx: Alloc::Storage(Storage {
                    alloc: self.ctx.into_inner(),
                }),
            },
        )
    }

    /// Creates a new barrier referencing `self`.
    ///
    /// # Panics
    /// Panics if more than `self.thread_count()` barriers have been created since the creation of
    /// `self`, or the last time [`Self::reset`] was called.
    pub fn barrier_ref(&self) -> AsyncBarrier<'_, T> {
        let tid = self.tid.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
        assert!(tid < self.num_threads());
        AsyncBarrier {
            inner: self.inner.barrier_ref(),
            data: &self.data,
            ctx: &self.ctx,
            shared: &self.shared,
            exclusive: &self.exclusive,
            tid,
            tag: &self.tag,
            __marker: PhantomData,
        }
    }

    /// Returns the number of tasks the barriers need to wait for.
    #[inline]
    pub fn num_threads(&self) -> usize {
        self.inner.num_threads()
    }

    /// Resets the barrier initializer.
    pub fn reset(&mut self) {
        *self.tid.get_mut() = 0;
    }
}

impl<'a, T, Ctx> Barrier<'a, T, Ctx> {
    /// Waits until `self.num_threads()` threads have called this function, at which point the last
    /// thread to arrive will call `f` with a reference to the data, splitting it into a shared
    /// section and mutable section. These are respectively sent to the other threads through a
    /// shared and exclusive reference.
    ///
    /// # Safety
    /// Threads from the same group that wait at this function at the same time must agree on the
    /// same types for `Shared`, `Exclusive`, and the type of `tag`.
    pub unsafe fn sync_blocking<
        'b,
        Shared: Sync,
        Exclusive: Send,
        I: IntoIterator<Item = Exclusive>,
    >(
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
                    [self.tid]
                    .get())
            },
        ))
    }

    /// Waits until `self.num_threads()` threads have called this function, at which point the last
    /// thread to arrive will call `f` with the context of the group, and store the resulting value
    /// as the new context.
    ///
    /// # Safety
    /// Threads from the same group that wait at this function at the same time must agree on the
    /// same types for `NewCtx`, and the type of `tag`.
    pub unsafe fn map_blocking<NewCtx: 'a>(
        self,
        f: impl FnOnce(Ctx) -> NewCtx,
        tag: impl core::any::Any,
    ) -> Option<Barrier<'a, T, NewCtx>> {
        let mut this = self;
        let tag = type_id_of_val(&tag);
        match this.inner.wait() {
            sync::BarrierWaitResult::Leader => {
                let ctx_vec = unsafe { &mut *this.ctx.get() };
                let ctx = unsafe {
                    core::ptr::from_ref(&ctx_vec.assume_ref::<UnsafeCell<Ctx>>()[0]).read()
                };
                ctx_vec.len = 0;

                let ctx = f(ctx.into_inner());
                ctx_vec.collect(core::iter::once(UnsafeCell::new(ctx)));

                unsafe { *this.tag.get() = tag };
                this.inner.lead();
            }
            sync::BarrierWaitResult::Follower => {
                this.inner.follow();
            }
            sync::BarrierWaitResult::Dropped => return None,
        }
        let this_tag = unsafe { *this.tag.get() };
        equator::assert!(tag == this_tag);

        Some(Barrier::<'a, T, NewCtx> {
            inner: this.inner,
            data: this.data,
            ctx: this.ctx,
            shared: this.shared,
            exclusive: this.exclusive,
            tid: this.tid,
            tag: this.tag,
            __marker: PhantomData,
        })
    }

    /// Returns the unique (among the barriers created by the same initializer) id of `self`, which
    /// is a value between `0` and `self.num_threads()`.
    #[inline]
    pub fn thread_id(&self) -> usize {
        self.tid
    }

    /// Returns the number of threads the barriers need to wait for.
    #[inline]
    pub fn num_threads(&self) -> usize {
        self.inner.num_threads()
    }
}

impl<'a, T, Ctx> AsyncBarrier<'a, T, Ctx> {
    /// Waits until `self.num_threads()` threads have called this function, at which point the last
    /// thread to arrive will call `f` with a reference to the data, splitting it into a shared
    /// section and mutable section. These are respectively sent to the other threads through a
    /// shared and exclusive reference.
    ///
    /// # Safety
    /// Threads from the same group that wait at this function at the same time must agree on the
    /// same types for `Shared`, `Exclusive`, and the type of `tag`.
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
                [self.tid]
                .get())
        }))
    }

    /// Waits until `self.num_threads()` threads have called this function, at which point the last
    /// thread to arrive will call `f` with the context of the group, and store the resulting value
    /// as the new context.
    ///
    /// # Safety
    /// Threads from the same group that wait at this function at the same time must agree on the
    /// same types for `NewCtx`, and the type of `tag`.
    pub async unsafe fn map<NewCtx: 'a>(
        self,
        f: impl FnOnce(Ctx) -> NewCtx,
        tag: impl core::any::Any,
    ) -> Option<AsyncBarrier<'a, T, NewCtx>> {
        let mut this = self;
        let tag = type_id_of_val(&tag);
        match this.inner.wait().await {
            sync::AsyncBarrierWaitResult::Leader => {
                let ctx_vec = unsafe { &mut *this.ctx.get() };
                let ctx = unsafe {
                    core::ptr::from_ref(&ctx_vec.assume_ref::<UnsafeCell<Ctx>>()[0]).read()
                };
                ctx_vec.len = 0;

                let ctx = f(ctx.into_inner());
                ctx_vec.collect(core::iter::once(UnsafeCell::new(ctx)));

                unsafe { *this.tag.get() = tag };
                this.inner.lead();
            }
            sync::AsyncBarrierWaitResult::Follower => {
                this.inner.follow().await;
            }
            sync::AsyncBarrierWaitResult::Dropped => return None,
        }
        let this_tag = unsafe { *this.tag.get() };
        equator::assert!(tag == this_tag);

        Some(AsyncBarrier::<'a, T, NewCtx> {
            inner: this.inner,
            data: this.data,
            ctx: this.ctx,
            shared: this.shared,
            exclusive: this.exclusive,
            tid: this.tid,
            tag: this.tag,
            __marker: PhantomData,
        })
    }

    /// Returns the unique (among the barriers created by the same initializer) id of `self`, which
    /// is a value between `0` and `self.num_threads()`.
    #[inline]
    pub fn thread_id(&self) -> usize {
        self.tid
    }

    /// Returns the number of threads the barriers need to wait for.
    #[inline]
    pub fn thread_count(&self) -> usize {
        self.inner.num_threads()
    }
}

impl<'short, T, Ctx> ReborrowMut<'short> for Barrier<'_, T, Ctx> {
    type Target = Barrier<'short, T, Ctx>;

    #[inline]
    fn rb_mut(&'short mut self) -> Self::Target {
        Barrier {
            inner: self.inner.rb_mut(),
            data: self.data,
            ctx: self.ctx,
            shared: self.shared,
            exclusive: self.exclusive,
            tid: self.tid,
            tag: self.tag,
            __marker: PhantomData,
        }
    }
}

impl<'short, T, Ctx> ReborrowMut<'short> for AsyncBarrier<'_, T, Ctx> {
    type Target = AsyncBarrier<'short, T, Ctx>;

    #[inline]
    fn rb_mut(&'short mut self) -> Self::Target {
        AsyncBarrier {
            inner: self.inner.rb_mut(),
            data: self.data,
            ctx: self.ctx,
            shared: self.shared,
            exclusive: self.exclusive,
            tid: self.tid,
            tag: self.tag,
            __marker: PhantomData,
        }
    }
}

fn type_id_of_val<T: 'static>(_: &T) -> pretty::TypeId {
    pretty::TypeId {
        id: core::any::TypeId::of::<T>(),
        name: pretty::Str(core::any::type_name::<T>()),
    }
}

#[macro_export]
macro_rules! sync_await {
    ($bar: expr, $f:expr) => {{
        #[allow(unused_unsafe)]
        let x = unsafe { ($bar).sync($f, || {}).await };
        x
    }};
}

#[macro_export]
macro_rules! sync_blocking {
    ($bar: expr, $f:expr) => {{
        #[allow(unused_unsafe)]
        let x = unsafe { ($bar).sync_blocking($f, || {}) };
        x
    }};
}

#[macro_export]
macro_rules! map_mut_await {
    ($bar: expr, $f:expr) => {{
        #[allow(unused_unsafe)]
        let x = unsafe { ($bar).map_mut($f, || {}).await };
        x
    }};
}

#[macro_export]
macro_rules! map_mut_blocking {
    ($bar: expr, $f:expr) => {{
        #[allow(unused_unsafe)]
        let x = unsafe { ($bar).map_mut_blocking($f, || {}) };
        x
    }};
}

mod pretty {
    use core::fmt;

    #[derive(Copy, Clone)]
    pub struct Str(pub &'static str);

    impl fmt::Debug for Str {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str(self.0)
        }
    }

    #[derive(Copy, Clone, Debug)]
    pub struct TypeId {
        #[allow(dead_code)]
        pub name: Str,
        pub id: core::any::TypeId,
    }

    impl PartialEq for TypeId {
        #[inline]
        fn eq(&self, other: &Self) -> bool {
            self.id == other.id
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
                let init = BarrierInit::new(&mut *x, nthreads, default(), default());
                std::thread::scope(|s| {
                    for _ in 0..nthreads {
                        s.spawn(|| {
                            let mut barrier = init.barrier_ref();

                            for i in 0..n / 2 {
                                let Some((head, data)) = sync_blocking!(barrier, |x| {
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
                let init = BarrierInit::new(&mut *x, nthreads, default(), default());
                threadpool_scope::scope_with(&pool, |scope| {
                    for _ in 0..nthreads {
                        scope.execute(|| {
                            let mut barrier = init.barrier_ref();

                            for i in 0..n / 2 {
                                let Some((&head, mine)) = sync_blocking!(barrier, |x| {
                                    let (head, x) = x[i..].split_at_mut(1);
                                    (head[0], iter::split_mut(x, nthreads))
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
                let init = BarrierInit::new(&mut *x, nthreads, default(), default());
                pool.in_place_scope(|s| {
                    for _ in 0..nthreads {
                        s.spawn(|_| {
                            let mut barrier = init.barrier_ref();

                            for i in 0..n / 2 {
                                let Some((head, mine)) = sync_blocking!(barrier, |x| {
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
        let nthreads = 3;

        for _ in 0..SAMPLES {
            let now = std::time::Instant::now();
            for _ in 0..ITERS {
                let n = N;
                let x = &mut *vec![1.0; n];

                let init =
                    AsyncBarrierInit::new(&mut *x, nthreads, AllocHint::default(), default());
                tokio_scoped::scoped(runtime.handle()).scope(|scope| {
                    for _ in 0..nthreads {
                        scope.spawn(async {
                            let mut barrier = init.barrier_ref();

                            for i in 0..n / 2 {
                                let Some((head, mine)) = sync_await!(barrier, |x, ()| {
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

                let init =
                    AsyncBarrierInit::new(&mut *x, nthreads, AllocHint::default(), default());
                pollster::block_on(join_all((0..nthreads).map(|_| async {
                    let mut barrier = init.barrier_ref();

                    for i in 0..n / 2 {
                        let Some((head, mine)) = sync_await!(barrier, |x, ()| {
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
            }
            dbg!(now.elapsed());
        }
    }

    #[test]
    #[should_panic]
    fn test_branchy() {
        let nthreads = rayon::current_num_threads();

        for _ in 0..SAMPLES {
            let now = std::time::Instant::now();
            for _ in 0..ITERS {
                let n = N;
                let x = &mut *vec![1.0; n];

                let init =
                    AsyncBarrierInit::new(&mut *x, nthreads, AllocHint::default(), default());
                pollster::block_on(join_all((0..nthreads).map(|_| async {
                    let mut barrier = init.barrier_ref();

                    for i in 0..n / 2 {
                        if barrier.thread_id() == 0 {
                            let Some((head, mine)) = sync_await!(barrier, |x, ()| {
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
                        } else {
                            let Some((head, mine)) = sync_await!(barrier, |x, ()| {
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
                    }
                })));
            }
            dbg!(now.elapsed());
        }
    }
}
