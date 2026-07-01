// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::operator_auth;

#[test]
fn constant_time_equal_accepts_same_token() {
    assert!(operator_auth::constant_time_equal("abc", "abc"));
}

#[test]
fn constant_time_equal_rejects_different_token() {
    assert!(!operator_auth::constant_time_equal("abc", "abd"));
}
