use alloc::sync::Arc;
use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering::SeqCst};
use crossbeam::queue::SegQueue;
use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll, Waker},
};

#[derive(Copy, Clone, Debug, Default)]
#[non_exhaustive]
pub struct BarrierParams {
    pub spin_iters_before_park: SpinIters,
}
#[derive(Copy, Clone, Debug, Default)]
#[non_exhaustive]
pub struct AsyncBarrierParams {
    pub spin_iters_before_park: SpinIters,
}

#[derive(Copy, Clone, Debug)]
pub struct SpinIters(pub usize);

impl Default for SpinIters {
    fn default() -> Self {
        Self(DEFAULT_SPIN_ITERS_BEFORE_PARK)
    }
}
const SHIFT: u32 = usize::BITS - 1;
const HIGH_BIT: usize = 1 << SHIFT;
const LOW_MASK: usize = !HIGH_BIT;

pub const DEFAULT_SPIN_ITERS_BEFORE_PARK: usize = 1 << 14;
const DEFAULT_SPIN_ITERS_BEFORE_SLEEPY: usize = 16;

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

#[derive(Debug)]
pub struct AsyncBarrierInit {
    done: AtomicBool,
    waiting_for_leader: AtomicUsize,
    gsense: AtomicUsize,
    count: AtomicUsize,
    wait_wakers: [SegQueue<Waker>; 2],
    follow_wakers: [SegQueue<Waker>; 2],
    max: usize,
    params: AsyncBarrierParams,
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

    #[inline]
    pub fn num_threads(&self) -> usize {
        self.max
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
                if count_gsense & LOW_MASK == 0 {
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
            let lsense = count_gsense >> SHIFT != 0;
            AdaBarrier { init: self, lsense }
        } else {
            let count_gsense = self.count_gsense.fetch_add(1, SeqCst);
            self.max.fetch_add(1, SeqCst);
            let lsense = count_gsense >> SHIFT != 0;
            AdaBarrier { init: self, lsense }
        }
    }

    pub fn barrier_ref(&self) -> AdaBarrierRef<'_> {
        if self.started.fetch_or(true, SeqCst) {
            let mut count_gsense = self.count_gsense.load(SeqCst);
            loop {
                if count_gsense & LOW_MASK == 0 {
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
            let lsense = count_gsense >> SHIFT != 0;
            AdaBarrierRef { init: self, lsense }
        } else {
            let count_gsense = self.count_gsense.fetch_add(1, SeqCst);
            self.max.fetch_add(1, SeqCst);
            let lsense = count_gsense >> SHIFT != 0;
            AdaBarrierRef { init: self, lsense }
        }
    }
}

impl AsyncBarrierInit {
    #[inline]
    pub fn new(num_threads: usize, params: AsyncBarrierParams) -> Self {
        Self {
            done: AtomicBool::new(false),
            waiting_for_leader: AtomicUsize::new(0),
            count: AtomicUsize::new(num_threads),
            gsense: AtomicUsize::new(0),
            wait_wakers: [SegQueue::new(), SegQueue::new()],
            follow_wakers: [SegQueue::new(), SegQueue::new()],
            max: num_threads,
            params,
        }
    }

    #[inline]
    pub fn num_threads(&self) -> usize {
        self.max
    }

    pub fn barrier_ref(&self) -> AsyncBarrierRef<'_> {
        let lsense = self.gsense.load(SeqCst) >> SHIFT == 1;
        AsyncBarrierRef { init: self, lsense }
    }
}

