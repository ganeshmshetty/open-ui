//! SPEC §3.8 non-gating release measurements. Setup is excluded from chunk timing.
use criterion::{BatchSize, Criterion, criterion_group, criterion_main};
use genui_core::{
    Session, SessionOptions,
    limits::SyntaxLimits,
    syntax::parse_batch,
    validate::{Options, validate},
};
use std::{
    hint::black_box,
    sync::Arc,
    time::{Duration, Instant},
};
fn prefix() -> String {
    let mut text = String::new();
    for i in 0..998 {
        text.push_str(&format!("n{i}=Divider()\n"));
    }
    text.push_str("root=Column([");
    for i in 0..998 {
        text.push_str(&format!("n{i},"));
    }
    text.push_str("Text(\"live");
    text
}
fn batch() -> String {
    let mut text = String::new();
    for i in 0..1900 {
        text.push_str(&format!("n{i}=Text(\"{}\")\n", "x".repeat(88)));
    }
    text.push_str("root=Column([n0,n1])\n");
    assert!(text.len() <= 200 * 1024);
    text.push_str("//");
    text.extend(std::iter::repeat_n('x', 200 * 1024 - text.len()));
    assert_eq!(text.len(), 200 * 1024);
    text
}
fn benchmarks(c: &mut Criterion) {
    let library = Arc::new(genui_std::std_library().unwrap());
    let prefix = prefix();
    let chunk = [b'x'; 64];
    let setup = || {
        let mut s = Session::new(SessionOptions::program(library.clone()));
        s.push(prefix.as_bytes());
        assert_eq!(s.ir().nodes.len(), 1000);
        s
    };
    let mut samples = Vec::new();
    for _ in 0..200 {
        let mut s = setup();
        let start = Instant::now();
        black_box(s.push(&chunk));
        samples.push(start.elapsed().as_nanos());
    }
    samples.sort_unstable();
    println!("M3_SINGLE_CHUNK_P99_NS={}", samples[197]);
    c.bench_function("64_byte_chunk_1000_nodes", |b| {
        b.iter_batched_ref(
            setup,
            |s| black_box(s.push(black_box(&chunk))),
            BatchSize::LargeInput,
        )
    });
    let input = batch();
    c.bench_function("batch_syntax_200_kib", |b| {
        b.iter(|| {
            black_box(parse_batch(
                black_box(input.as_bytes()),
                SyntaxLimits::default(),
            ))
        })
    });
    c.bench_function("batch_validated_200_kib", |b| {
        b.iter(|| {
            black_box(validate(
                black_box(input.as_bytes()),
                &library,
                &Options::default(),
            ))
        })
    });
}
criterion_group! {name=benches;config=Criterion::default().sample_size(20).warm_up_time(Duration::from_secs(1)).measurement_time(Duration::from_secs(2));targets=benchmarks}
criterion_main!(benches);
