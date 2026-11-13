// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

/// Conservative fallback used when a model tokenizer is not available.
/// UTF-8 bytes keep CJK text from being undercounted.
pub const CONSERVATIVE_TOKEN_ESTIMATOR_ID: &str = "utf8-bytes-div3-conservative-v2";

pub fn estimate_text_tokens(text: &str) -> usize {
    estimate_utf8_bytes_tokens(text.len())
}

pub fn estimate_utf8_bytes_tokens(bytes: usize) -> usize {
    bytes.saturating_add(2) / 3
}

#[cfg(test)]
mod tests {
    use super::estimate_text_tokens;

    #[test]
    fn estimator_counts_cjk_by_utf8_bytes() {
        assert_eq!(estimate_text_tokens("abcdefgh"), 3);
        assert_eq!(estimate_text_tokens("你好世界"), 4);
        assert_eq!(estimate_text_tokens("你好 hello"), 4);
    }
}
