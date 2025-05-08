//! Common functionality for atomic queues
//!
//! This module provides the shared traits and types used by various
//! atomic queue implementations.

use std::sync::atomic::AtomicUsize;
use crossbeam_utils::CachePadded;

use crate::arch::spin_loop_pause;

/// Memory ordering constants for atomic operations
/// 
/// These are aliases to the standard library's Ordering values
/// to match the original C++ implementation's naming convention.
pub mod ordering {
    pub use std::sync::atomic::Ordering::Acquire as A;
    pub use std::sync::atomic::Ordering::Release as R;
    pub use std::sync::atomic::Ordering::Relaxed as X;
    pub use std::sync::atomic::Ordering::AcqRel as AR;
}

/// Likely predicate helper - hints to the compiler that the condition is likely to be true
/// 
/// This is a simple implementation that just returns the boolean value.
/// In the future, when Rust stabilizes branch prediction intrinsics,
/// this could be replaced with a more efficient implementation.
#[inline(always)]
pub fn likely(b: bool) -> bool {
    b
}

/// Unlikely predicate helper - hints to the compiler that the condition is likely to be false
/// 
/// This is a simple implementation that just returns the boolean value.
/// In the future, when Rust stabilizes branch prediction intrinsics,
/// this could be replaced with a more efficient implementation.
#[inline(always)]
pub fn unlikely(b: bool) -> bool {
    b
}

/// Common base functionality for atomic queues
///
/// This struct provides the shared state and operations that are common
/// across all queue variants.
pub struct AtomicQueueCommon {
    /// The head index, where producers push elements
    /// 
    /// Placed on its own cache line to avoid false sharing with tail
    pub head: CachePadded<AtomicUsize>,
    
    /// The tail index, where consumers pop elements
    /// 
    /// Placed on its own cache line to avoid false sharing with head
    pub tail: CachePadded<AtomicUsize>,
}

impl AtomicQueueCommon {
    /// Creates a new empty atomic queue common structure
    #[inline]
    pub fn new() -> Self {
        Self {
            head: CachePadded::new(AtomicUsize::new(0)),
            tail: CachePadded::new(AtomicUsize::new(0)),
        }
    }
    
    /// Returns the current size of the queue (number of elements)
    ///
    /// Note that this is not thread-safe and may produce inaccurate results
    /// when called concurrently with push/pop operations.
    #[inline]
    pub fn was_size(&self) -> usize {
        self.head.load(ordering::X) - self.tail.load(ordering::X)
    }
    
    /// Checks if the queue was empty during this call
    ///
    /// Note that this is not thread-safe and may produce inaccurate results
    /// when called concurrently with push/pop operations.
    #[inline]
    pub fn was_empty(&self) -> bool {
        self.head.load(ordering::X) == self.tail.load(ordering::X)
    }
    
    /// Checks if the queue was full during this call
    ///
    /// Note that this is not thread-safe and may produce inaccurate results
    /// when called concurrently with push/pop operations.
    #[inline]
    pub fn was_full(&self, capacity: usize) -> bool {
        self.was_size() == capacity
    }
}

impl Default for AtomicQueueCommon {
    fn default() -> Self {
        Self::new()
    }
}

/// Trait for atomic queue operations
///
/// This trait defines the common interface that all atomic queue implementations must provide.
pub trait AtomicQueueOps<T> {
    /// Attempts to push an element to the queue
    ///
    /// Returns true if the element was successfully pushed, false if the queue was full.
    fn try_push(&self, element: T) -> bool;
    
    /// Attempts to pop an element from the queue
    ///
    /// Returns Some(element) if an element was successfully popped, None if the queue was empty.
    fn try_pop(&self) -> Option<T>;
    
    /// Pushes an element to the queue, waiting if necessary
    ///
    /// This method will busy-wait if the queue is full.
    fn push(&self, element: T);
    
    /// Pops an element from the queue, waiting if necessary
    ///
    /// This method will busy-wait if the queue is empty.
    fn pop(&self) -> T;
    
    /// Returns the capacity of the queue
    fn capacity(&self) -> usize;
    
    /// Returns the current size of the queue (number of elements)
    fn was_size(&self) -> usize;
    
    /// Checks if the queue was empty during this call
    fn was_empty(&self) -> bool;
    
    /// Checks if the queue was full during this call
    fn was_full(&self) -> bool;
}

/// Helper trait for queues that support busy-waiting operations
pub trait OptimistAtomicQueueOps<T: Clone>: AtomicQueueOps<T> {
    /// Pushes an element to the queue, busy-waiting if the queue is full
    fn optimist_push(&self, element: T) {
        let element = element;
        while !self.try_push(element.clone()) {
            spin_loop_pause();
        }
    }
    
    /// Pops an element from the queue, busy-waiting if the queue is empty
    fn optimist_pop(&self) -> T {
        loop {
            if let Some(element) = self.try_pop() {
                return element;
            }
            spin_loop_pause();
        }
    }
}
