#![forbid(unsafe_code)]

use std::collections::BTreeMap;

use serde_json::{Map, Number, Value};
use sha2::{Digest, Sha256};
use unicode_normalization::UnicodeNormalization;

use crate::domain::posting::LedgerPosting;

use super::error::{ApplicationError, ApplicationResult};

const MAX_SAFE_INTEGER: u64 = 9_007_199_254_740_991;

/// Hashes a canonical JSON v1 value after removing its top-level hash member.
///
/// # Errors
///
/// Returns an error when the value contains a float, negative/unsafe integer,
/// or keys that collide after Unicode NFC normalization.
pub fn canonical_hash(value: &Value) -> ApplicationResult<String> {
    let mut target = value.clone();
    if let Value::Object(object) = &mut target {
        object.remove("canonical_hash");
    }
    let bytes = canonical_bytes(&target)?;
    Ok(format!("sha256:{}", sha256_hex(&bytes)))
}

/// Serializes the supported canonical JSON v1 value domain.
///
/// # Errors
///
/// Returns an error for values outside the canonical JSON v1 value domain.
pub fn canonical_bytes(value: &Value) -> ApplicationResult<Vec<u8>> {
    let normalized = normalize(value)?;
    serde_json::to_vec(&normalized).map_err(|_| ApplicationError::TransactionFailed)
}

/// Sorts postings by the domain order and returns their canonical SHA-256.
///
/// # Errors
///
/// Returns an error when the posting payload cannot be represented as
/// canonical JSON v1.
pub fn canonical_postings_hash(postings: &[LedgerPosting]) -> ApplicationResult<String> {
    let mut ordered = postings.to_vec();
    ordered.sort_by(|left, right| {
        (
            &left.effective_date,
            left.sequence,
            left.event_id,
            left.posting_id,
        )
            .cmp(&(
                &right.effective_date,
                right.sequence,
                right.event_id,
                right.posting_id,
            ))
    });
    let values = ordered.iter().map(posting_value).collect();
    canonical_hash(&Value::Array(values))
}

fn posting_value(posting: &LedgerPosting) -> Value {
    let mut value = Map::new();
    value.insert(
        "posting_id".to_owned(),
        Value::String(posting.posting_id.to_string()),
    );
    value.insert(
        "event_id".to_owned(),
        Value::String(posting.event_id.to_string()),
    );
    value.insert(
        "posting_kind".to_owned(),
        Value::String(posting.posting_kind.as_str().to_owned()),
    );
    value.insert(
        "calculation_version".to_owned(),
        Value::String(posting.calculation_version.as_str().to_owned()),
    );
    if let Some(account_id) = posting.account_id {
        value.insert(
            "account_id".to_owned(),
            Value::String(account_id.to_string()),
        );
    }
    if let Some(portfolio_id) = posting.portfolio_id {
        value.insert(
            "portfolio_id".to_owned(),
            Value::String(portfolio_id.to_string()),
        );
    }
    if let Some(instrument_id) = posting.instrument_id {
        value.insert(
            "instrument_id".to_owned(),
            Value::String(instrument_id.to_string()),
        );
    }
    value.insert(
        "quantity_delta".to_owned(),
        Value::String(posting.quantity_delta.as_str().to_owned()),
    );
    value.insert(
        "currency".to_owned(),
        Value::String(posting.currency.as_str().to_owned()),
    );
    value.insert(
        "base_value".to_owned(),
        posting.base_value.as_ref().map_or(Value::Null, |amount| {
            Value::String(amount.as_str().to_owned())
        }),
    );
    value.insert(
        "base_currency".to_owned(),
        Value::String(posting.base_currency.as_str().to_owned()),
    );
    Value::Object(value)
}

fn normalize(value: &Value) -> ApplicationResult<Value> {
    match value {
        Value::Null | Value::Bool(_) | Value::String(_) => match value {
            Value::String(text) => Ok(Value::String(text.nfc().collect())),
            _ => Ok(value.clone()),
        },
        Value::Number(number) => validate_number(number),
        Value::Array(items) => items
            .iter()
            .map(normalize)
            .collect::<ApplicationResult<Vec<_>>>()
            .map(Value::Array),
        Value::Object(object) => {
            let mut sorted = BTreeMap::new();
            for (key, item) in object {
                let normalized_key: String = key.nfc().collect();
                if sorted.insert(normalized_key, normalize(item)?).is_some() {
                    return Err(ApplicationError::TransactionFailed);
                }
            }
            Ok(Value::Object(sorted.into_iter().collect()))
        }
    }
}

fn validate_number(number: &Number) -> ApplicationResult<Value> {
    number
        .as_u64()
        .filter(|value| *value <= MAX_SAFE_INTEGER)
        .map(|value| Value::Number(Number::from(value)))
        .ok_or(ApplicationError::TransactionFailed)
}

fn sha256_hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    Sha256::digest(bytes)
        .iter()
        .fold(String::with_capacity(64), |mut output, byte| {
            write!(output, "{byte:02x}").expect("writing to a String cannot fail");
            output
        })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{canonical_bytes, canonical_hash};

    #[test]
    fn canonical_json_normalizes_strings_sorts_keys_and_removes_only_top_hash() {
        let value = json!({
            "z": "e\u{301}",
            "a": {"canonical_hash": "kept"},
            "canonical_hash": "removed"
        });
        let bytes = canonical_bytes(&value).unwrap();
        assert_eq!(
            String::from_utf8(bytes).unwrap(),
            "{\"a\":{\"canonical_hash\":\"kept\"},\"canonical_hash\":\"removed\",\"z\":\"é\"}"
        );
        assert_eq!(
            canonical_hash(&json!({"b": 2, "a": 1})).unwrap(),
            "sha256:43258cff783fe7036d8a43033f830adfc60ec037382473548ac742b888292777"
        );
    }

    #[test]
    fn canonical_json_rejects_negative_float_and_unsafe_numbers() {
        for value in [json!(-1), json!(1.5), json!(9_007_199_254_740_992_u64)] {
            assert!(canonical_bytes(&value).is_err());
        }
    }
}
