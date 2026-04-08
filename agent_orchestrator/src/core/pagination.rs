use serde_json::Value;

pub fn parse_positive_limit(raw: Option<String>) -> Option<i64> {
    let value = raw.and_then(|s| parse_js_int(&s));
    value.filter(|v| *v > 0)
}

pub fn parse_non_negative_offset(raw: Option<String>) -> i64 {
    match raw.and_then(|s| parse_js_int(&s)) {
        Some(v) if v > 0 => v,
        _ => 0,
    }
}

pub fn parse_js_int_value(value: &Value) -> Option<i64> {
    let s = match value {
        Value::String(v) => v.clone(),
        _ => value.to_string(),
    };
    parse_js_int(&s)
}

pub fn parse_js_int(input: &str) -> Option<i64> {
    let s = input.trim_start();
    if s.is_empty() {
        return None;
    }

    let mut chars = s.chars().peekable();
    let mut sign: i128 = 1;
    if let Some(&c) = chars.peek() {
        if c == '+' || c == '-' {
            if c == '-' {
                sign = -1;
            }
            chars.next();
        }
    }

    let mut value: i128 = 0;
    let mut any = false;
    for c in chars {
        match c.to_digit(10) {
            Some(d) => {
                any = true;
                value = value.saturating_mul(10).saturating_add(d as i128);
                if value > i64::MAX as i128 {
                    value = i64::MAX as i128;
                    break;
                }
            }
            None => break,
        }
    }

    if !any {
        return None;
    }

    let signed = value.saturating_mul(sign);
    if signed > i64::MAX as i128 {
        Some(i64::MAX)
    } else if signed < i64::MIN as i128 {
        Some(i64::MIN)
    } else {
        Some(signed as i64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_js_integer_prefix() {
        assert_eq!(parse_js_int("  -12abc"), Some(-12));
        assert_eq!(parse_js_int("+34"), Some(34));
        assert_eq!(parse_js_int("abc"), None);
    }

    #[test]
    fn clamps_limits_and_offsets() {
        assert_eq!(parse_positive_limit(Some("100".to_string())), Some(100));
        assert_eq!(parse_positive_limit(Some("-10".to_string())), None);
        assert_eq!(parse_non_negative_offset(Some("-1".to_string())), 0);
        assert_eq!(parse_non_negative_offset(Some("20".to_string())), 20);
    }

    #[test]
    fn parses_json_values() {
        assert_eq!(parse_js_int_value(&json!("42px")), Some(42));
        assert_eq!(parse_js_int_value(&json!(123.5)), Some(123));
    }
}
