//! Typed access to wire props and renderer-side sanitizing (SPEC §10.8 item 1).
//!
//! Props arrive as §6.5 JSON: names are keys, integers and floats are typed
//! (`{"i":2}`, `{"f":2.5}`), maps are ordered tagged pairs (`{"map":[...]}`),
//! tokens are `{"tok":["space",3]}`, bindings `{"bind":"$name"}` and actions
//! `{"action":"..."}`. This module never panics on model-controlled values.

use serde_json::Value;
use std::collections::BTreeMap;

/// SPEC §10.8: each text prop is truncated to 32 KiB before drawing.
pub const MAX_TEXT_BYTES: usize = 32 * 1024;
/// SPEC §10.8: list lengths are capped before drawing.
pub const MAX_LIST_ITEMS: usize = 256;

pub type Props = BTreeMap<String, Value>;

/// Removes bidi and zero-width characters and truncates at a char boundary.
pub fn sanitize_text(text: &str) -> String {
    let filtered: String = text
        .chars()
        .filter(|ch| {
            !matches!(
                *ch,
                '\u{202a}'..='\u{202e}'
                    | '\u{2066}'..='\u{2069}'
                    | '\u{200b}'..='\u{200d}'
                    | '\u{2060}'
                    | '\u{feff}'
            )
        })
        .collect();
    truncate(&filtered, MAX_TEXT_BYTES)
}

fn truncate(text: &str, max_bytes: usize) -> String {
    if text.len() <= max_bytes {
        return text.to_string();
    }
    let mut end = max_bytes;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    text[..end].to_string()
}

/// The raw string value of a text prop, sanitized.
pub fn text(props: &Props, key: &str) -> Option<String> {
    props.get(key).and_then(Value::as_str).map(sanitize_text)
}

/// A text prop, or `fallback` when absent or not a string.
pub fn text_or(props: &Props, key: &str, fallback: &str) -> String {
    text(props, key).unwrap_or_else(|| fallback.to_string())
}

pub fn bool_(props: &Props, key: &str, fallback: bool) -> bool {
    props.get(key).and_then(Value::as_bool).unwrap_or(fallback)
}

pub fn int(props: &Props, key: &str) -> Option<i64> {
    let value = props.get(key)?;
    match value {
        Value::Number(number) => number.as_i64(),
        Value::Object(map) => map.get("i").and_then(Value::as_i64),
        _ => None,
    }
}

pub fn number(props: &Props, key: &str) -> Option<f64> {
    let value = props.get(key)?;
    match value {
        Value::Number(number) => number.as_f64(),
        Value::Object(map) => map
            .get("f")
            .and_then(Value::as_f64)
            .or_else(|| map.get("i").and_then(Value::as_i64).map(|v| v as f64)),
        _ => None,
    }
}

/// A token prop's step, clamped to the space scale's 0–8 range.
pub fn token_step(props: &Props, key: &str, fallback: u8) -> u8 {
    let Some(Value::Object(map)) = props.get(key) else {
        return fallback;
    };
    let Some(Value::Array(parts)) = map.get("tok") else {
        return fallback;
    };
    let step = match parts.get(1) {
        Some(Value::Number(number)) => number.as_u64().unwrap_or(u64::from(fallback)),
        Some(Value::Object(inner)) => inner
            .get("i")
            .and_then(Value::as_i64)
            .and_then(|value| u64::try_from(value).ok())
            .unwrap_or(u64::from(fallback)),
        _ => u64::from(fallback),
    };
    u8::try_from(step.min(8)).unwrap_or(8)
}

/// An enum prop name, sanitized and bounded.
pub fn name_or(props: &Props, key: &str, fallback: &str) -> String {
    text(props, key).unwrap_or_else(|| fallback.to_string())
}

/// A list of ordered map entries, as used by `Facts`, `Chart` and data props.
pub fn data(props: &Props, key: &str) -> Vec<Vec<(String, Value)>> {
    let Some(Value::Array(items)) = props.get(key) else {
        return Vec::new();
    };
    items
        .iter()
        .take(MAX_LIST_ITEMS)
        .filter_map(decode_map)
        .collect()
}

/// Decodes one wire map (`{"map":[[key, value], ...]}`).
pub fn decode_map(value: &Value) -> Option<Vec<(String, Value)>> {
    let Value::Object(object) = value else {
        return None;
    };
    let Value::Array(entries) = object.get("map")? else {
        return None;
    };
    Some(
        entries
            .iter()
            .filter_map(|entry| {
                let Value::Array(pair) = entry else {
                    return None;
                };
                let key = pair.first()?.as_str()?.to_string();
                let value = pair.get(1)?.clone();
                Some((key, value))
            })
            .collect(),
    )
}

pub fn map_get<'a>(entries: &'a [(String, Value)], key: &str) -> Option<&'a Value> {
    entries
        .iter()
        .find(|(name, _)| name == key)
        .map(|(_, value)| value)
}

/// One `options` entry: the visible label and the committed value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OptionItem {
    pub label: String,
    pub value: String,
}

