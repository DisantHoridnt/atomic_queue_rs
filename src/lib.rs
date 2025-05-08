//! # atomic_queue_rs
//!
//! A Rust implementation of lock-free multiple-producer-multiple-consumer queues
//! based on circular buffers and atomic operations.
//!
//! Ported from the C++ [atomic_queue](https://github.com/max0x7ba/atomic_queue) library.

mod arch;
mod common;
mod utils;

pub mod atomic_queue;
pub mod atomic_queue2;

// Re-exports for convenience
pub use atomic_queue::{AtomicQueue, AtomicQueueB, OptimistAtomicQueue, OptimistAtomicQueueB};
pub use atomic_queue2::{AtomicQueue2, AtomicQueueB2, OptimistAtomicQueue2, OptimistAtomicQueueB2};

/// Trait for elements that can be stored in an atomic queue
pub trait Element: Copy + Default + PartialEq + Send + Sync + 'static {}

// Implement Element trait for common primitive types
impl Element for u8 {}
impl Element for u16 {}
impl Element for u32 {}
impl Element for u64 {}
impl Element for usize {}
impl Element for i8 {}
impl Element for i16 {}
impl Element for i32 {}
impl Element for i64 {}
impl Element for isize {}

// Note: We don't implement for raw pointers as they don't implement Default

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn element_trait_sanity() {
        // Just verify that our Element trait is implemented for expected types
        fn is_element<T: Element>() -> bool { true }
        
        assert!(is_element::<u32>());
        assert!(is_element::<i64>());
        assert!(is_element::<*const u8>());
    }
}
