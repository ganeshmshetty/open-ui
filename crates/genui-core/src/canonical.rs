//! Canonical JSON byte contract (§6.5), independent of serde_json object storage.
use serde::Serialize;
use serde_json::Value;
use std::fmt::Write;

/// Sorted UTF-8 object keys, compact structure, ordered arrays and finite JSON numbers.
pub fn encode(value: &Value) -> String {
    let mut out = String::new();
    write_value(value, &mut out);
    out
}
pub fn serialize<T: Serialize + ?Sized>(value: &T) -> Result<String, serde_json::Error> {
    serde_json::to_value(value).map(|v| encode(&v))
}

/// xxh3-128 of UTF-8 bytes as 32 lowercase hexadecimal digits (SPEC §§8.2,9.1).
pub fn digest(text: &str) -> String {
    format!("{:032x}", xxhash_rust::xxh3::xxh3_128(text.as_bytes()))
}
fn string(value: &str, out: &mut String) {
    out.push('"');
    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{0008}' => out.push_str("\\b"),
            '\u{000c}' => out.push_str("\\f"),
            ch if ch < '\u{0020}' => {
                let _ = write!(out, "\\u{:04x}", ch as u32);
            }
            ch => out.push(ch),
        }
    }
    out.push('"');
}
fn write_value(value: &Value, out: &mut String) {
    match value {
        Value::Null => out.push_str("null"),
        Value::Bool(v) => out.push_str(if *v { "true" } else { "false" }),
        // Number owns finite JSON numeric spelling; no object serialization is delegated.
        Value::Number(v) => out.push_str(&v.to_string()),
        Value::String(v) => string(v, out),
        Value::Array(items) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write_value(item, out);
            }
            out.push(']');
        }
        Value::Object(items) => {
            out.push('{');
            let mut keys: Vec<_> = items.keys().collect();
            keys.sort_unstable_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
            for (i, key) in keys.into_iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                string(key, out);
                out.push(':');
                write_value(&items[key], out);
            }
            out.push('}');
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Map, Value, json};
    #[test]
    fn byte_vectors_sort_every_object_without_changing_array_order() {
        let mut object = Map::new();
        object.insert("z".into(), json!([3, 2, 1]));
        object.insert(
            "a".into(),
            json!({"β": 1.0, "é": -0.0, "b": "\n\t\r\u{0000}\"\\😀"}),
        );
        assert_eq!(
            encode(&Value::Object(object)),
            "{\"a\":{\"b\":\"\\n\\t\\r\\u0000\\\"\\\\😀\",\"é\":-0.0,\"β\":1.0},\"z\":[3,2,1]}"
        );
    }
    #[test]
    fn every_json_control_escape_has_fixed_bytes() {
        let value = serde_json::json!("\u{0008}\u{000c}\u{001f}\u{0001}/");
        assert_eq!(encode(&value), r#""\b\f\u001f\u0001/""#);
    }
    #[test]
    fn reordered_objects_have_identical_bytes() {
        let a: Value = serde_json::from_str(r#"{"z":0,"a":{"y":2,"x":1}}"#).unwrap();
        let b: Value = serde_json::from_str(r#"{"a":{"x":1,"y":2},"z":0}"#).unwrap();
        assert_eq!(encode(&a), encode(&b));
        assert_eq!(encode(&a), r#"{"a":{"x":1,"y":2},"z":0}"#);
    }
}
