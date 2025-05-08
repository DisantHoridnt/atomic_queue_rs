//! Implementation of atomic queues for non-atomic elements
//!
//! This module provides implementations of lock-free queues for elements that
//! cannot be atomically operated on (i.e., arbitrary types).

use std::marker::PhantomData;
use std::mem::MaybeUninit;
use std::alloc::{self, Layout};
use std::ops::Deref;
use std::ptr;
use std::sync::atomic::{AtomicU8, Ordering};

use crossbeam_utils::CachePadded;

use crate::arch::{CACHE_LINE_SIZE, spin_loop_pause, round_up_to_power_of_2, get_index_shuffle_bits, remap_index};
use crate::common::{AtomicQueueCommon, AtomicQueueOps, ordering};

/// A lock-free fixed-size queue for non-atomic elements with compile-time capacity
///
/// This queue is designed for multiple producers and multiple consumers,
/// and uses atomic operations to ensure thread safety. Unlike AtomicQueue,
/// this queue can store arbitrary types, not just atomically operable ones.
#[repr(C)]
pub struct AtomicQueue2<T, const SIZE: usize, 
                        const MINIMIZE_CONTENTION: bool = true,
                        const MAXIMIZE_THROUGHPUT: bool = true,
                        const TOTAL_ORDER: bool = false,
                        const SPSC: bool = false> {
    /// The common queue state (head and tail indices)
    common: AtomicQueueCommon,
    
    /// The storage for element states
    ///
    /// Each state is a separate atomic value, indicating whether the slot is empty or full.
    states: Box<[CachePadded<AtomicU8>]>,
    
    /// The storage for queue elements
    elements: Box<[MaybeUninit<T>]>,
    
    /// The actual queue capacity (potentially rounded up to power of 2)
    capacity: usize,
    
    /// Number of bits to use for index remapping (to reduce contention)
    shuffle_bits: usize,
    
    /// Marker for variance
    _marker: PhantomData<T>,
}

/// Element state constants
const EMPTY: u8 = 0;
const FULL: u8 = 1;

