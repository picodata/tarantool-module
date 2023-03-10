use crate::error::TarantoolError;
use crate::ffi::tarantool as ffi;
use std::alloc::Layout;
use std::ptr::NonNull;
use std::mem::MaybeUninit;

pub struct Allocator {
    svp: usize,
}

impl Allocator {
    /// Create a new allocator, which will allocate memory on the current
    /// fiber's gc region.
    ///
    /// Can be used for temporary allocation.
    ///
    /// Must exist in one instance at a time! (How do we enforce it?)
    /// Must not be moved between fibers! (How do we enforce it?)
    pub fn new() -> Self {
        Self { svp: used() }
    }

    pub fn used_at_creation(&self) -> usize {
        self.svp
    }

    pub fn allocated(&self) -> usize {
        used().saturating_sub(self.svp)
    }
}

impl Drop for Allocator {
    fn drop(&mut self) {
        unsafe {
            ffi::box_region_truncate(self.svp)
        }
    }
}

impl Allocator {
    /// Allocates an aligned memory region using the current fiber's gc region
    /// and returns it as a mutable reference to a `MaybeUninit<T>` to highlight
    /// the fact that the memory is uninitialized.
    ///
    /// Returns a tarantool error in case of allocation failure.
    ///
    /// All allocations are freed when `Allocator` is dropped, for this reason
    /// they borrow the allocator, so that borrow checker saves us from use
    /// after free.
    ///
    /// This code doesn't compile:
    /// ```compile_fail
    /// use tarantool::region::Allocator;
    /// let data = {
    ///     let region = Allocator::new();
    ///     region.alloc::<(i32, i8)>()
    /// };
    /// ```
    pub fn alloc<T>(&self) -> Result<&mut MaybeUninit<T>, TarantoolError> {
        let layout = Layout::new::<T>();
        unsafe {
            let ptr = ffi::box_region_aligned_alloc(layout.size(), layout.align());
            if ptr.is_null() {
                return Err(TarantoolError::last());
            }
            Ok(std::mem::transmute(ptr))
        }
    }

    pub fn alloc_unaligned(&self, size: usize) -> Result<NonNull<[u8]>, TarantoolError> {
        unsafe {
            let ptr = ffi::box_region_alloc(size);
            if ptr.is_null() {
                return Err(TarantoolError::last());
            }
            let ptr = ptr.cast::<u8>();
            let slice_ptr = std::ptr::slice_from_raw_parts_mut(ptr, size);
            Ok(NonNull::new_unchecked(slice_ptr))
        }
    }
}

#[inline(always)]
pub fn used() -> usize {
    // Safety: this is only unsafe if fiber runtime is not initialized, in which
    // case pretty much everything is unsafe, so who cares
    unsafe {
        ffi::box_region_used()
    }
}

#[cfg(feature = "internal_test")]
mod tests {
    use super::*;

    #[crate::test(tarantool = "crate")]
    fn region() {
        let svp = used();
        {
            let region = Allocator::new();

            let some_bytes = region.alloc::<[u8; 3]>().unwrap();
            let some_bytes = some_bytes.write([1, 2, 3]);
            assert_eq!(some_bytes, &[1, 2, 3]);

            /// Field alignment matters
            #[repr(C)]
            #[derive(PartialEq, Debug)]
            struct S {
                x: i8,
                y: i32,
                z: isize,
            }
            let s = region.alloc::<S>().unwrap();
            let s = s.write(S { x: 1, y: 2, z: 3 });
            assert_eq!(s, &S { x: 1, y: 2, z: 3});

            assert_eq!(region.used_at_creation(), svp);
            assert_ne!(region.allocated(), 0);
            assert_ne!(svp, used());
        }
        assert_eq!(svp, used());
    }
}

