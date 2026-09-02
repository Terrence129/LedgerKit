use std::collections::BTreeMap;

use serde_json::Value;
use sha2::{Digest, Sha256};
use unicode_normalization::UnicodeNormalization;

use crate::error::SpikeResult;

pub fn canonical_hash(value: &Value) -> SpikeResult<String> {
    let mut hash_target = value.clone();
    if let Value::Object(object) = &mut hash_target {
        object.remove("canonical_hash");
    }
    let canonical = canonical_bytes(&hash_target)?;
    Ok(sha256_prefixed(&canonical))
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

pub fn sha256_prefixed(bytes: &[u8]) -> String {
    format!("sha256:{}", sha256_hex(bytes))
}

pub fn canonical_bytes(value: &Value) -> SpikeResult<Vec<u8>> {
    let normalized = normalize(value);
    Ok(serde_json::to_vec(&normalized)?)
}

fn normalize(value: &Value) -> Value {
    match value {
        Value::String(text) => Value::String(text.nfc().collect()),
        Value::Array(items) => Value::Array(items.iter().map(normalize).collect()),
        Value::Object(object) => {
            let sorted: BTreeMap<String, Value> = object
                .iter()
                .map(|(key, item)| (key.nfc().collect(), normalize(item)))
                .collect();
            Value::Object(sorted.into_iter().collect())
        }
        _ => value.clone(),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn hashes_sorted_keys_without_whitespace() {
        let value = json!({"b": 2, "a": 1});
        assert_eq!(canonical_bytes(&value).unwrap(), br#"{"a":1,"b":2}"#);
    }
}
