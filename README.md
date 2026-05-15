# csv-data-quality

[![CI](https://github.com/mizcausevic-dev/csv-data-quality-rs/actions/workflows/ci.yml/badge.svg)](https://github.com/mizcausevic-dev/csv-data-quality-rs/actions/workflows/ci.yml)
[![Rust](https://img.shields.io/badge/rust-1.86%2B-orange)](https://www.rust-lang.org/)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)

**Streaming CSV validator against a [`data-contract-registry`](https://github.com/mizcausevic-dev/data-contract-registry) contract.** Reads a CSV row by row, checks each cell against the contract's field-type / required / enum rules, and emits a structured violation report.

The **fourth cross-ecosystem hook** in the Kinetic Gain portfolio.

```rust
use csv_data_quality::{Validator, Contract};

# async fn demo() -> Result<(), Box<dyn std::error::Error>> {
let contract = Contract::from_json(r#"{
  "dataset_id": "users.daily_active",
  "version": "1.0.0",
  "fields": [
    {"name": "user_id",     "type": "string"},
    {"name": "active_date", "type": "timestamp"},
    {"name": "plan",        "type": "string", "enum": ["free", "pro"]},
    {"name": "ltv",         "type": "number", "required": false}
  ]
}"#)?;

let validator = Validator::new(contract);
let report = validator.validate_file("daily_active_2026_05_15.csv").await?;
println!("{} violation(s)", report.violation_count);
# Ok(()) }
```

---

## Why

The registry says "the dataset must look like this." Producers have to be able to **prove** their output matches. CI runs that proof on every push: load the contract from the registry, validate the produced CSV, fail the build if violations show up. No drift, no surprise consumer outages.

---

## Violation kinds

| Kind | Triggers when |
| --- | --- |
| `Required` | A required cell is empty. |
| `BadType` | The cell doesn't parse as the declared `integer` / `number` / `boolean` / `timestamp`. |
| `EnumMismatch` | The cell value isn't one of the contract's enum entries. |
| `ColumnCountMismatch` | A row has a different number of columns than the header. |
| `InvalidJson` | A `json`-typed cell isn't valid JSON. |

Six primitive field types match the registry vocabulary: `string` · `integer` · `number` · `boolean` · `timestamp` · `json`.

---

## Report shape

```json
{
  "dataset_id": "users.daily_active",
  "contract_version": "1.0.0",
  "rows_scanned": 12345,
  "violation_count": 3,
  "valid": false,
  "samples": [
    { "row": 7, "column": "plan", "kind": "enum_mismatch", "message": "column \"plan\" value \"startup\" is not in declared enum" },
    { "row": 9, "column": "ltv", "kind": "bad_type", "message": "column \"ltv\" value \"not-a-number\" is not a valid number" },
    { "row": 12, "column": "user_id", "kind": "required", "message": "required column \"user_id\" is empty" }
  ]
}
```

`samples` is capped (default 100, configurable via `.max_samples(0)` for unlimited). `violation_count` is the true total, even when samples are truncated.

---

## Streaming

The validator is row-by-row. Memory cost is proportional to `max_samples`, not the file size. A 10GB CSV with `max_samples(100)` peaks at ~100 violation records plus one row's worth of cells.

---

## Composes with

- **[data-contract-registry](https://github.com/mizcausevic-dev/data-contract-registry)** — fetch contract by `dataset_id`; this crate validates against it. The fourth cross-ecosystem hook.
- **[audit-stream-py](https://github.com/mizcausevic-dev/audit-stream-py)** — emit a `contract_compatibility_failed` event when validation lights up.
- **[reliability-toolkit-rs](https://github.com/mizcausevic-dev/reliability-toolkit-rs)** — wrap the registry-fetch call in a circuit breaker.

---

## Example

```bash
cargo run --example validate
```

Validates a tiny in-memory CSV against an in-memory contract and prints the report. Useful for kicking the tyres without setting up a registry.

---

## Bench

```bash
cargo bench
```

Bundled bench validates 10k clean rows so you can spot regressions in the streaming path.

---

## Tests

```bash
cargo test --all-targets
cargo test --doc
cargo clippy --all-targets -- -Dwarnings
cargo fmt --all -- --check
```

CI matrix: `stable`, `beta`, `1.86.0` (MSRV). Fifteen tests cover the happy path, every violation kind, header mismatch, optional cells, JSON payload validation, sample cap, and the async file path.

---

## License

MIT. See [LICENSE](LICENSE).
