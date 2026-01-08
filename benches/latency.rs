use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use bytes::Bytes;

// Simulated message forwarding
fn forward_message_sync(payload: &Bytes, broker_count: usize) {
    for _ in 0..broker_count {
        let _cloned = payload.clone(); // Bytes is cheap to clone (Arc internally)
        // Simulate sending
        black_box(_cloned);
    }
}

fn latency_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("message_latency");

    // Test different payload sizes
    for size in [64, 256, 1024, 4096, 16384].iter() {
        let payload = Bytes::from(vec![0u8; *size]);

        group.bench_with_input(
            BenchmarkId::new("1_broker", size),
            &payload,
            |b, payload| {
                b.iter(|| forward_message_sync(payload, 1))
            },
        );

        group.bench_with_input(
            BenchmarkId::new("3_brokers", size),
            &payload,
            |b, payload| {
                b.iter(|| forward_message_sync(payload, 3))
            },
        );

        group.bench_with_input(
            BenchmarkId::new("10_brokers", size),
            &payload,
            |b, payload| {
                b.iter(|| forward_message_sync(payload, 10))
            },
        );
    }

    group.finish();
}

criterion_group!(benches, latency_benchmark);
criterion_main!(benches);
