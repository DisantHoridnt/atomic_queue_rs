//! Implementation of atomic queues for atomic elements
//!
//! This module provides implementations of lock-free queues for elements that
//! can be atomically operated on (i.e., primitive types).

use std::marker::PhantomData;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::alloc::{self, Layout};
use std::ops::Deref;
use std::ptr;

use crossbeam_utils::CachePadded;

use crate::arch::{spin_loop_pause, round_up_to_power_of_2};
use crate::common::{AtomicQueueCommon, AtomicQueueOps, ordering};
use crate::Element;

/// Wrapper for accessing elements atomically
#[repr(transparent)]
struct AtomicValue<T: Element> {
    /// The actual storage for the atomic value
    value: AtomicUsize,
    /// Phantom data to track the type parameter
    _marker: PhantomData<T>,
}

impl<T: Element> AtomicValue<T> {
    /// Creates a new AtomicValue initialized to the default value
    #[inline]
    fn new() -> Self {
        Self {
            value: AtomicUsize::new(0),
            _marker: PhantomData,
        }
    }
    
    /// Loads the atomic value
    #[inline]
    fn load(&self, order: Ordering) -> T {
        let raw = self.value.load(order);
        unsafe { std::mem::transmute_copy(&raw) }
    }
    
    /// Stores a value into the atomic
    #[inline]
    fn store(&self, value: T, order: Ordering) {
        let raw: usize = unsafe { std::mem::transmute_copy(&value) };
        self.value.store(raw, order);
    }
    
    /// Atomically exchanges a value
    #[inline]
    fn swap(&self, value: T, order: Ordering) -> T {
        let raw: usize = unsafe { std::mem::transmute_copy(&value) };
        let old = self.value.swap(raw, order);
        unsafe { std::mem::transmute_copy(&old) }
    }
    
    /// Atomically compares and exchanges a value
    #[inline]
    fn compare_exchange(&self, current: T, new: T, success: Ordering, failure: Ordering) -> Result<T, T> {
        let current_raw: usize = unsafe { std::mem::transmute_copy(&current) };
        let new_raw: usize = unsafe { std::mem::transmute_copy(&new) };
        
        match self.value.compare_exchange(current_raw, new_raw, success, failure) {
            Ok(val) => Ok(unsafe { std::mem::transmute_copy(&val) }),
            Err(val) => Err(unsafe { std::mem::transmute_copy(&val) }),
        }
    }
}

/// A lock-free fixed-size queue for atomic elements with compile-time capacity
///
/// This queue is designed for multiple producers and multiple consumers,
/// and uses atomic operations to ensure thread safety.
pub struct AtomicQueue<T: Element, const SIZE: usize> {
    /// The common queue state (head and tail indices)
    common: AtomicQueueCommon,
    
    /// The storage for queue elements
    elements: Box<[CachePadded<AtomicValue<T>>]>,
    
    /// The actual capacity (rounded up to power of 2)
    capacity: usize,
    
    /// Marker for variance
    _marker: PhantomData<T>,
}

impl<T: Element, const SIZE: usize> AtomicQueue<T, SIZE> {
    /// Creates a new empty queue
    pub fn new() -> Self {
        let capacity = round_up_to_power_of_2(SIZE);
        
        // Create atomic elements
        let mut elements = Vec::with_capacity(capacity);
        for _ in 0..capacity {
            elements.push(CachePadded::new(AtomicValue::new()));
        }
        
        Self {
            common: AtomicQueueCommon::new(),
            elements: elements.into_boxed_slice(),
            capacity,
            _marker: PhantomData,
        }
    }
    
    /// Get an element at the specified index (with wrapping)
    #[inline(always)]
    fn get_element(&self, index: usize) -> &AtomicValue<T> {
        let idx = index & (self.capacity - 1); // Fast modulo for power of 2
        &self.elements[idx]
    }
}

impl<T: Element, const SIZE: usize> AtomicQueueOps<T> for AtomicQueue<T, SIZE> {
    fn try_push(&self, element: T) -> bool {
        // Check if the queue is full
        let head = self.common.head.load(ordering::X);
        let tail = self.common.tail.load(ordering::X);
        
        if head - tail >= self.capacity {
            return false; // Queue is full
        }
        
        // Try to claim a slot by incrementing the head
        if self.common.head.compare_exchange(head, head + 1, ordering::A, ordering::X).is_err() {
            return false; // Another producer claimed the slot
        }
        
        // Store the element at the claimed position
        let slot = self.get_element(head);
        slot.store(element, ordering::R);
        
        true
    }
    
    fn try_pop(&self) -> Option<T> {
        // Check if the queue is empty
        let tail = self.common.tail.load(ordering::X);
        let head = self.common.head.load(ordering::X);
        
        if tail == head {
            return None; // Queue is empty
        }
        
        // Try to claim a slot by incrementing the tail
        if self.common.tail.compare_exchange(tail, tail + 1, ordering::A, ordering::X).is_err() {
            return None; // Another consumer claimed the slot
        }
        
        // Load the element from the claimed position
        let slot = self.get_element(tail);
        let zero = T::default();
        
        // Try to get the element and replace it with zero
        loop {
            let current = slot.load(ordering::A);
            if current == zero {
                // The producer hasn't stored the element yet, wait
                spin_loop_pause();
                continue;
            }
            
            // Replace the element with zero
            slot.store(zero, ordering::R);
            return Some(current);
        }
    }
    