impl<T, const SIZE: usize, 
     const MINIMIZE_CONTENTION: bool,
     const MAXIMIZE_THROUGHPUT: bool,
     const TOTAL_ORDER: bool,
     const SPSC: bool> AtomicQueue2<T, SIZE, MINIMIZE_CONTENTION, MAXIMIZE_THROUGHPUT, TOTAL_ORDER, SPSC> {
    
    /// Creates a new empty queue with the specified compile-time capacity
    pub fn new() -> Self {
        // Round up capacity to power of 2 if minimizing contention
        let capacity = if MINIMIZE_CONTENTION {
            round_up_to_power_of_2(SIZE)
        } else {
            SIZE
        };
        
        // Calculate shuffle bits for contention reduction
        let elements_per_cache_line = CACHE_LINE_SIZE / std::mem::size_of::<CachePadded<AtomicU8>>();
        let shuffle_bits = get_index_shuffle_bits(MINIMIZE_CONTENTION, capacity, elements_per_cache_line);
        
        // Create atomic states, all initialized to EMPTY
        let mut states = Vec::with_capacity(capacity);
        for _ in 0..capacity {
            states.push(CachePadded::new(AtomicU8::new(EMPTY)));
        }
        
        // Create storage for elements
        let mut elements = Vec::with_capacity(capacity);
        for _ in 0..capacity {
            elements.push(MaybeUninit::uninit());
        }
        
        Self {
            common: AtomicQueueCommon::new(),
            states: states.into_boxed_slice(),
            elements: elements.into_boxed_slice(),
            capacity,
            shuffle_bits,
            _marker: PhantomData,
        }
    }
    
    /// Get the state and element for an index, with proper remapping to reduce contention
    #[inline(always)]
    fn get_slot(&self, index: usize) -> (&AtomicU8, *mut MaybeUninit<T>) {
        let remapped_index = remap_index(index % self.capacity, self.shuffle_bits);
        let state = &self.states[remapped_index];
        let element = &self.elements[remapped_index] as *const MaybeUninit<T> as *mut MaybeUninit<T>;
        (state, element)
    }
    
    /// Implementation of element pop operation
    #[inline]
    fn do_pop_any(&self, state: &AtomicU8, element: *mut MaybeUninit<T>) -> T {
        if SPSC {
            // Simplified implementation for single-producer-single-consumer case
            loop {
                if state.load(ordering::A) == FULL {
                    // Retrieve the element
                    let value = unsafe { ptr::read(element as *const T) };
                    // Mark the slot as empty
                    state.store(EMPTY, ordering::X);
                    return value;
                }
                if MAXIMIZE_THROUGHPUT {
                    spin_loop_pause();
                }
            }
        } else {
            // Full MPMC implementation
            loop {
                if state.compare_exchange(FULL, EMPTY, ordering::A, ordering::X).is_ok() {
                    // Retrieve the element
                    return unsafe { ptr::read(element as *const T) };
                }
                
                // Speculative loads while waiting
                if MAXIMIZE_THROUGHPUT {
                    while state.load(ordering::X) == EMPTY {
                        spin_loop_pause();
                    }
                }
            }
        }
    }
    
    /// Implementation of element push operation
    #[inline]
    fn do_push_any<U: Into<T>>(&self, value: U, state: &AtomicU8, element: *mut MaybeUninit<T>) {
        if SPSC {
            // Simplified implementation for single-producer-single-consumer case
            while state.load(ordering::X) != EMPTY {
                if MAXIMIZE_THROUGHPUT {
                    spin_loop_pause();
                }
            }
            
            // Store the element
            unsafe { ptr::write(element as *mut T, value.into()) };
            
            // Mark the slot as full using release semantics
            state.store(FULL, ordering::R);
        } else {
            // Full MPMC implementation
            loop {
                if state.compare_exchange(EMPTY, FULL, ordering::A, ordering::R).is_ok() {
                    // Store the element
                    unsafe { ptr::write(element as *mut T, value.into()) };
                    return;
                }
                
                if MAXIMIZE_THROUGHPUT {
                    spin_loop_pause();
                }
            }
        }
    }
}

impl<T, const SIZE: usize, 
     const MINIMIZE_CONTENTION: bool,
     const MAXIMIZE_THROUGHPUT: bool,
     const TOTAL_ORDER: bool,
     const SPSC: bool> Drop 
     for AtomicQueue2<T, SIZE, MINIMIZE_CONTENTION, MAXIMIZE_THROUGHPUT, TOTAL_ORDER, SPSC> {
    
    fn drop(&mut self) {
        // Drop any remaining elements in the queue
        for i in 0..self.capacity {
            let remapped_index = remap_index(i, self.shuffle_bits);
            let state = &self.states[remapped_index];
            
            if state.load(Ordering::Relaxed) == FULL {
                unsafe {
                    ptr::drop_in_place(self.elements[remapped_index].as_mut_ptr());
                }
            }
        }
    }
}

