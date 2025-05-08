use atomic_queue_rs::{AtomicQueue, AtomicQueueB, AtomicQueue2, AtomicQueueB2};
use atomic_queue_rs::{OptimistAtomicQueue, OptimistAtomicQueueB, OptimistAtomicQueue2, OptimistAtomicQueueB2};
use std::sync::{Arc, Barrier};
use std::thread;
use std::time::{Duration, Instant};
use std::sync::Mutex;

// Queue capacity
const CAPACITY: usize = 1024;
// Number of messages to send per producer
const MESSAGES_PER_PRODUCER: usize = 1_000_000;
// Warmup runs
const WARMUP_RUNS: usize = 3;
// Timing runs
const TIMING_RUNS: usize = 10;
// Ping-pong iterations
const PING_PONG_ITERATIONS: usize = 100_000;

// Benchmark results
struct BenchmarkResult {
    name: String,
    throughput_mpmc: Vec<f64>,  // Million operations per second
    latency_ns: Vec<f64>,       // Nanoseconds per round-trip
}

fn main() {
    println!("Atomic Queue Rust Benchmarks");
    println!("----------------------------\n");
    
    // Get number of CPUs
    let num_cpus = num_cpus::get();
    println!("Number of CPUs: {}", num_cpus);
    
    let mut results = Vec::new();
    
    // Benchmark various queue implementations
    benchmark_queue::<OptimistAtomicQueue<u32, CAPACITY>, u32>(&mut results, "OptimistAtomicQueue<u32>");
    benchmark_queue::<OptimistAtomicQueueB<u32>, u32>(&mut results, "OptimistAtomicQueueB<u32>");
    benchmark_queue::<OptimistAtomicQueue2<u32, CAPACITY>, u32>(&mut results, "OptimistAtomicQueue2<u32>");
    benchmark_queue::<OptimistAtomicQueueB2<u32>, u32>(&mut results, "OptimistAtomicQueueB2<u32>");
    
    // Print summary results
    println!("\nSummary Results:");
    println!("{:<30} {:>15} {:>15}", "Queue Type", "Throughput (Mops/s)", "Latency (ns)");
    println!("{:<30} {:>15} {:>15}", "---------", "------------------", "------------");
    
    for result in results {
        let avg_throughput = result.throughput_mpmc.iter().sum::<f64>() / result.throughput_mpmc.len() as f64;
        let avg_latency = result.latency_ns.iter().sum::<f64>() / result.latency_ns.len() as f64;
        
        println!(
            "{:<30} {:>15.2} {:>15.2}",
            result.name,
            avg_throughput,
            avg_latency
        );
    }
}

