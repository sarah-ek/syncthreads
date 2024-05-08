use aligned_vec::AVec;
use core::cell::UnsafeCell;
use core::mem::MaybeUninit;
use equator::assert;

#[derive(Debug)]
pub struct DynVec {
    data: AVec<UnsafeCell<MaybeUninit<u8>>, aligned_vec::RuntimeAlign>,
    sizeof: usize,
    len: usize,
    drop: unsafe fn(*mut (), usize),
}

impl DynVec {
    pub fn with_capacity(align: usize, capacity: usize) -> Self {
        Self {
            data: AVec::with_capacity(align, capacity),
            sizeof: 0,
            len: 0,
            drop: |_, _| {},
        }
    }

    pub unsafe fn assume_ref<T>(&self) -> &[T] {
        assert!(self.sizeof == core::mem::size_of::<T>());
        core::slice::from_raw_parts(self.data.as_ptr() as *const T, self.len)
    }

    pub fn collect<I: IntoIterator>(&mut self, iter: I) {
        let mut iter = iter.into_iter();

        self.len = 0;
        unsafe { (self.drop)(self.data.as_mut_ptr() as *mut (), self.len) };

        if core::mem::align_of::<I::Item>() > self.data.alignment() {
            self.data = AVec::new(core::mem::align_of::<I::Item>());
        }

        let drop_fn = |ptr, len| unsafe {
            core::ptr::drop_in_place(core::ptr::slice_from_raw_parts_mut(
                ptr as *mut I::Item,
                len,
            ))
        };

        if core::mem::size_of::<I::Item>() == 0 {
            self.sizeof = 0;
            self.drop = drop_fn;
            iter.for_each(|x| {
                self.len += 1;
                core::mem::forget(x);
            });
            return;
        }

        self.data.clear();
        let mut data = core::mem::replace(&mut self.data, AVec::new(1));

        struct Guard<T> {
            ptr: *mut T,
            len: usize,
        }

        impl<T> Drop for Guard<T> {
            fn drop(&mut self) {
                unsafe {
                    core::ptr::drop_in_place(core::ptr::slice_from_raw_parts_mut(
                        self.ptr, self.len,
                    ))
                };
            }
        }

        let mut len;
        {
            let (lower_bound, upper_bound) = iter.size_hint();
            data.reserve(core::mem::size_of::<I::Item>() * lower_bound);

            if upper_bound == Some(lower_bound) {
                let ptr = data.as_mut_ptr() as *mut I::Item;

                let mut guard = Guard {
                    ptr: data.as_mut_ptr() as *mut I::Item,
                    len: 0,
                };
                iter.take(lower_bound).enumerate().for_each(|(i, item)| {
                    unsafe { ptr.add(i).write(item) };
                    guard.len += 1;
                });
                core::mem::forget(guard);

                let (ptr, align, _, cap) = data.into_raw_parts();
                self.data = unsafe {
                    AVec::from_raw_parts(
                        ptr,
                        align,
                        lower_bound * core::mem::size_of::<I::Item>(),
                        cap,
                    )
                };
                len = lower_bound;
            } else {
                let ptr = data.as_mut_ptr() as *mut I::Item;

                let mut guard = Guard {
                    ptr: data.as_mut_ptr() as *mut I::Item,
                    len: 0,
                };
                (&mut iter)
                    .take(lower_bound)
                    .enumerate()
                    .for_each(|(i, item)| {
                        unsafe { ptr.add(i).write(item) };
                        guard.len += 1;
                    });
                core::mem::forget(guard);

                let (ptr, align, _, cap) = data.into_raw_parts();
                let mut data = unsafe {
                    AVec::<_, aligned_vec::RuntimeAlign>::from_raw_parts(
                        ptr as *mut I::Item,
                        align,
                        lower_bound,
                        cap / core::mem::size_of::<I::Item>(),
                    )
                };
                len = lower_bound;

                iter.for_each(|item| {
                    data.push(item);
                    len += 1;
                });
                let (ptr, align, len, cap) = data.into_raw_parts();
                self.data = unsafe {
                    AVec::from_raw_parts(
                        ptr as _,
                        align,
                        len * core::mem::size_of::<I::Item>(),
                        cap * core::mem::size_of::<I::Item>(),
                    )
                };
            }
        }
        self.sizeof = core::mem::size_of::<I::Item>();
        self.drop = drop_fn;
        self.len = len;
    }
}

impl Drop for DynVec {
    fn drop(&mut self) {
        unsafe { (self.drop)(self.data.as_mut_ptr() as *mut (), self.len) }
    }
}
