use serde_json::Value;

use crate::core::validation::normalize_non_empty;

pub(in crate::api::projects) fn value_string(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .and_then(|value| normalize_non_empty(Some(value.to_string())))
}

pub(super) fn value_i64(value: &Value, key: &str) -> Option<i64> {
    value.get(key).and_then(Value::as_i64)
}

pub(super) fn value_string_vec(value: &Value, key: &str) -> Option<Vec<String>> {
    let items = value.get(key)?.as_array()?;
    Some(normalize_tags(
        items
            .iter()
            .filter_map(Value::as_str)
            .map(ToOwned::to_owned)
            .collect(),
    ))
}

pub(super) fn normalize_tags(values: Vec<String>) -> Vec<String> {
    let mut out = values
        .into_iter()
        .filter_map(|value| normalize_non_empty(Some(value)))
        .collect::<Vec<_>>();
    out.sort();
    out.dedup();
    out
}
