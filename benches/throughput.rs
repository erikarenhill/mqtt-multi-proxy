use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use bytes::Bytes;

fn throughput_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("message_throughput");

    for size in [64, 256, 1024, 4096].iter() {
        let payload = Bytes::from(vec![0u8; *size]);

        // Set throughput to measure messages/sec
        group.throughput(Throughput::Elements(1));

        group.bench_with_input(
            BenchmarkId::from_parameter(size),
            &payload,
            |b, payload| {
                b.iter(|| {
                    // Simulate message processing
                    let _cloned = payload.clone();
                })
            },
        );
    }

    group.finish();
}

criterion_group!(benches, throughput_benchmark);
criterion_main!(benches);
