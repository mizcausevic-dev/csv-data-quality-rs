# csv-data-quality

[![CI](https://github.com/mizcausevic-dev/csv-data-quality-rs/actions/workflows/ci.yml/badge.svg)](https://github.com/mizcausevic-dev/csv-data-quality-rs/actions/workflows/ci.yml)
[![Rust](https://img.shields.io/badge/rust-1.86%2B-orange)](https://www.rust-lang.org/)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)

**CSV validator for the JSON shape emitted by [`data-contract-registry`](https://github.com/mizcausevic-dev/data-contract-registry).** It reads a file record by record, checks declared column order, required cells, types, and enums, and returns a structured report. The caller supplies the contract JSON; this crate makes no registry request.

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

The registry declares the expected shape. A producer can pin a contract version, pass its JSON to this crate, and reject CSV output when `report.valid` is false. The included CI tests the validator itself; it does not fetch a registry contract or validate an external dataset. This check cannot establish freshness, uniqueness of primary keys, or downstream compatibility by itself.

---

## Violation kinds

| Kind | Triggers when |
| --- | --- |
| `Required` | A required cell is empty. |
| `BadType` | The cell doesn't match the declared `integer` / `number` / `boolean` / `timestamp`. Numbers must be finite; integers fit in signed 64-bit range. Dates use valid `YYYY-MM-DD`; date-times use `YYYY-MM-DDTHH:MM:SS` with optional fractional seconds and `Z` or numeric offset. |
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
    { "row": 7, "column": "plan", "kind": "enum_mismatch", "message": "column \"plan\" value is not in declared enum" },
    { "row": 9, "column": "ltv", "kind": "bad_type", "message": "column \"ltv\" value is not a valid number" },
    { "row": 12, "column": "user_id", "kind": "required", "message": "required column \"user_id\" is empty" }
  ]
}
```

`samples` is capped at 100 by default. `.max_samples(0)` retains no samples; requested caps above 10,000 are reduced to 10,000. `violation_count` remains the total when samples are omitted. Messages do not echo CSV cell values or unexpected header text.

---

## Streaming

`validate_file` opens a buffered file reader on a blocking worker, and `validate_reader` accepts any synchronous `Read` source. Both parse incrementally. `validate_bytes` uses a buffer that the caller already holds in memory. Memory use depends on retained samples, contract size, and the largest CSV record, including a JSON cell. A logical record is limited to 8 MiB by default, including embedded newlines; `.max_record_bytes(n)` can set a smaller limit or raise it to at most 64 MiB. Files with many bounded records can be much larger than memory.

## 0.2.0 behavior changes

- `.max_samples(0)` now retains no samples. The largest supported cap is 10,000; `violation_count` still counts every violation.
- Records over 8 MiB fail by default; callers may raise the limit to 64 MiB. Contracts with empty or duplicate fields, mismatched enum types, or unknown primary-key fields fail validation.
- Dates and date-times are checked for valid components; `NaN` and infinities are invalid numbers. String enums compare the CSV value without trimming meaningful spaces.
- Numeric enum comparisons use exact integer parsing for large integer literals. Floating enum values at or above 2^53 in magnitude fail closed because adjacent decimal integers can round to the same binary float.
- Diagnostics omit source cell values and unexpected header text. Consumers that displayed rejected values from `message` must obtain them from their own protected input path.

---

## Composes with

- **[data-contract-registry](https://github.com/mizcausevic-dev/data-contract-registry)** — the caller can fetch a specific contract version as JSON and pass it to `Contract::from_json`. This crate checks CSV shape and cells; it does not enforce registry ownership, freshness, status, or primary-key uniqueness.
- **[audit-stream-py](https://github.com/mizcausevic-dev/audit-stream-py)** — emit a `contract_compatibility_failed` event when validation lights up.
- **[reliability-toolkit-rs](https://github.com/mizcausevic-dev/reliability-toolkit-rs)** — wrap the registry-fetch call in a circuit breaker.

---

## Example

```bash
cargo run --example validate
```

Validates a tiny in-memory CSV against an in-memory contract and prints the report. Useful for kicking the tyres without setting up a registry.

To validate a saved full registry contract JSON document against a CSV file:

```bash
cargo run --example validate_contract_file -- tests/fixtures/registry-contract.json data.csv
```

The example prints JSON and exits `0` for a valid report, `2` for violations, or `1` for input and parse errors. Pass a saved contract version from the registry; the example does not fetch over the network. The repository's `tests/fixtures/registry-contract.json` is a synthetic exported contract for local experiments.

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

CI matrix: `stable`, `beta`, `1.86.0` (declared MSRV). Tests cover the happy path, violation kinds, bad headers and contracts, privacy-safe diagnostics, date and number boundaries, sample caps, chunked readers, and the async file path.

---

## License

MIT. See [LICENSE](LICENSE).