    fn push(&self, element: T) {
        // Increment head and get the slot
        let head = self.common.head.fetch_add(1, ordering::A);
        let slot = self.get_element(head);
        
        // Store the element
        slot.store(element, ordering::R);
    }
    
    fn pop(&self) -> T {
        // Increment tail and get the slot
        let tail = self.common.tail.fetch_add(1, ordering::A);
        let slot = self.get_element(tail);
        let zero = T::default();
        
        // Wait until the element is available
        loop {
            let current = slot.load(ordering::A);
            if current == zero {
                // The producer hasn't stored the element yet, wait
                spin_loop_pause();
                continue;
            }
            
            // Replace the element with zero
            slot.store(zero, ordering::R);
            return current;
        }
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

impl<T: Element, const SIZE: usize> Default for AtomicQueue<T, SIZE> {
    fn default() -> Self {
        Self::new()
    }
}

/// A lock-free fixed-size queue for atomic elements with runtime capacity
pub struct AtomicQueueB<T: Element> {
    /// The common queue state (head and tail indices)
    common: AtomicQueueCommon,
    
    /// The storage for queue elements
    elements: *mut CachePadded<AtomicValue<T>>,
    
    /// The actual capacity (rounded up to power of 2)
    capacity: usize,
    
    /// Marker for variance and drop check
    _marker: PhantomData<Box<[CachePadded<AtomicValue<T>>]>>,
}

// Safety: AtomicQueueB<T> uses thread-safe atomics internally
unsafe impl<T: Element> Send for AtomicQueueB<T> {}
unsafe impl<T: Element> Sync for AtomicQueueB<T> {}

impl<T: Element> AtomicQueueB<T> {
    /// Creates a new empty queue with the specified capacity
    pub fn new(size: usize) -> Self {
        let capacity = round_up_to_power_of_2(size.max(2)); // Ensure at least capacity 2
        
        // Allocate memory for elements
        let layout = Layout::array::<CachePadded<AtomicValue<T>>>(capacity)
            .expect("Layout allocation failed");
            
        let elements = unsafe {
            let ptr = alloc::alloc(layout) as *mut CachePadded<AtomicValue<T>>;
            if ptr.is_null() {
                alloc::handle_alloc_error(layout);
            }
            
            // Initialize all elements
            for i in 0..capacity {
                ptr::write(ptr.add(i), CachePadded::new(AtomicValue::new()));
            }
            
            ptr
        };
        
        Self {
            common: AtomicQueueCommon::new(),
            elements,
            capacity,
            _marker: PhantomData,
        }
    }
    
    /// Get an element at the specified index (with wrapping)
    #[inline(always)]
    fn get_element(&self, index: usize) -> &AtomicValue<T> {
        let idx = index & (self.capacity - 1); // Fast modulo for power of 2
        unsafe {
            let element = self.elements.add(idx);
            // Access the AtomicValue inside the CachePadded<AtomicValue>
            &*(*element).deref()
        }
    }
}

impl<T: Element> Drop for AtomicQueueB<T> {
    fn drop(&mut self) {
        unsafe {
            // Destroy all elements
            for i in 0..self.capacity {
                ptr::drop_in_place(self.elements.add(i));
            }
            
            // Deallocate memory
            let layout = Layout::array::<CachePadded<AtomicValue<T>>>(self.capacity)
                .expect("Layout deallocation failed");
            alloc::dealloc(self.elements as *mut u8, layout);
        }
    }
}

impl<T: Element> AtomicQueueOps<T> for AtomicQueueB<T> {
    fn try_push(&self, element: T) -> bool {
        // Check if the queue is full
        let head = self.common.head.load(ordering::X);
        let tail = self.common.tail.load(ordering::X);
        
        if head - tail >= self.capacity {
            return false; // Queue is full
        }
        
        // Try to claim a slot by incrementing the head
        if self.common.head.compare_exchange(head, head + 1, ordering::A, ordering::X).is_err() {
            return false; // Another producer claimed the slot
        }
        
        // Store the element at the claimed position
        let slot = self.get_element(head);
        slot.store(element, ordering::R);
        
        true
    }
    
    fn try_pop(&self) -> Option<T> {
        // Check if the queue is empty
        let tail = self.common.tail.load(ordering::X);
        let head = self.common.head.load(ordering::X);
        
        if tail == head {
            return None; // Queue is empty
        }
        
        // Try to claim a slot by incrementing the tail
        if self.common.tail.compare_exchange(tail, tail + 1, ordering::A, ordering::X).is_err() {
            return None; // Another consumer claimed the slot
        }
        
        // Load the element from the claimed position
        let slot = self.get_element(tail);
        let zero = T::default();
        
        // Try to get the element and replace it with zero
        loop {
            let current = slot.load(ordering::A);
            if current == zero {
                // The producer hasn't stored the element yet, wait
                spin_loop_pause();
                continue;
            }
            
            // Replace the element with zero
            slot.store(zero, ordering::R);
            return Some(current);
        }
    }
    
    fn push(&self, element: T) {
        // Increment head and get the slot
        let head = self.common.head.fetch_add(1, ordering::A);
        let slot = self.get_element(head);
        
        // Store the element
        slot.store(element, ordering::R);
    }
    
    fn pop(&self) -> T {
        // Increment tail and get the slot
        let tail = self.common.tail.fetch_add(1, ordering::A);
        let slot = self.get_element(tail);
        let zero = T::default();
        
        // Wait until the element is available
        loop {
            let current = slot.load(ordering::A);
            if current == zero {
                // The producer hasn't stored the element yet, wait
                spin_loop_pause();
                continue;
            }
            
            // Replace the element with zero
            slot.store(zero, ordering::R);
            return current;
        }
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

/// A faster version of AtomicQueue that busy-waits when empty or full
pub type OptimistAtomicQueue<T: Element, const SIZE: usize> = AtomicQueue<T, SIZE>;

/// A faster version of AtomicQueueB that busy-waits when empty or full
pub type OptimistAtomicQueueB<T: Element> = AtomicQueueB<T>;

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::thread;
    
    #[test]
    fn test_atomic_queue_basic() {
        let q = AtomicQueue::<u32, 16>::new();
        
        // Test push and pop
        assert!(q.try_push(1));
        assert!(q.try_push(2));
        assert!(q.try_push(3));
        
        assert_eq!(q.try_pop(), Some(1));
        assert_eq!(q.try_pop(), Some(2));
        assert_eq!(q.try_pop(), Some(3));
        assert_eq!(q.try_pop(), None);
    }
    
    #[test]
    fn test_atomic_queue_full() {
        let q = AtomicQueue::<u32, 3>::new();
        
        // Fill the queue
        assert!(q.try_push(1));
        assert!(q.try_push(2));
        assert!(q.try_push(3));
        
        // Queue should be full
        assert!(!q.try_push(4));
        
        // Make space and try again
        assert_eq!(q.try_pop(), Some(1));
        assert!(q.try_push(4));
        
        // Check remaining values
        assert_eq!(q.try_pop(), Some(2));
        assert_eq!(q.try_pop(), Some(3));
        assert_eq!(q.try_pop(), Some(4));
        assert_eq!(q.try_pop(), None);
    }
    
    #[test]
    fn test_atomic_queueb_basic() {
        let q = AtomicQueueB::<u32>::new(16);
        
        // Test push and pop
        assert!(q.try_push(1));
        assert!(q.try_push(2));
        assert!(q.try_push(3));
        
        assert_eq!(q.try_pop(), Some(1));
        assert_eq!(q.try_pop(), Some(2));
        assert_eq!(q.try_pop(), Some(3));
        assert_eq!(q.try_pop(), None);
    }
    
    #[test]
    fn test_atomic_queue_threaded() {
        const NUM_PRODUCERS: usize = 4;
        const NUM_CONSUMERS: usize = 4;
        const ITEMS_PER_PRODUCER: usize = 1000;
        
        let q = Arc::new(AtomicQueue::<usize, 1024>::new());
        
        // Track expected sum
        let expected_sum = (0..ITEMS_PER_PRODUCER).sum::<usize>() * NUM_PRODUCERS;
        
        // Spawn producer threads
        let mut producer_threads = Vec::new();
        for _ in 0..NUM_PRODUCERS {
            let q2 = q.clone();
            producer_threads.push(thread::spawn(move || {
                for i in 0..ITEMS_PER_PRODUCER {
                    q2.push(i);
                }
            }));
        }
        
        // Spawn consumer threads
        let results = Arc::new(std::sync::Mutex::new(Vec::new()));
        let mut consumer_threads = Vec::new();
        for _ in 0..NUM_CONSUMERS {
            let q2 = q.clone();
            let results2 = results.clone();
            consumer_threads.push(thread::spawn(move || {
                let mut local_sum = 0;
                let mut count = 0;
                
                while count < (ITEMS_PER_PRODUCER * NUM_PRODUCERS) / NUM_CONSUMERS {
                    if let Some(value) = q2.try_pop() {
                        local_sum += value;
                        count += 1;
                    }
                }
                
                let mut results = results2.lock().unwrap();
                results.push(local_sum);
            }));
        }
        
        // Wait for producers and consumers to finish
        for handle in producer_threads {
            handle.join().unwrap();
        }
        
        for handle in consumer_threads {
            handle.join().unwrap();
        }
        
        // Check results
        let results = results.lock().unwrap();
        let total_sum: usize = results.iter().sum();
        
        assert_eq!(total_sum, expected_sum);
    }
}
