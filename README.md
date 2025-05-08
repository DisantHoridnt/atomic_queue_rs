# atomic_queue_rs

A Rust implementation of lock-free multiple-producer-multiple-consumer queues based on circular buffers and atomic operations. This is a port of the C++ [atomic_queue](https://github.com/max0x7ba/atomic_queue) library.

[![Crates.io](https://img.shields.io/crates/v/atomic_queue_rs.svg)](https://crates.io/crates/atomic_queue_rs)
[![Docs.rs](https://docs.rs/atomic_queue_rs/badge.svg)](https://docs.rs/atomic_queue_rs)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

## Features

- Multiple lock-free queue implementations:
  - `AtomicQueue` - Fixed-size ring-buffer for atomic elements with compile-time capacity
  - `AtomicQueueB` - Fixed-size ring-buffer for atomic elements with runtime capacity
  - `AtomicQueue2` - Fixed-size ring-buffer for non-atomic elements with compile-time capacity
  - `AtomicQueueB2` - Fixed-size ring-buffer for non-atomic elements with runtime capacity
  - `OptimistAtomicQueue` & variants - Faster implementations that busy-wait when empty or full
- Designed to minimize latency between push and pop operations
- Architecture-specific optimizations for x86_64, ARM, RISC-V, PowerPC, etc.
- Optional total ordering of operations
- Single-producer-single-consumer (SPSC) optimizations
- Cache-line contention avoidance

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
atomic_queue_rs = "0.1.0"
```

## Usage Example

```rust
use atomic_queue_rs::{AtomicQueueB, Element};
use std::thread;

fn main() {
    const PRODUCERS: usize = 1;
    const CONSUMERS: usize = 2;
    const N: u32 = 1_000_000;
    const CAPACITY: usize = 1024;

    // Create a queue shared between producers and consumers
    let queue = AtomicQueueB::<u32>::new(CAPACITY);
    let queue = std::sync::Arc::new(queue);

    // Start consumers
    let mut consumer_handles = Vec::with_capacity(CONSUMERS);
    let sums = std::sync::Arc::new(std::sync::Mutex::new(vec![0u64; CONSUMERS]));

    for i in 0..CONSUMERS {
        let q = queue.clone();
        let sums_clone = sums.clone();
        let handle = thread::spawn(move || {
            let mut sum = 0u64;
            while let Some(n) = q.pop() {
                if n == 0 {
                    break;
                }
                sum += n as u64;
            }
            let mut sums = sums_clone.lock().unwrap();
            sums[i] = sum;
        });
        consumer_handles.push(handle);
    }

    // Start producers
    let mut producer_handles = Vec::with_capacity(PRODUCERS);
    for _ in 0..PRODUCERS {
        let q = queue.clone();
        let handle = thread::spawn(move || {
            for n in (1..=N).rev() {
                q.push(n);
            }
        });
        producer_handles.push(handle);
    }

    // Wait for producers to finish
    for handle in producer_handles {
        handle.join().unwrap();
    }

    // Tell consumers to terminate
    for _ in 0..CONSUMERS {
        queue.push(0);
    }

    // Wait for consumers to finish
    for handle in consumer_handles {
        handle.join().unwrap();
    }

    // Verify results
    let sums = sums.lock().unwrap();
    let total_sum: u64 = sums.iter().sum();
    let expected_sum = (N as u64 * (N as u64 + 1) / 2) * PRODUCERS as u64;
    
    assert_eq!(total_sum, expected_sum, "Sum mismatch: got {}, expected {}", total_sum, expected_sum);
    println!("Success! All elements processed correctly.");
}
```

## Performance

This library is designed with a goal to minimize the latency between one thread pushing an element into a queue and another thread popping it from the queue.

## Safety

This crate uses `unsafe` code for atomic operations and memory management. The implementation has been carefully designed and tested, but as with any lock-free code, extreme care should be taken.

## License

This project is licensed under the MIT License - see the LICENSE file for details.