impl<T, const SIZE: usize, 
     const MINIMIZE_CONTENTION: bool,
     const MAXIMIZE_THROUGHPUT: bool,
     const TOTAL_ORDER: bool,
     const SPSC: bool> AtomicQueueOps<T> 
     for AtomicQueue2<T, SIZE, MINIMIZE_CONTENTION, MAXIMIZE_THROUGHPUT, TOTAL_ORDER, SPSC> {
    
    fn try_push(&self, value: T) -> bool {
        // Get current head and calculate new head
        let head = self.common.head.load(ordering::X);
        let size = head - self.common.tail.load(ordering::X);
        
        if size >= self.capacity {
            return false; // Queue is full
        }
        
        // Try to atomically update head
        if TOTAL_ORDER {
            if self.common.head.compare_exchange(head, head + 1, ordering::AR, ordering::X).is_err() {
                return false;
            }
        } else {
            self.common.head.fetch_add(1, ordering::X);
        }
        
        // Get the slot at the head position
        let (state, element) = self.get_slot(head);
        
        // Store the value into the element
        self.do_push_any(value, state, element);
        
        true
    }
    
    fn try_pop(&self) -> Option<T> {
        // Get current tail
        let tail = self.common.tail.load(ordering::X);
        
        // Check if queue is empty
        if tail == self.common.head.load(ordering::X) {
            return None; // Queue is empty
        }
        
        // Try to atomically update tail
        if TOTAL_ORDER {
            if self.common.tail.compare_exchange(tail, tail + 1, ordering::AR, ordering::X).is_err() {
                return None;
            }
        } else {
            self.common.tail.fetch_add(1, ordering::X);
        }
        
        // Get the slot at the tail position
        let (state, element) = self.get_slot(tail);
        
        // Get the value using acquire semantics
        // The acquire operation synchronizes with the release operation in push
        let value = self.do_pop_any(state, element);
        
        Some(value)
    }
    
    fn push(&self, value: T) {
        // Get current head and calculate new head
        let head = if TOTAL_ORDER {
            self.common.head.fetch_add(1, ordering::AR)
        } else {
            self.common.head.fetch_add(1, ordering::X)
        };
        
        // Get the slot at the head position
        let (state, element) = self.get_slot(head);
        
        // Push the value into the element
        self.do_push_any(value, state, element);
    }
    
    fn pop(&self) -> T {
        // Get current tail and calculate new tail
        let tail = if TOTAL_ORDER {
            self.common.tail.fetch_add(1, ordering::AR)
        } else {
            self.common.tail.fetch_add(1, ordering::X)
        };
        
        // Get the slot at the tail position
        let (state, element) = self.get_slot(tail);
        
        // Pop the value from the element
        self.do_pop_any(state, element)
    }
    
    fn capacity(&self) -> usize {
        self.capacity
    }
    
    fn was_size(&self) -> usize {
        self.common.was_size()
    }
    
    fn was_empty(&self) -> bool {
        self.common.was_empty()
    }
    
    fn was_full(&self) -> bool {
        self.common.was_full(self.capacity)
    }
}

impl<T, const SIZE: usize, 
     const MINIMIZE_CONTENTION: bool,
     const MAXIMIZE_THROUGHPUT: bool,
     const TOTAL_ORDER: bool,
     const SPSC: bool> Default 
     for AtomicQueue2<T, SIZE, MINIMIZE_CONTENTION, MAXIMIZE_THROUGHPUT, TOTAL_ORDER, SPSC> {
    
    fn default() -> Self {
        Self::new()
    }
}

/// A lock-free fixed-size queue for non-atomic elements with runtime capacity
///
/// Similar to AtomicQueue2, but with capacity specified at runtime.
pub struct AtomicQueueB2<T, 
                         const MAXIMIZE_THROUGHPUT: bool = true,
                         const TOTAL_ORDER: bool = false,
                         const SPSC: bool = false> {
    /// The common queue state (head and tail indices)
    common: AtomicQueueCommon,
    
    /// The storage for element states
    states: *mut CachePadded<AtomicU8>,
    
    /// The storage for queue elements
    elements: *mut MaybeUninit<T>,
    
    /// The actual queue capacity (rounded up to power of 2)
    capacity: usize,
    
    /// Number of bits to use for index remapping (to reduce contention)
    shuffle_bits: usize,
    
    /// Marker for variance and drop check
    _marker: PhantomData<(Box<[CachePadded<AtomicU8>]>, Box<[T]>)>,
}

// Safety: AtomicQueueB2<T> is internally synchronized, so it's safe to share references
// between threads.
unsafe impl<T: Send, const MAXIMIZE_THROUGHPUT: bool, const TOTAL_ORDER: bool, const SPSC: bool>
    Send for AtomicQueueB2<T, MAXIMIZE_THROUGHPUT, TOTAL_ORDER, SPSC> {}
unsafe impl<T: Send, const MAXIMIZE_THROUGHPUT: bool, const TOTAL_ORDER: bool, const SPSC: bool>
    Sync for AtomicQueueB2<T, MAXIMIZE_THROUGHPUT, TOTAL_ORDER, SPSC> {}

