use atomic_queue_rs::{AtomicQueueB, OptimistAtomicQueueB};
use std::thread;
use std::sync::Arc;
use std::time::Instant;

fn main() {
    println!("AtomicQueue Rust Example");
    println!("------------------------\n");
    
    // Configuration
    const PRODUCERS: usize = 1; // Number of producer threads
    const CONSUMERS: usize = 2; // Number of consumer threads
    const N: u32 = 1_000_000; // Each producer pushes this many elements into the queue
    const CAPACITY: usize = 1024; // Queue capacity

    // Create a queue object shared between all producers and consumers
    let queue = Arc::new(AtomicQueueB::<u32>::new(CAPACITY));
    
    println!("Starting {} producers and {} consumers", PRODUCERS, CONSUMERS);
    println!("Each producer will push {} elements", N);
    println!("Queue capacity: {}\n", CAPACITY);
    
    let start_time = Instant::now();
    
    // Start the consumers
    let mut sums = vec![0u64; CONSUMERS];
    let sums_arc = Arc::new(std::sync::Mutex::new(sums));
    let mut consumer_threads = Vec::with_capacity(CONSUMERS);
    
    for i in 0..CONSUMERS {
        let q = queue.clone();
        let sums = sums_arc.clone();
        consumer_threads.push(thread::spawn(move || {
            let mut local_sum = 0u64;
            
            // Keep popping elements until we get a 0 (termination signal)
            while let Some(n) = q.try_pop() {
                if n == 0 {
                    break;
                }
                local_sum += n as u64;
            }
            
            // Update the global sum array (only once to avoid false sharing)
            let mut sums = sums.lock().unwrap();
            sums[i] = local_sum;
        }));
    }
    
    // Start the producers
    let mut producer_threads = Vec::with_capacity(PRODUCERS);
    
    for _ in 0..PRODUCERS {
        let q = queue.clone();
        producer_threads.push(thread::spawn(move || {
            // Push elements in descending order [N, 1]
            for n in (1..=N).rev() {
                q.push(n);
            }
        }));
    }
    
    // Wait for all producers to finish
    for handle in producer_threads {
        handle.join().unwrap();
    }
    
    // Tell consumers to terminate by pushing one 0 for each consumer
    for _ in 0..CONSUMERS {
        queue.push(0);
    }
    
    // Wait for all consumers to finish
    for handle in consumer_threads {
        handle.join().unwrap();
    }
    
    // Calculate and verify the total sum
    let sums = sums_arc.lock().unwrap();
    let total_sum: u64 = sums.iter().sum();
    
    // The expected sum is N*(N+1)/2 * PRODUCERS
    let expected_sum: u64 = (N as u64 * (N as u64 + 1) / 2) * PRODUCERS as u64;
    
    println!("Execution time: {:?}", start_time.elapsed());
    println!("Total sum: {}", total_sum);
    println!("Expected sum: {}", expected_sum);
    
    // Verify the result
    if total_sum != expected_sum {
        println!("ERROR: Sum mismatch! Difference: {}", total_sum as i64 - expected_sum as i64);
    } else {
        println!("SUCCESS: All elements were correctly processed.");
    }
    
    // Show per-consumer stats
    println!("\nPer-consumer statistics:");
    for (i, &sum) in sums.iter().enumerate() {
        println!("Consumer {}: sum = {}", i, sum);
        if sum == 0 {
            println!("WARNING: Consumer {} received no elements!", i);
        }
    }
}
