use super::{normalize_history_limit, normalize_history_offset};

#[test]
fn history_limit_defaults_and_clamps() {
    assert_eq!(normalize_history_limit(None), 1200);
    assert_eq!(normalize_history_limit(Some(0)), 1);
    assert_eq!(normalize_history_limit(Some(999_999)), 5000);
}

#[test]
fn history_offset_defaults_and_is_non_negative() {
    assert_eq!(normalize_history_offset(None), 0);
    assert_eq!(normalize_history_offset(Some(-10)), 0);
    assert_eq!(normalize_history_offset(Some(25)), 25);
}