impl<T, const MAXIMIZE_THROUGHPUT: bool, const TOTAL_ORDER: bool, const SPSC: bool>
    AtomicQueueB2<T, MAXIMIZE_THROUGHPUT, TOTAL_ORDER, SPSC> {
    
    /// Creates a new empty queue with the specified runtime capacity
    pub fn new(size: usize) -> Self {
        // Calculate the actual capacity (rounded up to power of 2)
        let capacity = round_up_to_power_of_2(size);
        
        // Calculate shuffle bits for contention reduction
        let elements_per_cache_line = CACHE_LINE_SIZE / std::mem::size_of::<CachePadded<AtomicU8>>();
        let shuffle_bits = get_index_shuffle_bits(true, capacity, elements_per_cache_line);
        
        // Allocate memory for states
        let states_layout = Layout::array::<CachePadded<AtomicU8>>(capacity).unwrap();
        let states = unsafe { 
            let ptr = alloc::alloc(states_layout) as *mut CachePadded<AtomicU8>;
            if ptr.is_null() {
                alloc::handle_alloc_error(states_layout);
            }
            
            // Initialize all states to EMPTY
            for i in 0..capacity {
                ptr::write(ptr.add(i), CachePadded::new(AtomicU8::new(EMPTY)));
            }
            
            ptr
        };
        
        // Allocate memory for elements
        let elements_layout = Layout::array::<MaybeUninit<T>>(capacity).unwrap();
        let elements = unsafe { 
            let ptr = alloc::alloc(elements_layout) as *mut MaybeUninit<T>;
            if ptr.is_null() {
                // Clean up states allocation before error
                for i in 0..capacity {
                    ptr::drop_in_place(states.add(i));
                }
                alloc::dealloc(states as *mut u8, states_layout);
                alloc::handle_alloc_error(elements_layout);
            }
            
            // Initialize all elements as uninitialized
            for i in 0..capacity {
                ptr::write(ptr.add(i), MaybeUninit::uninit());
            }
            
            ptr
        };
        
        Self {
            common: AtomicQueueCommon::new(),
            states,
            elements,
            capacity,
            shuffle_bits,
            _marker: PhantomData,
        }
    }
    
    /// Get the state and element for an index, with proper remapping to reduce contention
    #[inline(always)]
    fn get_slot(&self, index: usize) -> (&AtomicU8, *mut MaybeUninit<T>) {
        let remapped_index = remap_index(index & (self.capacity - 1), self.shuffle_bits);
        unsafe {
            let state = &*(*self.states.add(remapped_index)).deref();
            let element = self.elements.add(remapped_index);
            (state, element)
        }
    }
    
    /// Implementation of element pop operation
    #[inline]
    fn do_pop_any(&self, state: &AtomicU8, element: *mut MaybeUninit<T>) -> T {
        if SPSC {
            // Simplified implementation for single-producer-single-consumer case
            loop {
                if state.load(ordering::A) == FULL {
                    // Retrieve the element
                    let value = unsafe { ptr::read(element as *const T) };
                    // Mark the slot as empty
                    state.store(EMPTY, ordering::X);
                    return value;
                }
                if MAXIMIZE_THROUGHPUT {
                    spin_loop_pause();
                }
            }
        } else {
            // Full MPMC implementation
            loop {
                if state.compare_exchange(FULL, EMPTY, ordering::A, ordering::X).is_ok() {
                    // Retrieve the element
                    return unsafe { ptr::read(element as *const T) };
                }
                
                // Speculative loads while waiting
                if MAXIMIZE_THROUGHPUT {
                    while state.load(ordering::X) == EMPTY {
                        spin_loop_pause();
                    }
                }
            }
        }
    }
    
    /// Implementation of element push operation
    #[inline]
    fn do_push_any<U: Into<T>>(&self, value: U, state: &AtomicU8, element: *mut MaybeUninit<T>) {
        if SPSC {
            // Simplified implementation for single-producer-single-consumer case
            while state.load(ordering::X) != EMPTY {
                if MAXIMIZE_THROUGHPUT {
                    spin_loop_pause();
                }
            }
            
            // Store the element
            unsafe { ptr::write(element as *mut T, value.into()) };
            
            // Mark the slot as full using release semantics
            state.store(FULL, ordering::R);
        } else {
            // Full MPMC implementation
            loop {
                if state.compare_exchange(EMPTY, FULL, ordering::A, ordering::R).is_ok() {
                    // Store the element
                    unsafe { ptr::write(element as *mut T, value.into()) };
                    return;
                }
                
                if MAXIMIZE_THROUGHPUT {
                    spin_loop_pause();
                }
            }
        }
    }
}

