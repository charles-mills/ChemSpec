use std::fmt;

use serde_json::Value;
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CanonicalJsonError {
    FloatingPointNumber,
}

impl fmt::Display for CanonicalJsonError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::FloatingPointNumber => {
                formatter.write_str("canonical chemistry JSON forbids floating-point numbers")
            }
        }
    }
}

impl std::error::Error for CanonicalJsonError {}

/// Serializes JSON with sorted object keys, no insignificant whitespace, and
/// no binary floating-point numbers.
///
/// # Errors
///
/// Returns [`CanonicalJsonError::FloatingPointNumber`] if any nested value is a
/// JSON number that cannot be represented as an integer.
pub fn canonical_json(value: &Value) -> Result<Vec<u8>, CanonicalJsonError> {
    let mut output = String::new();
    write_canonical_json(value, &mut output)?;
    Ok(output.into_bytes())
}

fn write_canonical_json(value: &Value, output: &mut String) -> Result<(), CanonicalJsonError> {
    match value {
        Value::Null => output.push_str("null"),
        Value::Bool(value) => output.push_str(if *value { "true" } else { "false" }),
        Value::Number(number) => {
            if number.is_i64() || number.is_u64() {
                output.push_str(&number.to_string());
            } else {
                return Err(CanonicalJsonError::FloatingPointNumber);
            }
        }
        Value::String(value) => output.push_str(
            &serde_json::to_string(value).expect("serializing a JSON string cannot fail"),
        ),
        Value::Array(values) => {
            output.push('[');
            for (index, value) in values.iter().enumerate() {
                if index != 0 {
                    output.push(',');
                }
                write_canonical_json(value, output)?;
            }
            output.push(']');
        }
        Value::Object(values) => {
            output.push('{');
            let mut entries = values.iter().collect::<Vec<_>>();
            entries.sort_unstable_by_key(|(key, _)| *key);
            for (index, (key, value)) in entries.into_iter().enumerate() {
                if index != 0 {
                    output.push(',');
                }
                output.push_str(
                    &serde_json::to_string(key).expect("serializing a JSON key cannot fail"),
                );
                output.push(':');
                write_canonical_json(value, output)?;
            }
            output.push('}');
        }
    }
    Ok(())
}

#[must_use]
pub fn sha256(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}

#[must_use]
pub fn lowercase_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(char::from(HEX[usize::from(byte >> 4)]));
        output.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    output
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{CanonicalJsonError, canonical_json, lowercase_hex, sha256};

    #[test]
    fn canonical_json_sorts_keys_and_has_no_whitespace() {
        let value = json!({"z": [3, 2, 1], "a": {"b": true, "a": null}});
        assert_eq!(
            canonical_json(&value).expect("integer JSON is canonicalizable"),
            br#"{"a":{"a":null,"b":true},"z":[3,2,1]}"#
        );
    }

    #[test]
    fn canonical_json_rejects_binary_float_values() {
        assert_eq!(
            canonical_json(&json!({"amount": 0.1})),
            Err(CanonicalJsonError::FloatingPointNumber)
        );
    }

    #[test]
    fn sha256_matches_published_vectors() {
        assert_eq!(
            lowercase_hex(&sha256(b"")),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            lowercase_hex(&sha256(b"abc")),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}
