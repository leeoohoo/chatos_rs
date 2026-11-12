// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

pub(crate) fn is_selectable_model(name: &str) -> bool {
    !matches!(
        name.trim().to_ascii_lowercase().as_str(),
        "gpt-5.3-codex-spark" | "codex-auto-review"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn removed_aliases_are_excluded_independently_of_capacity_configuration() {
        for model in [
            "gpt-5.3-codex-spark",
            "codex-auto-review",
            " CODEX-AUTO-REVIEW ",
        ] {
            assert!(!is_selectable_model(model));
        }
        for model in [
            "gpt-6.1-sol",
            "deepseek-v4-pro",
            "gpt-image-2",
            "MiniMax-H3",
        ] {
            assert!(is_selectable_model(model));
        }
    }
}