impl<T, const MAXIMIZE_THROUGHPUT: bool, const TOTAL_ORDER: bool, const SPSC: bool>
    Drop for AtomicQueueB2<T, MAXIMIZE_THROUGHPUT, TOTAL_ORDER, SPSC> {
    
    fn drop(&mut self) {
        // We need to be careful about dropping the elements and deallocating memory
        unsafe {
            // Drop any remaining elements in the queue
            for i in 0..self.capacity {
                let remapped_index = remap_index(i & (self.capacity - 1), self.shuffle_bits);
                let state = &*(*self.states.add(remapped_index)).deref();
                
                if state.load(Ordering::Relaxed) == FULL {
                    // Drop the element if it's full
                    ptr::drop_in_place((self.elements.add(remapped_index)) as *mut T);
                }
                
                // Drop the atomic state itself
                ptr::drop_in_place(self.states.add(remapped_index));
            }
            
            // Deallocate memory
            let states_layout = Layout::array::<CachePadded<AtomicU8>>(self.capacity).unwrap();
            let elements_layout = Layout::array::<MaybeUninit<T>>(self.capacity).unwrap();
            
            alloc::dealloc(self.states as *mut u8, states_layout);
            alloc::dealloc(self.elements as *mut u8, elements_layout);
        }
    }
}

impl<T, const MAXIMIZE_THROUGHPUT: bool, const TOTAL_ORDER: bool, const SPSC: bool>
    AtomicQueueOps<T> for AtomicQueueB2<T, MAXIMIZE_THROUGHPUT, TOTAL_ORDER, SPSC> {
    
    fn try_push(&self, value: T) -> bool {
        // Get current head and calculate new head
        let head = self.common.head.load(ordering::X);
        let size = head - self.common.tail.load(ordering::X);
        
        if size >= self.capacity {
            return false; // Queue is full
        }
        
        // Try to atomically update head
        if TOTAL_ORDER {
            if self.common.head.compare_exchange(head, head + 1, ordering::AR, ordering::X).is_err() {
                return false;
            }
        } else {
            self.common.head.fetch_add(1, ordering::X);
        }
        
        // Get the slot at the head position
        let (state, element) = self.get_slot(head);
        
        // Store the value into the element
        self.do_push_any(value, state, element);
        
        true
    }
    
    fn try_pop(&self) -> Option<T> {
        // Get current tail
        let tail = self.common.tail.load(ordering::X);
        
        // Check if queue is empty
        if tail == self.common.head.load(ordering::X) {
            return None; // Queue is empty
        }
        
        // Try to atomically update tail
        if TOTAL_ORDER {
            if self.common.tail.compare_exchange(tail, tail + 1, ordering::AR, ordering::X).is_err() {
                return None;
            }
        } else {
            self.common.tail.fetch_add(1, ordering::X);
        }
        
        // Get the slot at the tail position
        let (state, element) = self.get_slot(tail);
        
        // Get the value using acquire semantics
        // The acquire operation synchronizes with the release operation in push
        let value = self.do_pop_any(state, element);
        
        Some(value)
    }
    
    fn push(&self, value: T) {
        // Get current head and calculate new head
        let head = if TOTAL_ORDER {
            self.common.head.fetch_add(1, ordering::AR)
        } else {
            self.common.head.fetch_add(1, ordering::X)
        };
        
        // Get the slot at the head position
        let (state, element) = self.get_slot(head);
        
        // Push the value into the element
        self.do_push_any(value, state, element);
    }
    
    fn pop(&self) -> T {
        // Get current tail and calculate new tail
        let tail = if TOTAL_ORDER {
            self.common.tail.fetch_add(1, ordering::AR)
        } else {
            self.common.tail.fetch_add(1, ordering::X)
        };
        
        // Get the slot at the tail position
        let (state, element) = self.get_slot(tail);
        
        // Pop the value from the element
        self.do_pop_any(state, element)
    }
    
    fn capacity(&self) -> usize {
        self.capacity
    }
    
    fn was_size(&self) -> usize {
        self.common.was_size()
    }
    
    fn was_empty(&self) -> bool {
        self.common.was_empty()
    }
    
    fn was_full(&self) -> bool {
        self.common.was_full(self.capacity)
    }
}

