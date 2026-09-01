# LedgerKit Canonical JSON v1

`ledgerkit-canonical-json-v1` is the stack-neutral serialization used by M0 golden fixture hashes. It is intentionally narrower than general JSON so Rust/Tauri and C#/Avalonia can produce identical bytes without using binary floating point.

## Value domain

- Financial values are ADR-0004 decimal strings. Exponent notation, localized separators, leading `+`, meaningless integer leading zeroes and negative zero are forbidden. Meaningful trailing zeroes preserve source scale.
- JSON numbers are limited to non-negative safe integers used for counts, sequence, watermarks, case numbers and schema versions.
- Strings are normalized to Unicode NFC before hashing.
- Object keys are sorted by Unicode scalar value. Arrays retain their contract-defined order; producers must apply domain ordering before serialization.

## Bytes and hash

1. Remove the `canonical_hash` member from the object being hashed. Nested objects with their own `canonical_hash` keep those values when their parent is hashed.
2. Serialize `null`, booleans, safe integers, normalized strings, arrays and sorted-key objects without insignificant whitespace.
3. Encode the result as UTF-8 without BOM or a trailing newline.
4. Compute SHA-256 and render `sha256:` followed by 64 lowercase hexadecimal characters.

Posting arrays are already ordered by `(effective_date, sequence, event_id, posting_id)`. Expense bucket arrays are ordered by `amount DESC, bucket_id ASC`; Top 10 uses the first ten positive buckets and `system:top10-other` for the remainder. The hash therefore never relies on SQLite physical row order or import batch order.

## Expense query payload

`expense-analysis-query-result/v1` includes resolved dates, base currency, valued totals, global and per-bucket distinct counts, complete buckets, Top 10, refund/reimbursement summaries, unvalued counts, event/master-data watermarks, all policy/calculation versions and the canonical hash. Drilldown fields contain only bounded filter context; event ID arrays are forbidden.

Consumers must validate the JSON Schema first, then enforce the semantic checks in `tools/validate-m0-fixtures.mjs`. A candidate stack passes M0 compatibility only when it reproduces every expected value and hash; reformatting the checked-in files is not a substitute for calculation.
