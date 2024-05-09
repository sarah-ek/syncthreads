use alloc::sync::Arc;
use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering::SeqCst};
use reborrow::*;
use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};

#[derive(Copy, Clone, Debug)]
pub struct BarrierParams {
    pub spin_iters_before_park: usize,
}
impl Default for BarrierParams {
    fn default() -> Self {
        BarrierParams {
            spin_iters_before_park: DEFAULT_SPIN_ITERS_BEFORE_PARK,
        }
    }
}

pub const DEFAULT_SPIN_ITERS_BEFORE_PARK: usize = 1 << 14;

#[derive(Debug)]
pub struct BarrierInit {
    done: AtomicBool,
    waiting_for_leader: AtomicBool,
    gsense: AtomicBool,
    count: AtomicUsize,
    max: usize,
    params: BarrierParams,
}
#[derive(Debug)]
pub struct Barrier {
    init: Arc<BarrierInit>,
    lsense: bool,
}
#[derive(Debug)]
pub struct BarrierRef<'a> {
    init: &'a BarrierInit,
    lsense: bool,
}
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum BarrierWaitResult {
    Leader,
    Follower,
    Dropped,
}

#[derive(Debug)]
pub struct AdaBarrierInit {
    started: AtomicBool,
    done: AtomicBool,
    waiting_for_leader: AtomicBool,
    count_gsense: AtomicUsize,
    max: AtomicUsize,
    params: BarrierParams,
}
#[derive(Debug)]
pub struct AdaBarrier {
    init: Arc<AdaBarrierInit>,
    lsense: bool,
}
#[derive(Debug)]
pub struct AdaBarrierRef<'a> {
    init: &'a AdaBarrierInit,
    lsense: bool,
}
#[derive(Copy, Clone, Debug)]
pub enum AdaBarrierWaitResult {
    Leader { num_threads: usize },
    Follower,
    Dropped,
}

static GLOBAL_COUNTER: AtomicUsize = AtomicUsize::new(0);
thread_local! {
  static GLOBAL_TID: usize = GLOBAL_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
}

#[derive(Debug)]
pub struct AsyncBarrierInit {
    done: AtomicBool,
    waiting_for_leader: AtomicBool,
    gsense: AtomicBool,
    count: AtomicUsize,
    max: usize,
    tids: Vec<AtomicUsize>,
    blocking: AtomicBool,
    params: BarrierParams,
}
#[derive(Debug)]
pub struct AsyncBarrierRef<'a> {
    init: &'a AsyncBarrierInit,
    lsense: bool,
}
#[derive(Copy, Clone, Debug)]
pub enum AsyncBarrierWaitResult {
    Leader,
    Follower,
    Dropped,
}

impl BarrierInit {
    #[inline]
    pub fn new(num_threads: usize, params: BarrierParams) -> Self {
        Self {
            done: AtomicBool::new(false),
            waiting_for_leader: AtomicBool::new(false),
            count: AtomicUsize::new(num_threads),
            gsense: AtomicBool::new(false),
            max: num_threads,
            params,
        }
    }

    pub fn barrier(self: Arc<Self>) -> Barrier {
        let lsense = false;
        Barrier { init: self, lsense }
    }

    pub fn barrier_ref(&self) -> BarrierRef<'_> {
        let lsense = false;
        BarrierRef { init: self, lsense }
    }
}

impl AdaBarrierInit {
    #[inline]
    pub fn new(params: BarrierParams) -> Self {
        Self {
            started: AtomicBool::new(false),
            done: AtomicBool::new(false),
            waiting_for_leader: AtomicBool::new(false),
            count_gsense: AtomicUsize::new(0),
            max: AtomicUsize::new(0),
            params,
        }
    }

    pub fn barrier(self: Arc<Self>) -> AdaBarrier {
        if self.started.fetch_or(true, SeqCst) {
            let mut count_gsense = self.count_gsense.load(SeqCst);
            loop {
                if count_gsense & !(1usize << (usize::BITS - 1)) == 0 {
                    count_gsense = self.count_gsense.load(SeqCst);
                    continue;
                }
                match self.count_gsense.compare_exchange_weak(
                    count_gsense,
                    count_gsense + 1,
                    SeqCst,
                    SeqCst,
                ) {
                    Ok(_) => break,
                    Err(actual) => {
                        count_gsense = actual;
                    }
                }
            }
            self.max.fetch_add(1, SeqCst);
            let lsense = (count_gsense >> (usize::BITS - 1)) != 0;
            AdaBarrier { init: self, lsense }
        } else {
            let count_gsense = self.count_gsense.fetch_add(1, SeqCst);
            self.max.fetch_add(1, SeqCst);
            let lsense = (count_gsense >> (usize::BITS - 1)) != 0;
            AdaBarrier { init: self, lsense }
        }
    }

