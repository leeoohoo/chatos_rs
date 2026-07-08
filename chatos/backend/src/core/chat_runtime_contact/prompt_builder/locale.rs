// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::core::internal_context_locale::InternalContextLocale;

pub(super) fn text(locale: InternalContextLocale, zh: &'static str, en: &'static str) -> String {
    if locale.is_english() {
        en.to_string()
    } else {
        zh.to_string()
    }
}

#[cfg(test)]
pub(super) fn text_ref(
    locale: InternalContextLocale,
    zh: &'static str,
    en: &'static str,
) -> &'static str {
    if locale.is_english() {
        en
    } else {
        zh
    }
}

#[cfg(test)]
pub(super) fn field(
    locale: InternalContextLocale,
    zh: &'static str,
    en: &'static str,
) -> &'static str {
    if locale.is_english() {
        en
    } else {
        zh
    }
}