/// A list of options (`Select`, `CheckboxGroup`, `RadioGroup`). Strings are
/// both label and value; `{label, value}` maps use `value` when present.
pub fn options(props: &Props, key: &str) -> Vec<OptionItem> {
    let Some(Value::Array(items)) = props.get(key) else {
        return Vec::new();
    };
    items
        .iter()
        .take(MAX_LIST_ITEMS)
        .filter_map(|item| match item {
            Value::String(text) => {
                let text = sanitize_text(text);
                Some(OptionItem {
                    label: text.clone(),
                    value: text,
                })
            }
            Value::Object(map) => {
                let entries = map.get("map")?.as_array()?;
                let mut label = None;
                let mut value = None;
                for entry in entries {
                    let pair = entry.as_array()?;
                    let name = pair.first()?.as_str()?;
                    let inner = pair.get(1)?;
                    match name {
                        "label" => label = inner.as_str().map(sanitize_text),
                        "value" => value = inner.as_str().map(sanitize_text),
                        _ => {}
                    }
                }
                let label = label?;
                Some(OptionItem {
                    value: value.unwrap_or_else(|| label.clone()),
                    label,
                })
            }
            _ => None,
        })
        .collect()
}

/// A list of string labels (`Select`, `RadioGroup` and similar).
pub fn strings(props: &Props, key: &str) -> Vec<String> {
    options(props, key)
        .into_iter()
        .map(|item| item.label)
        .collect()
}

/// The plan handle of an `action` prop, when present.
pub fn action(props: &Props, key: &str) -> Option<String> {
    let Value::Object(map) = props.get(key)? else {
        return None;
    };
    map.get("action")?.as_str().map(str::to_string)
}

/// The state name of a `bind` prop, without its leading `$`.
pub fn binding(props: &Props, key: &str) -> Option<String> {
    let Value::Object(map) = props.get(key)? else {
        return None;
    };
    let bind = map.get("bind")?.as_str()?;
    let name = bind.strip_prefix('$').unwrap_or(bind);
    (!name.is_empty()).then(|| name.to_string())
}

/// A wire scalar as an editable string. Numbers use their plain form.
pub fn scalar_text(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => Some(sanitize_text(text)),
        Value::Bool(value) => Some(value.to_string()),
        Value::Object(map) => map
            .get("i")
            .and_then(Value::as_i64)
            .map(|value| value.to_string())
            .or_else(|| {
                map.get("f")
                    .and_then(Value::as_f64)
                    .map(|value| format!("{value}"))
            }),
        Value::Number(number) => Some(number.to_string()),
        _ => None,
    }
}

/// The synthetic `status` wire prop or a nested `{"status":...}` value
/// (`{"status":"loading"}`), when present.
pub fn data_status(props: &Props, key: &str) -> Option<String> {
    if let Some(status) = props.get("status").and_then(Value::as_str) {
        return Some(status.to_string());
    }
    let Value::Object(map) = props.get(key)? else {
        return None;
    };
    map.get("status")
        .and_then(Value::as_str)
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn text_is_sanitized_and_truncated_on_a_char_boundary() {
        assert_eq!(sanitize_text("a\u{200b}b\u{202e}c"), "abc");
        let long = "é".repeat(MAX_TEXT_BYTES);
        let clipped = sanitize_text(&long);
        assert!(clipped.len() <= MAX_TEXT_BYTES);
        assert!(clipped.is_char_boundary(clipped.len()));
        assert_eq!(clipped.chars().count(), MAX_TEXT_BYTES / 2);
    }

    #[test]
    fn typed_numbers_tokens_and_names_decode_without_panicking() {
        let props: Props = serde_json::from_value(json!({
            "value": {"i": 42},
            "ratio": {"f": 2.5},
            "gap": {"tok": ["space", 3]},
            "bad_gap": {"tok": ["space", {"i": 20}]},
            "size": "lg",
            "flag": true,
            "action": {"action": "root|arg:on_click"},
            "bind": {"bind": "$name"},
            "nested": {"map": [["key", {"i": 1}]]}
        }))
        .expect("props");
        assert_eq!(int(&props, "value"), Some(42));
        assert_eq!(number(&props, "ratio"), Some(2.5));
        assert_eq!(number(&props, "value"), Some(42.0));
        assert_eq!(token_step(&props, "gap", 2), 3);
        assert_eq!(token_step(&props, "bad_gap", 2), 8);
        assert_eq!(token_step(&props, "missing", 2), 2);
        assert_eq!(name_or(&props, "size", "md"), "lg");
        assert!(bool_(&props, "flag", false));
        assert_eq!(int(&props, "size"), None);
        assert_eq!(int(&props, "nested"), None);
    }

    #[test]
    fn ordered_data_maps_and_option_strings_decode_in_order() {
        let props: Props = serde_json::from_value(json!({
            "items": [
                {"map": [["label", "Plan"], ["value", "Team"]]},
                {"map": [["label", "Renewal"], ["value", "November 1"]]}
            ],
            "options": ["North", "South"],
            "odd": [1, {"not": "a map"}, {"map": [["label", "Ignored"]]}]
        }))
        .expect("props");
        let items = data(&props, "items");
        assert_eq!(items.len(), 2);
        assert_eq!(map_get(&items[0], "label"), Some(&json!("Plan")));
        assert_eq!(map_get(&items[1], "value"), Some(&json!("November 1")));
        assert_eq!(strings(&props, "options"), vec!["North", "South"]);
        assert_eq!(data(&props, "odd").len(), 1);
        assert!(data(&props, "missing").is_empty());
    }

    #[test]
    fn data_lists_are_capped() {
        let entries: Vec<Value> = (0..MAX_LIST_ITEMS + 20)
            .map(|index| json!({"map": [["label", index.to_string()]]}))
            .collect();
        let props: Props = serde_json::from_value(json!({ "items": entries })).expect("props");
        assert_eq!(data(&props, "items").len(), MAX_LIST_ITEMS);
    }
}