    pub fn barrier_ref(&self) -> AdaBarrierRef<'_> {
        if self.started.fetch_or(true, SeqCst) {
            let mut count_gsense = self.count_gsense.load(SeqCst);
            loop {
                if count_gsense & !(1usize << (usize::BITS - 1)) == 0 {
                    count_gsense = self.count_gsense.load(SeqCst);
                    continue;
                }
                match self.count_gsense.compare_exchange_weak(
                    count_gsense,
                    count_gsense + 1,
                    SeqCst,
                    SeqCst,
                ) {
                    Ok(_) => break,
                    Err(actual) => {
                        count_gsense = actual;
                    }
                }
            }
            self.max.fetch_add(1, SeqCst);
            let lsense = (count_gsense >> (usize::BITS - 1)) != 0;
            AdaBarrierRef { init: self, lsense }
        } else {
            let count_gsense = self.count_gsense.fetch_add(1, SeqCst);
            self.max.fetch_add(1, SeqCst);
            let lsense = (count_gsense >> (usize::BITS - 1)) != 0;
            AdaBarrierRef { init: self, lsense }
        }
    }
}

impl AsyncBarrierInit {
    #[inline]
    pub fn new(num_threads: usize, params: BarrierParams) -> Self {
        Self {
            done: AtomicBool::new(false),
            waiting_for_leader: AtomicBool::new(false),
            count: AtomicUsize::new(num_threads),
            gsense: AtomicBool::new(false),
            max: num_threads,
            tids: (0..num_threads).map(|_| AtomicUsize::new(0)).collect(),
            blocking: AtomicBool::new(false),
            params,
        }
    }

    #[inline]
    pub fn num_threads(&self) -> usize {
        self.max
    }

    pub fn barrier_ref(&self) -> AsyncBarrierRef<'_> {
        let lsense = self.gsense.load(SeqCst);
        AsyncBarrierRef { init: self, lsense }
    }
}

macro_rules! impl_barrier {
    ($bar: ty) => {
        impl $bar {
            #[inline(never)]
            pub fn wait(&mut self) -> BarrierWaitResult {
                self.lsense = !self.lsense;
                let addr: &BarrierInit = &*self.init;
                let addr = unsafe { core::mem::transmute::<_, usize>(addr as *const BarrierInit) };

                if (self.init.count.fetch_sub(1, SeqCst)) == 1 {
                    let max = self.init.max;
                    self.init.waiting_for_leader.store(true, SeqCst);
                    self.init.count.store(max, SeqCst);
                    self.init.gsense.store(self.lsense, SeqCst);
                    unsafe {
                        parking_lot_core::unpark_all(addr, parking_lot_core::DEFAULT_UNPARK_TOKEN)
                    };
                    BarrierWaitResult::Leader
                } else {
                    let mut wait = parking_lot_core::SpinWait::new();
                    let mut iters = 0usize;
                    while self.init.gsense.load(SeqCst) != self.lsense {
                        if self.init.done.load(SeqCst) {
                            return BarrierWaitResult::Dropped;
                        }
                        wait.spin();
                        if iters >= self.init.params.spin_iters_before_park {
                            unsafe {
                                parking_lot_core::park(
                                    addr,
                                    || self.init.gsense.load(SeqCst) != self.lsense,
                                    || {},
                                    |_, _| {},
                                    parking_lot_core::DEFAULT_PARK_TOKEN,
                                    None,
                                );
                            }
                        };
                        iters += 1;
                    }
                    BarrierWaitResult::Follower
                }
            }

            #[inline]
            pub fn lead(&self) {
                self.init.waiting_for_leader.store(false, SeqCst);

                let addr: &BarrierInit = &*self.init;
                let addr = unsafe { core::mem::transmute::<_, usize>(addr as *const BarrierInit) };
                unsafe {
                    parking_lot_core::unpark_all(addr, parking_lot_core::DEFAULT_UNPARK_TOKEN)
                };
            }

            #[inline(never)]
            pub fn follow(&self) {
                let addr: &BarrierInit = &*self.init;
                let addr = unsafe { core::mem::transmute::<_, usize>(addr as *const BarrierInit) };

                let mut wait = parking_lot_core::SpinWait::new();
                let mut iters = 0usize;
                while self.init.waiting_for_leader.load(SeqCst) {
                    wait.spin();
                    if iters >= self.init.params.spin_iters_before_park {
                        unsafe {
                            parking_lot_core::park(
                                addr,
                                || self.init.waiting_for_leader.load(SeqCst),
                                || {},
                                |_, _| {},
                                parking_lot_core::DEFAULT_PARK_TOKEN,
                                None,
                            )
                        };
                    }
                    iters += 1;
                }
            }
        }
    };
}

