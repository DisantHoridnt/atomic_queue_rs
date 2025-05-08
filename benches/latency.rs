use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId};
use atomic_queue_rs::{AtomicQueue, AtomicQueueB, AtomicQueue2, AtomicQueueB2};
use atomic_queue_rs::{OptimistAtomicQueue, OptimistAtomicQueueB, OptimistAtomicQueue2, OptimistAtomicQueueB2};
use std::sync::Arc;
use std::thread;

// Queue capacity for benchmarks
const CAPACITY: usize = 1024;
// Number of ping-pong operations per benchmark
const PING_PONGS: usize = 100_000;

fn bench_latency(c: &mut Criterion) {
    let mut group = c.benchmark_group("latency");

    // 1. AtomicQueue
    group.bench_function(
        BenchmarkId::new("OptimistAtomicQueue", "ping-pong"), 
        |b| {
            b.iter(|| {
                let q1 = Arc::new(OptimistAtomicQueue::<u32, CAPACITY>::new());
                let q2 = Arc::new(OptimistAtomicQueue::<u32, CAPACITY>::new());
                
                // Ping thread
                let q1_ping = q1.clone();
                let q2_ping = q2.clone();
                let ping_thread = thread::spawn(move || {
                    for i in 0..PING_PONGS {
                        q1_ping.push(black_box(i as u32));
                        black_box(q2_ping.pop());
                    }
                });
                
                // Pong thread
                let pong_thread = thread::spawn(move || {
                    for _ in 0..PING_PONGS {
                        let val = q1.pop();
                        q2.push(black_box(val));
                    }
                });
                
                ping_thread.join().unwrap();
                pong_thread.join().unwrap();
            })
        },
    );
    
    // 2. AtomicQueueB
    group.bench_function(
        BenchmarkId::new("OptimistAtomicQueueB", "ping-pong"), 
        |b| {
            b.iter(|| {
                let q1 = Arc::new(OptimistAtomicQueueB::<u32>::new(CAPACITY));
                let q2 = Arc::new(OptimistAtomicQueueB::<u32>::new(CAPACITY));
                
                // Ping thread
                let q1_ping = q1.clone();
                let q2_ping = q2.clone();
                let ping_thread = thread::spawn(move || {
                    for i in 0..PING_PONGS {
                        q1_ping.push(black_box(i as u32));
                        black_box(q2_ping.pop());
                    }
                });
                
                // Pong thread
                let pong_thread = thread::spawn(move || {
                    for _ in 0..PING_PONGS {
                        let val = q1.pop();
                        q2.push(black_box(val));
                    }
                });
                
                ping_thread.join().unwrap();
                pong_thread.join().unwrap();
            })
        },
    );
    
    // 3. AtomicQueue2
    group.bench_function(
        BenchmarkId::new("OptimistAtomicQueue2", "ping-pong"), 
        |b| {
            b.iter(|| {
                let q1 = Arc::new(OptimistAtomicQueue2::<u32, CAPACITY>::new());
                let q2 = Arc::new(OptimistAtomicQueue2::<u32, CAPACITY>::new());
                
                // Ping thread
                let q1_ping = q1.clone();
                let q2_ping = q2.clone();
                let ping_thread = thread::spawn(move || {
                    for i in 0..PING_PONGS {
                        q1_ping.push(black_box(i as u32));
                        black_box(q2_ping.pop());
                    }
                });
                
                // Pong thread
                let pong_thread = thread::spawn(move || {
                    for _ in 0..PING_PONGS {
                        let val = q1.pop();
                        q2.push(black_box(val));
                    }
                });
                
                ping_thread.join().unwrap();
                pong_thread.join().unwrap();
            })
        },
    );
    
    // 4. AtomicQueueB2
    group.bench_function(
        BenchmarkId::new("OptimistAtomicQueueB2", "ping-pong"), 
        |b| {
            b.iter(|| {
                let q1 = Arc::new(OptimistAtomicQueueB2::<u32>::new(CAPACITY));
                let q2 = Arc::new(OptimistAtomicQueueB2::<u32>::new(CAPACITY));
                
                // Ping thread
                let q1_ping = q1.clone();
                let q2_ping = q2.clone();
                let ping_thread = thread::spawn(move || {
                    for i in 0..PING_PONGS {
                        q1_ping.push(black_box(i as u32));
                        black_box(q2_ping.pop());
                    }
                });
                
                // Pong thread
                let pong_thread = thread::spawn(move || {
                    for _ in 0..PING_PONGS {
                        let val = q1.pop();
                        q2.push(black_box(val));
                    }
                });
                
                ping_thread.join().unwrap();
                pong_thread.join().unwrap();
            })
        },
    );
    
    group.finish();
}

criterion_group!(benches, bench_latency);
criterion_main!(benches);