fn benchmark_queue<Q, T>(results: &mut Vec<BenchmarkResult>, name: &str) 
where
    Q: atomic_queue_rs::AtomicQueueOps<T> + Send + Sync + 'static,
    T: Copy + Default + Send + Sync + From<u32> + Into<u64> + 'static,
{
    println!("\nBenchmarking {}", name);
    
    let mut throughput_results = Vec::new();
    let mut latency_results = Vec::new();
    
    // Throughput benchmark (MPMC)
    for run in 0..(WARMUP_RUNS + TIMING_RUNS) {
        let producers_consumers = if run < WARMUP_RUNS { 1 } else { (num_cpus::get() / 2).max(1) };
        let is_warmup = run < WARMUP_RUNS;
        
        if !is_warmup {
            print!("  MPMC throughput with {} producers & consumers: ", producers_consumers);
        }
        
        let queue = Arc::new(Q::default());
        let barrier = Arc::new(Barrier::new(producers_consumers * 2));  // producers + consumers
        let start_time = Arc::new(Mutex::new(None::<Instant>));
        let end_time = Arc::new(Mutex::new(None::<Instant>));
        
        // Start consumers
        let mut consumer_handles = Vec::with_capacity(producers_consumers);
        for _ in 0..producers_consumers {
            let q = queue.clone();
            let b = barrier.clone();
            let end = end_time.clone();
            
            consumer_handles.push(thread::spawn(move || {
                let mut count = 0;
                let messages_per_consumer = MESSAGES_PER_PRODUCER * producers_consumers / producers_consumers;
                
                // Wait for all threads to be ready
                b.wait();
                
                while count < messages_per_consumer {
                    if let Some(val) = q.try_pop() {
                        count += 1;
                    }
                }
                
                // Update end time if we're the last consumer
                let mut end = end.lock().unwrap();
                if end.is_none() {
                    *end = Some(Instant::now());
                }
            }));
        }
        
        // Start producers
        let mut producer_handles = Vec::with_capacity(producers_consumers);
        for _ in 0..producers_consumers {
            let q = queue.clone();
            let b = barrier.clone();
            let start = start_time.clone();
            
            producer_handles.push(thread::spawn(move || {
                let messages_per_producer = MESSAGES_PER_PRODUCER / producers_consumers;
                
                // Wait for all threads to be ready
                b.wait();
                
                // Update start time if we're the first producer
                {
                    let mut start = start.lock().unwrap();
                    if start.is_none() {
                        *start = Some(Instant::now());
                    }
                }
                
                for i in 0..messages_per_producer {
                    q.push(T::from(i as u32));
                }
            }));
        }
        
        // Wait for all threads to complete
        for handle in producer_handles {
            handle.join().unwrap();
        }
        for handle in consumer_handles {
            handle.join().unwrap();
        }
        
        // Calculate throughput
        let start = *start_time.lock().unwrap();
        let end = *end_time.lock().unwrap();
        
        if let (Some(start), Some(end)) = (start, end) {
            let duration = end.duration_since(start);
            let seconds = duration.as_secs() as f64 + duration.subsec_nanos() as f64 / 1_000_000_000.0;
            let ops_per_sec = MESSAGES_PER_PRODUCER as f64 / seconds;
            let mops_per_sec = ops_per_sec / 1_000_000.0;
            
            if !is_warmup {
                println!("{:.2} Mops/s", mops_per_sec);
                throughput_results.push(mops_per_sec);
            }
        }
        
        // Let the system cool down a bit between runs
        thread::sleep(Duration::from_millis(100));
    }
    
    // Latency benchmark (ping-pong)
    for run in 0..(WARMUP_RUNS + TIMING_RUNS) {
        let is_warmup = run < WARMUP_RUNS;
        
        if !is_warmup {
            print!("  Ping-pong latency: ");
        }
        
        let q1 = Arc::new(Q::default());
        let q2 = Arc::new(Q::default());
        
        let start = Instant::now();
        
        // Create ping thread
        let q1_clone = q1.clone();
        let q2_clone = q2.clone();
        let ping_thread = thread::spawn(move || {
            for i in 0..PING_PONG_ITERATIONS {
                q1_clone.push(T::from(i as u32));
                q2_clone.pop();
            }
        });
        
        // Create pong thread
        let pong_thread = thread::spawn(move || {
            for _ in 0..PING_PONG_ITERATIONS {
                let val = q1.pop();
                q2.push(val);
            }
        });
        
        // Wait for both threads to complete
        ping_thread.join().unwrap();
        pong_thread.join().unwrap();
        
        let duration = start.elapsed();
        let round_trip_ns = (duration.as_secs() * 1_000_000_000 + duration.subsec_nanos() as u64) as f64 
                            / PING_PONG_ITERATIONS as f64;
        
        if !is_warmup {
            println!("{:.2} ns", round_trip_ns);
            latency_results.push(round_trip_ns);
        }
        
        // Let the system cool down a bit between runs
        thread::sleep(Duration::from_millis(100));
    }
    
    // Store the results
    results.push(BenchmarkResult {
        name: name.to_string(),
        throughput_mpmc: throughput_results,
        latency_ns: latency_results,
    });
}