macro_rules! impl_ada_barrier {
    ($bar: ty) => {
        impl $bar {
            #[inline(never)]
            pub fn wait(&mut self) -> AdaBarrierWaitResult {
                self.lsense = !self.lsense;
                let addr: &AdaBarrierInit = &*self.init;
                let addr =
                    unsafe { core::mem::transmute::<_, usize>(addr as *const AdaBarrierInit) };

                if self.init.count_gsense.fetch_sub(1, SeqCst) & !(1usize << (usize::BITS - 1)) == 1
                {
                    let max = self.init.max.load(SeqCst);
                    self.init.waiting_for_leader.store(true, SeqCst);
                    self.init
                        .count_gsense
                        .store(max | ((self.lsense as usize) << (usize::BITS - 1)), SeqCst);
                    unsafe {
                        parking_lot_core::unpark_all(addr, parking_lot_core::DEFAULT_UNPARK_TOKEN)
                    };
                    AdaBarrierWaitResult::Leader { num_threads: max }
                } else {
                    let mut wait = parking_lot_core::SpinWait::new();
                    let mut iters = 0usize;
                    while (self.init.count_gsense.load(SeqCst) >> (usize::BITS - 1) != 0)
                        != self.lsense
                    {
                        if self.init.done.load(SeqCst) {
                            return AdaBarrierWaitResult::Dropped;
                        };
                        wait.spin();

                        if iters >= self.init.params.spin_iters_before_park {
                            unsafe {
                                parking_lot_core::park(
                                    addr,
                                    || {
                                        (self.init.count_gsense.load(SeqCst) >> (usize::BITS - 1)
                                            != 0)
                                            != self.lsense
                                    },
                                    || {},
                                    |_, _| {},
                                    parking_lot_core::DEFAULT_PARK_TOKEN,
                                    None,
                                );
                            }
                        }
                        iters += 1;
                    }
                    AdaBarrierWaitResult::Follower
                }
            }

            pub fn lead(&self) {
                self.init.waiting_for_leader.store(false, SeqCst);

                let addr: &AdaBarrierInit = &*self.init;
                let addr =
                    unsafe { core::mem::transmute::<_, usize>(addr as *const AdaBarrierInit) };
                unsafe {
                    parking_lot_core::unpark_all(addr, parking_lot_core::DEFAULT_UNPARK_TOKEN)
                };
            }

            #[inline(never)]
            pub fn follow(&self) {
                let addr: &AdaBarrierInit = &*self.init;
                let addr =
                    unsafe { core::mem::transmute::<_, usize>(addr as *const AdaBarrierInit) };

                let mut wait = parking_lot_core::SpinWait::new();
                let mut iters = 0usize;
                while self.init.waiting_for_leader.load(SeqCst) {
                    wait.spin();
                    if iters >= self.init.params.spin_iters_before_park {
                        unsafe {
                            parking_lot_core::park(
                                addr,
                                || self.init.waiting_for_leader.load(SeqCst),
                                || {},
                                |_, _| {},
                                parking_lot_core::DEFAULT_PARK_TOKEN,
                                None,
                            )
                        };
                    }
                    iters += 1;
                }
            }
        }

        impl Drop for $bar {
            fn drop(&mut self) {
                self.init.done.store(true, SeqCst);
            }
        }
    };
}

impl<'short> ReborrowMut<'short> for AsyncBarrierRef<'_> {
    type Target = AsyncBarrierRef<'short>;

    #[inline]
    fn rb_mut(&'short mut self) -> Self::Target {
        AsyncBarrierRef {
            init: self.init,
            lsense: self.lsense,
        }
    }
}

