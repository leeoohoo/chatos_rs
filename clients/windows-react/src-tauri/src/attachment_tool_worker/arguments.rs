use super::*;

pub(super) fn invalid_todo_planning_arguments() -> String {
    "The Todo planning arguments are invalid.".into()
}

pub(super) fn optional_planning_text(
    arguments: &Map<String, Value>,
    field: &str,
    maximum: usize,
    allow_empty: bool,
) -> Result<Option<String>, String> {
    arguments
        .get(field)
        .map(|value| {
            value
                .as_str()
                .filter(|value| {
                    (allow_empty || !value.is_empty())
                        && value.trim() == *value
                        && value.chars().count() <= maximum
                        && !value.contains('\0')
                })
                .map(str::to_owned)
                .ok_or_else(invalid_todo_planning_arguments)
        })
        .transpose()
}

pub(super) fn optional_planning_integer(
    arguments: &Map<String, Value>,
    field: &str,
    minimum: i64,
    maximum: i64,
) -> Result<Option<i64>, String> {
    arguments
        .get(field)
        .map(|value| {
            value
                .as_i64()
                .filter(|value| (minimum..=maximum).contains(value))
                .ok_or_else(invalid_todo_planning_arguments)
        })
        .transpose()
}

pub(super) fn planning_string_array(
    arguments: &Map<String, Value>,
    field: &str,
    minimum_items: usize,
    maximum_items: usize,
    maximum_characters: usize,
) -> Result<Option<Vec<String>>, String> {
    arguments
        .get(field)
        .map(|value| {
            let values = value
                .as_array()
                .filter(|values| (minimum_items..=maximum_items).contains(&values.len()))
                .ok_or_else(invalid_todo_planning_arguments)?;
            let mut unique = std::collections::HashSet::new();
            values
                .iter()
                .map(|value| {
                    let value = value
                        .as_str()
                        .filter(|value| {
                            !value.is_empty()
                                && value.trim() == *value
                                && value.chars().count() <= maximum_characters
                                && !value.contains('\0')
                        })
                        .ok_or_else(invalid_todo_planning_arguments)?;
                    if !unique.insert(value) {
                        return Err(invalid_todo_planning_arguments());
                    }
                    Ok(value.to_owned())
                })
                .collect()
        })
        .transpose()
}

pub(super) fn required_bounded_string(
    arguments: &Map<String, Value>,
    field: &str,
    maximum: usize,
) -> Result<String, String> {
    arguments
        .get(field)
        .and_then(Value::as_str)
        .filter(|value| {
            !value.is_empty()
                && value.trim() == *value
                && value.chars().count() <= maximum
                && !value.chars().any(|character| character == '\0')
        })
        .map(str::to_owned)
        .ok_or_else(|| "The Todo execution arguments are invalid.".to_owned())
}

pub(super) fn optional_bounded_string(
    arguments: &Map<String, Value>,
    field: &str,
    maximum: usize,
) -> Result<Option<String>, String> {
    arguments
        .get(field)
        .map(|_| required_bounded_string(arguments, field, maximum))
        .transpose()
}
