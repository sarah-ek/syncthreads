use alloc::sync::Arc;
use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering::SeqCst};

#[derive(Debug)]
pub struct BarrierInit {
    done: AtomicBool,
    waiting_for_leader: AtomicBool,
    gsense: AtomicBool,
    count: AtomicUsize,
    max: usize,
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

impl BarrierInit {
    #[inline]
    pub fn new(num_threads: usize) -> Self {
        Self {
            done: AtomicBool::new(false),
            waiting_for_leader: AtomicBool::new(false),
            count: AtomicUsize::new(num_threads),
            gsense: AtomicBool::new(false),
            max: num_threads,
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
    pub fn new() -> Self {
        Self {
            started: AtomicBool::new(false),
            done: AtomicBool::new(false),
            waiting_for_leader: AtomicBool::new(false),
            count_gsense: AtomicUsize::new(0),
            max: AtomicUsize::new(0),
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
                    let mut iters = 0;
                    while self.init.gsense.load(SeqCst) != self.lsense {
                        if self.init.done.load(SeqCst) {
                            return BarrierWaitResult::Dropped;
                        }
                        wait.spin();
                        if iters >= 16 {
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
                let mut iters = 0;
                while self.init.waiting_for_leader.load(SeqCst) {
                    wait.spin();
                    if iters >= 16 {
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
                    let mut iters = 0;
                    while (self.init.count_gsense.load(SeqCst) >> (usize::BITS - 1) != 0)
                        != self.lsense
                    {
                        if self.init.done.load(SeqCst) {
                            return AdaBarrierWaitResult::Dropped;
                        };
                        wait.spin();

                        if iters >= 16 {
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
                let mut iters = 0;
                while self.init.waiting_for_leader.load(SeqCst) {
                    wait.spin();
                    if iters >= 16 {
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

impl_ada_barrier!(AdaBarrier);
impl_ada_barrier!(AdaBarrierRef<'_>);
impl_barrier!(Barrier);
impl_barrier!(BarrierRef<'_>);