impl AsyncBarrierRef<'_> {
    #[inline]
    pub fn num_threads(&self) -> usize {
        self.init.max
    }

    #[inline(never)]
    pub async fn wait(&mut self) -> AsyncBarrierWaitResult {
        self.lsense = !self.lsense;
        let lsense = self.lsense;
        let addr: &AsyncBarrierInit = &*self.init;
        let addr = unsafe { core::mem::transmute::<_, usize>(addr as *const AsyncBarrierInit) };

        let count = self.init.count.fetch_sub(1, SeqCst) - 1;
        let blocking = self.init.blocking.load(SeqCst);
        if !blocking {
            self.init.tids[count].store(GLOBAL_TID.with(|x| *x), SeqCst);
        }
        if count == 0 {
            if !blocking {
                let tids = unsafe {
                    core::slice::from_raw_parts_mut(
                        self.init.tids.as_ptr() as *mut usize,
                        self.init.tids.len(),
                    )
                };
                tids.sort_unstable();
                let mut dup = false;
                for tid in tids.windows(2) {
                    if tid[0] == tid[1] {
                        dup = true;
                    }
                }
                if !dup {
                    self.init.blocking.store(true, SeqCst);
                }
            }

            let max = self.init.max;
            self.init.waiting_for_leader.store(true, SeqCst);
            self.init.count.store(max, SeqCst);
            self.init.gsense.store(lsense, SeqCst);
            if blocking {
                unsafe {
                    parking_lot_core::unpark_all(addr, parking_lot_core::DEFAULT_UNPARK_TOKEN)
                };
            }
            AsyncBarrierWaitResult::Leader
        } else {
            struct Wait<'a> {
                gsense: &'a AtomicBool,
                done: &'a AtomicBool,
                lsense: bool,
            }

            impl Future for Wait<'_> {
                type Output = bool;

                fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
                    if self.gsense.load(SeqCst) != self.lsense {
                        if self.done.load(SeqCst) {
                            Poll::Ready(true)
                        } else {
                            cx.waker().wake_by_ref();
                            Poll::Pending
                        }
                    } else {
                        Poll::Ready(false)
                    }
                }
            }

            if blocking {
                let mut wait = parking_lot_core::SpinWait::new();
                let mut iters = 0usize;
                while self.init.gsense.load(SeqCst) != self.lsense {
                    if self.init.done.load(SeqCst) {
                        return AsyncBarrierWaitResult::Dropped;
                    }
                    wait.spin();
                    if iters >= self.init.params.spin_iters_before_park {
                        unsafe {
                            parking_lot_core::park(
                                addr,
                                || self.init.gsense.load(SeqCst) != self.lsense,
                                || {},
                                |_, _| {},
                                parking_lot_core::DEFAULT_PARK_TOKEN,
                                None,
                            );
                        }
                    };
                    iters += 1;
                }
                AsyncBarrierWaitResult::Follower
            } else {
                if (Wait {
                    gsense: &self.init.gsense,
                    done: &self.init.done,
                    lsense,
                }
                .await)
                {
                    AsyncBarrierWaitResult::Dropped
                } else {
                    AsyncBarrierWaitResult::Follower
                }
            }
        }
    }

    pub fn lead(&self) {
        self.init.waiting_for_leader.store(false, SeqCst);

        let blocking = self.init.blocking.load(SeqCst);
        if blocking {
            let addr: &AsyncBarrierInit = &*self.init;
            let addr = unsafe { core::mem::transmute::<_, usize>(addr as *const AsyncBarrierInit) };
            unsafe { parking_lot_core::unpark_all(addr, parking_lot_core::DEFAULT_UNPARK_TOKEN) };
        }
    }

    #[inline(never)]
    pub async fn follow(&self) {
        struct Wait<'a> {
            waiting_for_leader: &'a AtomicBool,
        }

        impl Future for Wait<'_> {
            type Output = ();

            fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
                if self.waiting_for_leader.load(SeqCst) {
                    cx.waker().wake_by_ref();
                    Poll::Pending
                } else {
                    Poll::Ready(())
                }
            }
        }

        let blocking = self.init.blocking.load(SeqCst);
        if blocking {
            let addr: &AsyncBarrierInit = &*self.init;
            let addr = unsafe { core::mem::transmute::<_, usize>(addr as *const AsyncBarrierInit) };

            let mut wait = parking_lot_core::SpinWait::new();
            let mut iters = 0usize;
            while self.init.waiting_for_leader.load(SeqCst) {
                wait.spin();
                if iters >= self.init.params.spin_iters_before_park {
                    unsafe {
                        parking_lot_core::park(
                            addr,
                            || self.init.waiting_for_leader.load(SeqCst),
                            || {},
                            |_, _| {},
                            parking_lot_core::DEFAULT_PARK_TOKEN,
                            None,
                        )
                    };
                }
                iters += 1;
            }
        } else {
            Wait {
                waiting_for_leader: &self.init.waiting_for_leader,
            }
            .await;
        }
    }
}

impl Drop for AsyncBarrierRef<'_> {
    fn drop(&mut self) {
        self.init.done.store(true, SeqCst);
    }
}

impl_ada_barrier!(AdaBarrier);
impl_ada_barrier!(AdaBarrierRef<'_>);
impl_barrier!(Barrier);
impl_barrier!(BarrierRef<'_>);
