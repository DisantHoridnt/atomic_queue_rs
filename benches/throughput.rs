use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId, Throughput};
use atomic_queue_rs::{AtomicQueue, AtomicQueueB, AtomicQueue2, AtomicQueueB2};
use atomic_queue_rs::{OptimistAtomicQueue, OptimistAtomicQueueB, OptimistAtomicQueue2, OptimistAtomicQueueB2};
use std::sync::{Arc, Barrier};
use std::thread;

// Queue capacity for benchmarks
const CAPACITY: usize = 1024;
// Number of operations per benchmark
const OPS_PER_BENCH: usize = 1_000_000;

fn bench_throughput(c: &mut Criterion) {
    let mut group = c.benchmark_group("throughput");
    group.throughput(Throughput::Elements(OPS_PER_BENCH as u64));

    // Test different thread counts
    for threads in [1, 2, 4].iter() {
        // Skip configurations that would require more than available CPUs
        if *threads * 2 > num_cpus::get() {
            continue;
        }

        // 1. AtomicQueue
        group.bench_with_input(
            BenchmarkId::new("OptimistAtomicQueue", threads), 
            threads, 
            |b, &threads| {
                b.iter(|| {
                    let queue = Arc::new(OptimistAtomicQueue::<u32, CAPACITY>::new());
                    let barrier = Arc::new(Barrier::new(threads * 2));
                    
                    let mut handles = Vec::with_capacity(threads * 2);
                    
                    // Producers
                    for _ in 0..*threads {
                        let q = queue.clone();
                        let b = barrier.clone();
                        handles.push(thread::spawn(move || {
                            b.wait();
                            for i in 0..(OPS_PER_BENCH / *threads) {
                                q.push(black_box(i as u32));
                            }
                        }));
                    }
                    
                    // Consumers
                    for _ in 0..*threads {
                        let q = queue.clone();
                        let b = barrier.clone();
                        handles.push(thread::spawn(move || {
                            b.wait();
                            for _ in 0..(OPS_PER_BENCH / *threads) {
                                black_box(q.pop());
                            }
                        }));
                    }
                    
                    for handle in handles {
                        handle.join().unwrap();
                    }
                })
            },
        );
        
        // 2. AtomicQueueB
        group.bench_with_input(
            BenchmarkId::new("OptimistAtomicQueueB", threads), 
            threads, 
            |b, &threads| {
                b.iter(|| {
                    let queue = Arc::new(OptimistAtomicQueueB::<u32>::new(CAPACITY));
                    let barrier = Arc::new(Barrier::new(threads * 2));
                    
                    let mut handles = Vec::with_capacity(threads * 2);
                    
                    // Producers
                    for _ in 0..*threads {
                        let q = queue.clone();
                        let b = barrier.clone();
                        handles.push(thread::spawn(move || {
                            b.wait();
                            for i in 0..(OPS_PER_BENCH / *threads) {
                                q.push(black_box(i as u32));
                            }
                        }));
                    }
                    
                    // Consumers
                    for _ in 0..*threads {
                        let q = queue.clone();
                        let b = barrier.clone();
                        handles.push(thread::spawn(move || {
                            b.wait();
                            for _ in 0..(OPS_PER_BENCH / *threads) {
                                black_box(q.pop());
                            }
                        }));
                    }
                    
                    for handle in handles {
                        handle.join().unwrap();
                    }
                })
            },
        );
        
        // 3. AtomicQueue2
        group.bench_with_input(
            BenchmarkId::new("OptimistAtomicQueue2", threads), 
            threads, 
            |b, &threads| {
                b.iter(|| {
                    let queue = Arc::new(OptimistAtomicQueue2::<u32, CAPACITY>::new());
                    let barrier = Arc::new(Barrier::new(threads * 2));
                    
                    let mut handles = Vec::with_capacity(threads * 2);
                    
                    // Producers
                    for _ in 0..*threads {
                        let q = queue.clone();
                        let b = barrier.clone();
                        handles.push(thread::spawn(move || {
                            b.wait();
                            for i in 0..(OPS_PER_BENCH / *threads) {
                                q.push(black_box(i as u32));
                            }
                        }));
                    }
                    
                    // Consumers
                    for _ in 0..*threads {
                        let q = queue.clone();
                        let b = barrier.clone();
                        handles.push(thread::spawn(move || {
                            b.wait();
                            for _ in 0..(OPS_PER_BENCH / *threads) {
                                black_box(q.pop());
                            }
                        }));
                    }
                    
                    for handle in handles {
                        handle.join().unwrap();
                    }
                })
            },
        );
        
        // 4. AtomicQueueB2
        group.bench_with_input(
            BenchmarkId::new("OptimistAtomicQueueB2", threads), 
            threads, 
            |b, &threads| {
                b.iter(|| {
                    let queue = Arc::new(OptimistAtomicQueueB2::<u32>::new(CAPACITY));
                    let barrier = Arc::new(Barrier::new(threads * 2));
                    
                    let mut handles = Vec::with_capacity(threads * 2);
                    
                    // Producers
                    for _ in 0..*threads {
                        let q = queue.clone();
                        let b = barrier.clone();
                        handles.push(thread::spawn(move || {
                            b.wait();
                            for i in 0..(OPS_PER_BENCH / *threads) {
                                q.push(black_box(i as u32));
                            }
                        }));
                    }
                    
                    // Consumers
                    for _ in 0..*threads {
                        let q = queue.clone();
                        let b = barrier.clone();
                        handles.push(thread::spawn(move || {
                            b.wait();
                            for _ in 0..(OPS_PER_BENCH / *threads) {
                                black_box(q.pop());
                            }
                        }));
                    }
                    
                    for handle in handles {
                        handle.join().unwrap();
                    }
                })
            },
        );
    }
    
    group.finish();
}

criterion_group!(benches, bench_throughput);
criterion_main!(benches);
