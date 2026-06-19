use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InternalContextLocale {
    ZhCn,
    EnUs,
}

impl InternalContextLocale {
    pub const DEFAULT_KEY: &'static str = "zh-CN";
    pub const ENGLISH_KEY: &'static str = "en-US";

    pub fn is_english(self) -> bool {
        matches!(self, Self::EnUs)
    }
}

pub fn parse_internal_context_locale(value: Option<&str>) -> InternalContextLocale {
    match value
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .unwrap_or(InternalContextLocale::DEFAULT_KEY)
    {
        InternalContextLocale::ENGLISH_KEY => InternalContextLocale::EnUs,
        _ => InternalContextLocale::ZhCn,
    }
}

pub fn internal_context_locale_from_settings(settings: &Value) -> InternalContextLocale {
    parse_internal_context_locale(
        settings
            .get("INTERNAL_CONTEXT_LOCALE")
            .and_then(Value::as_str),
    )
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{
        InternalContextLocale, internal_context_locale_from_settings, parse_internal_context_locale,
    };

    #[test]
    fn defaults_to_zh_cn_for_missing_or_invalid_values() {
        assert_eq!(
            parse_internal_context_locale(None),
            InternalContextLocale::ZhCn
        );
        assert_eq!(
            parse_internal_context_locale(Some("")),
            InternalContextLocale::ZhCn
        );
        assert_eq!(
            parse_internal_context_locale(Some("fr-FR")),
            InternalContextLocale::ZhCn
        );
    }

    #[test]
    fn accepts_en_us_value() {
        assert_eq!(
            parse_internal_context_locale(Some("en-US")),
            InternalContextLocale::EnUs
        );
    }

    #[test]
    fn reads_from_user_settings_payload() {
        assert_eq!(
            internal_context_locale_from_settings(&json!({"INTERNAL_CONTEXT_LOCALE": "en-US"})),
            InternalContextLocale::EnUs
        );
    }
}
