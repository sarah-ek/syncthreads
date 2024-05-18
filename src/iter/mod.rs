/// Iterator obtained from [`split`].
pub struct Split<'a, T> {
    slice: &'a [T],
    div: usize,
    count: usize,
}

/// Iterator obtained from [`split_mut`].
pub struct SplitMut<'a, T> {
    slice: &'a mut [T],
    div: usize,
    count: usize,
}

/// Returns an iterator producing `chunk_count` contiguous slices of `slice`.
#[inline]
pub fn split<T>(slice: &[T], chunk_count: usize) -> Split<'_, T> {
    let div = slice.len().div_ceil(chunk_count);
    Split {
        slice,
        div,
        count: chunk_count,
    }
}

/// Returns an iterator producing `chunk_count` contiguous slices of `slice`.
#[inline]
pub fn split_mut<T>(slice: &mut [T], chunk_count: usize) -> SplitMut<'_, T> {
    let div = slice.len().div_ceil(chunk_count);
    SplitMut {
        slice,
        div,
        count: chunk_count,
    }
}

impl<'a, T> Iterator for SplitMut<'a, T> {
    type Item = &'a mut [T];

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        if self.count == 0 {
            None
        } else {
            let len = self.slice.len();
            let next;
            (next, self.slice) =
                core::mem::take(&mut self.slice).split_at_mut(Ord::min(self.div, len));
            self.count -= 1;
            Some(next)
        }
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.count, Some(self.count))
    }
}

impl<'a, T> Iterator for Split<'a, T> {
    type Item = &'a [T];

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        if self.count == 0 {
            None
        } else {
            let len = self.slice.len();
            let next;
            (next, self.slice) = core::mem::take(&mut self.slice).split_at(Ord::min(self.div, len));
            self.count -= 1;
            Some(next)
        }
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.count, Some(self.count))
    }
}

impl<'a, T> DoubleEndedIterator for SplitMut<'a, T> {
    #[inline]
    fn next_back(&mut self) -> Option<Self::Item> {
        if self.count == 0 {
            None
        } else {
            let len = self.slice.len();
            let next;
            (self.slice, next) =
                core::mem::take(&mut self.slice).split_at_mut(len - Ord::min(self.div, len));
            self.count -= 1;
            Some(next)
        }
    }
}

impl<'a, T> DoubleEndedIterator for Split<'a, T> {
    #[inline]
    fn next_back(&mut self) -> Option<Self::Item> {
        if self.count == 0 {
            None
        } else {
            let len = self.slice.len();
            let next;
            (self.slice, next) =
                core::mem::take(&mut self.slice).split_at(len - Ord::min(self.div, len));
            self.count -= 1;
            Some(next)
        }
    }
}

impl<'a, T> ExactSizeIterator for SplitMut<'a, T> {
    #[inline]
    fn len(&self) -> usize {
        self.count
    }
}

impl<'a, T> ExactSizeIterator for Split<'a, T> {
    #[inline]
    fn len(&self) -> usize {
        self.count
    }
}