macro_rules! impl_barrier {
    ($bar: ty) => {
        impl $bar {
            #[inline]
            pub fn num_threads(&self) -> usize {
                self.init.max
            }

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
                    loop {
                        let done = self.init.done.load(SeqCst);
                        let keep_going = self.init.gsense.load(SeqCst) != self.lsense;
                        if !keep_going {
                            break;
                        }
                        if done {
                            return BarrierWaitResult::Dropped;
                        }
                        wait.spin();
                        if iters >= self.init.params.spin_iters_before_park.0 {
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
                    if iters >= self.init.params.spin_iters_before_park.0 {
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

                if self.init.count_gsense.fetch_sub(1, SeqCst) & LOW_MASK == 1 {
                    let max = self.init.max.load(SeqCst);
                    self.init.waiting_for_leader.store(true, SeqCst);
                    self.init
                        .count_gsense
                        .store(max | ((self.lsense as usize) << SHIFT), SeqCst);
                    unsafe {
                        parking_lot_core::unpark_all(addr, parking_lot_core::DEFAULT_UNPARK_TOKEN)
                    };
                    AdaBarrierWaitResult::Leader { num_threads: max }
                } else {
                    let mut wait = parking_lot_core::SpinWait::new();
                    let mut iters = 0usize;
                    loop {
                        let done = self.init.done.load(SeqCst);
                        let keep_going =
                            self.init.count_gsense.load(SeqCst) >> SHIFT != self.lsense as usize;
                        if !keep_going {
                            break;
                        }
                        if done {
                            return AdaBarrierWaitResult::Dropped;
                        };
                        wait.spin();

                        if iters >= self.init.params.spin_iters_before_park.0 {
                            unsafe {
                                parking_lot_core::park(
                                    addr,
                                    || {
                                        self.init.count_gsense.load(SeqCst) >> SHIFT
                                            != self.lsense as usize
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
                    if iters >= self.init.params.spin_iters_before_park.0 {
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

impl AsyncBarrierRef<'_> {
    #[inline]
    pub fn num_threads(&self) -> usize {
        self.init.max
    }

    #[inline(never)]
    pub async fn wait(&mut self) -> AsyncBarrierWaitResult {
        self.lsense = !self.lsense;
        let lsense = self.lsense;
        let wakers = &self.init.wait_wakers[lsense as usize];

        let count = self.init.count.fetch_sub(1, SeqCst) - 1;
        if count == 0 {
            let max = self.init.max;
            self.init.waiting_for_leader.store(HIGH_BIT, SeqCst);
            self.init.count.store(max, SeqCst);
            let mut wakers_count = self.init.gsense.fetch_xor(HIGH_BIT, SeqCst) & LOW_MASK;
            while wakers_count > 0 {
                if let Some(waker) = wakers.pop() {
                    waker.wake();
                    wakers_count -= 1;
                    self.init.gsense.fetch_sub(1, SeqCst);
                }
            }

            AsyncBarrierWaitResult::Leader
        } else {
            struct Wait<'a> {
                gsense: &'a AtomicUsize,
                done: &'a AtomicBool,
                lsense: bool,
                wakers: &'a SegQueue<Waker>,
                iters: usize,
                params: &'a AsyncBarrierParams,
            }

            impl Future for Wait<'_> {
                type Output = bool;

                fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
                    let lsense = self.lsense as usize;
                    let mut done = self.done.load(SeqCst);
                    let mut gsense = self.gsense.load(SeqCst);
                    let iter = self.iters;
                    self.iters += 1;

                    if iter < self.params.spin_iters_before_park.0 {
                        if gsense >> SHIFT != lsense {
                            if done {
                                Poll::Ready(true)
                            } else {
                                if iter >= DEFAULT_SPIN_ITERS_BEFORE_SLEEPY {
                                    std::thread::yield_now();
                                }
                                cx.waker().wake_by_ref();
                                Poll::Pending
                            }
                        } else {
                            Poll::Ready(false)
                        }
                    } else {
                        loop {
                            if gsense >> SHIFT == lsense {
                                return Poll::Ready(false);
                            }

                            match self.gsense.compare_exchange_weak(
                                gsense,
                                gsense + 1,
                                SeqCst,
                                SeqCst,
                            ) {
                                Ok(_) => {
                                    if done {
                                        return Poll::Ready(true);
                                    }
                                    self.wakers.push(cx.waker().clone());
                                    return Poll::Pending;
                                }
                                Err(new) => {
                                    done = self.done.load(SeqCst);
                                    gsense = new;
                                }
                            }
                        }
                    }
                }
            }

            if (Wait {
                gsense: &self.init.gsense,
                done: &self.init.done,
                lsense,
                wakers,
                iters: 0,
                params: &self.init.params,
            }
            .await)
            {
                AsyncBarrierWaitResult::Dropped
            } else {
                AsyncBarrierWaitResult::Follower
            }
        }
    }

    pub fn lead(&self) {
        let lsense = self.lsense;
        let wakers = &self.init.follow_wakers[lsense as usize];
        let mut wakers_count = self.init.waiting_for_leader.fetch_and(LOW_MASK, SeqCst) & LOW_MASK;
        while wakers_count > 0 {
            if let Some(waker) = wakers.pop() {
                waker.wake();
                wakers_count -= 1;
            }
        }
    }

    #[inline(never)]
    pub async fn follow(&self) {
        struct Wait<'a> {
            waiting_for_leader: &'a AtomicUsize,
            wakers: &'a SegQueue<Waker>,
            iters: usize,
            params: &'a AsyncBarrierParams,
        }

        impl Future for Wait<'_> {
            type Output = ();

            fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
                let iter = self.iters;
                self.iters += 1;
                let mut waiting_for_leader = self.waiting_for_leader.load(SeqCst);
                if iter < self.params.spin_iters_before_park.0 {
                    if waiting_for_leader >> SHIFT == 1 {
                        if iter >= DEFAULT_SPIN_ITERS_BEFORE_SLEEPY {
                            std::thread::yield_now();
                        }
                        cx.waker().wake_by_ref();
                        Poll::Pending
                    } else {
                        Poll::Ready(())
                    }
                } else {
                    loop {
                        if waiting_for_leader >> SHIFT == 0 {
                            return Poll::Ready(());
                        }

                        match self.waiting_for_leader.compare_exchange_weak(
                            waiting_for_leader,
                            waiting_for_leader + 1,
                            SeqCst,
                            SeqCst,
                        ) {
                            Ok(_) => {
                                self.wakers.push(cx.waker().clone());
                                return Poll::Pending;
                            }
                            Err(new) => waiting_for_leader = new,
                        }
                    }
                }
            }
        }

        let lsense = self.lsense;
        let wakers = &self.init.follow_wakers[lsense as usize];
        Wait {
            waiting_for_leader: &self.init.waiting_for_leader,
            iters: 0,
            wakers,
            params: &self.init.params,
        }
        .await;
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