/// A faster version of AtomicQueue2 that busy-waits when empty or full
///
/// This is a type alias for AtomicQueue2 with different default template parameters.
pub type OptimistAtomicQueue2<T, const SIZE: usize, 
                             const MINIMIZE_CONTENTION: bool = true,
                             const TOTAL_ORDER: bool = false,
                             const SPSC: bool = false> 
    = AtomicQueue2<T, SIZE, MINIMIZE_CONTENTION, true, TOTAL_ORDER, SPSC>;

/// A faster version of AtomicQueueB2 that busy-waits when empty or full
///
/// This is a type alias for AtomicQueueB2 with different default template parameters.
pub type OptimistAtomicQueueB2<T, 
                              const TOTAL_ORDER: bool = false,
                              const SPSC: bool = false>
    = AtomicQueueB2<T, true, TOTAL_ORDER, SPSC>;

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::thread;
    
    #[test]
    fn test_atomic_queue2_basic() {
        let q = AtomicQueue2::<String, 16>::new();
        
        // Test push and pop
        assert!(q.try_push("hello".to_string()));
        assert!(q.try_push("world".to_string()));
        assert!(q.try_push("rust".to_string()));
        
        assert_eq!(q.try_pop().unwrap(), "hello");
        assert_eq!(q.try_pop().unwrap(), "world");
        assert_eq!(q.try_pop().unwrap(), "rust");
        assert_eq!(q.try_pop(), None);
    }
    
    #[test]
    fn test_atomic_queue2_full() {
        let q = AtomicQueue2::<String, 3>::new();
        
        // Fill the queue
        assert!(q.try_push("one".to_string()));
        assert!(q.try_push("two".to_string()));
        assert!(q.try_push("three".to_string()));
        
        // Queue should be full
        assert!(!q.try_push("four".to_string()));
        
        // Make space and try again
        assert_eq!(q.try_pop().unwrap(), "one");
        assert!(q.try_push("four".to_string()));
        
        // Check remaining values
        assert_eq!(q.try_pop().unwrap(), "two");
        assert_eq!(q.try_pop().unwrap(), "three");
        assert_eq!(q.try_pop().unwrap(), "four");
        assert_eq!(q.try_pop(), None);
    }
    
    #[test]
    fn test_atomic_queueb2_basic() {
        let q = AtomicQueueB2::<String>::new(16);
        
        // Test push and pop
        assert!(q.try_push("hello".to_string()));
        assert!(q.try_push("world".to_string()));
        assert!(q.try_push("rust".to_string()));
        
        assert_eq!(q.try_pop().unwrap(), "hello");
        assert_eq!(q.try_pop().unwrap(), "world");
        assert_eq!(q.try_pop().unwrap(), "rust");
        assert_eq!(q.try_pop(), None);
    }
    
    struct ComplexType {
        id: u32,
        data: Vec<u8>,
    }
    
    #[test]
    fn test_complex_type() {
        let q = AtomicQueue2::<ComplexType, 16>::new();
        
        // Test push and pop with a more complex type
        assert!(q.try_push(ComplexType { id: 1, data: vec![1, 2, 3] }));
        assert!(q.try_push(ComplexType { id: 2, data: vec![4, 5, 6] }));
        
        let item1 = q.try_pop().unwrap();
        assert_eq!(item1.id, 1);
        assert_eq!(item1.data, vec![1, 2, 3]);
        
        let item2 = q.try_pop().unwrap();
        assert_eq!(item2.id, 2);
        assert_eq!(item2.data, vec![4, 5, 6]);
    }
}
