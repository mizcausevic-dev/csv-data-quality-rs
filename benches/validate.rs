//! Throughput micro-bench. Validates 10k rows of a 5-column contract. Run
//! with `cargo bench`.

use std::time::Duration;

use criterion::{criterion_group, criterion_main, Criterion};
use csv_data_quality::{Contract, Validator};

const CONTRACT: &str = r#"{
  "dataset_id": "users.daily_active",
  "version": "1.0.0",
  "fields": [
    {"name": "user_id",     "type": "string"},
    {"name": "active_date", "type": "timestamp"},
    {"name": "plan",        "type": "string", "enum": ["free", "pro"]},
    {"name": "ltv",         "type": "number", "required": false},
    {"name": "verified",    "type": "boolean"}
  ]
}"#;

fn build_csv(n: usize) -> Vec<u8> {
    let mut out = String::from("user_id,active_date,plan,ltv,verified\n");
    for i in 0..n {
        out.push_str(&format!("u{i},2026-05-15,pro,{i}.0,true\n"));
    }
    out.into_bytes()
}

fn bench_validate(c: &mut Criterion) {
    let v = Validator::new(Contract::from_json(CONTRACT).unwrap());
    let csv = build_csv(10_000);

    c.bench_function("validate_10k_rows", |b| {
        b.iter(|| {
            let report = v.validate_bytes(&csv).unwrap();
            assert!(report.valid);
        });
    });
}

criterion_group! {
    name = benches;
    config = Criterion::default().sample_size(20).measurement_time(Duration::from_secs(3));
    targets = bench_validate
}
criterion_main!(benches);
