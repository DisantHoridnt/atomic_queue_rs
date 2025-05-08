//! Utility functions and types for atomic queues
//!
//! This module provides various helper utilities used by the queue implementations.

use std::fmt;
use std::marker::PhantomData;
use std::mem::MaybeUninit;
use std::ptr;

/// Trait for types that can be initialized with a specified value
pub trait Initialize {
    /// Initialize this type with the given value
    fn initialize(&mut self, value: Self);
}

impl<T: Copy> Initialize for T {
    #[inline]
    fn initialize(&mut self, value: Self) {
        *self = value;
    }
}

/// Fill an array with a value
///
/// This function is similar to std::uninitialized_fill_n in C++.
#[inline]
pub unsafe fn uninitialized_fill_n<T: Initialize + Copy>(ptr: *mut T, count: usize, value: T) {
    for i in 0..count {
        (*ptr.add(i)).initialize(value);
    }
}

/// Destroy a range of objects
///
/// This function is similar to std::destroy_n in C++.
#[inline]
pub unsafe fn destroy_n<T>(ptr: *mut T, count: usize) {
    for i in 0..count {
        ptr::drop_in_place(ptr.add(i));
    }
}

/// Trait for index calculation strategies
pub trait IndexStrategy {
    /// Calculate array index from logical position
    fn get_index(position: usize, capacity: usize) -> usize;
}

/// Simple modulo-based index calculation
pub struct SimpleIndex;

impl IndexStrategy for SimpleIndex {
    #[inline(always)]
    fn get_index(position: usize, capacity: usize) -> usize {
        position % capacity
    }
}

/// Power-of-2 optimized index calculation
pub struct PowerOf2Index;

impl IndexStrategy for PowerOf2Index {
    #[inline(always)]
    fn get_index(position: usize, capacity: usize) -> usize {
        position & (capacity - 1)
    }
}

/// Helper type for array construction with specific alignment
#[repr(C)]
pub struct AlignedArray<T, const N: usize> {
    pub data: [MaybeUninit<T>; N],
    _marker: PhantomData<T>,
}

impl<T, const N: usize> AlignedArray<T, N> {
    /// Create a new uninitialized array
    #[inline]
    pub const fn new_uninit() -> Self {
        Self {
            data: unsafe { MaybeUninit::uninit().assume_init() },
            _marker: PhantomData,
        }
    }
    
    /// Get a pointer to the array data
    #[inline]
    pub fn as_ptr(&self) -> *const MaybeUninit<T> {
        self.data.as_ptr()
    }
    
    /// Get a mutable pointer to the array data
    #[inline]
    pub fn as_mut_ptr(&mut self) -> *mut MaybeUninit<T> {
        self.data.as_mut_ptr()
    }
}

impl<T: fmt::Debug, const N: usize> fmt::Debug for AlignedArray<T, N> 
where
    MaybeUninit<T>: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AlignedArray")
            .field("data", &self.data)
            .finish()
    }
}
