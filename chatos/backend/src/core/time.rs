// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

pub fn now_rfc3339() -> String {
    chrono::Utc::now().to_rfc3339()
}

#[cfg(test)]
mod tests {
    use super::now_rfc3339;

    #[test]
    fn generates_non_empty_timestamp() {
        assert!(!now_rfc3339().is_empty());
    }
}
