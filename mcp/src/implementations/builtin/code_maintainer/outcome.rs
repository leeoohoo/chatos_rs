// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileModificationOutcome {
    Changed,
    AlreadyApplied,
    StaleContext,
    ExpectedMatch,
    Validation,
    Infrastructure,
}

impl FileModificationOutcome {
    pub const fn from_changed(changed: bool) -> Self {
        if changed {
            Self::Changed
        } else {
            Self::AlreadyApplied
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Changed => "changed",
            Self::AlreadyApplied => "already_applied",
            Self::StaleContext => "stale_context",
            Self::ExpectedMatch => "expected_match",
            Self::Validation => "validation",
            Self::Infrastructure => "infrastructure",
        }
    }

    pub const fn is_success(self) -> bool {
        matches!(self, Self::Changed | Self::AlreadyApplied)
    }
}

pub fn classify_file_modification_error(error: &str) -> FileModificationOutcome {
    let normalized = error.to_ascii_lowercase();
    if [
        "old_text not found in file",
        "patch context not found in file",
        "target not found for replace",
        "stale_context",
        "file revision does not match",
        "does not match the active session baseline",
    ]
    .iter()
    .any(|marker| normalized.contains(marker))
    {
        return FileModificationOutcome::StaleContext;
    }

    if [
        "expected_matches mismatch",
        "no match satisfied line/context filters",
        "candidate matches",
        "expected_match",
    ]
    .iter()
    .any(|marker| normalized.contains(marker))
    {
        return FileModificationOutcome::ExpectedMatch;
    }

    if [
        " is required",
        "cannot be empty",
        "cannot be greater",
        "writes are disabled",
        "write exceeds",
        "write limit",
        "too large",
        "exceeds write limit",
        "patch does not contain",
        "failed to parse patch",
        "fallback parse failed",
        "move target already exists",
        "multiple conflicting actions",
        "outside workspace",
        "path traversal",
        "target is a directory",
    ]
    .iter()
    .any(|marker| normalized.contains(marker))
    {
        return FileModificationOutcome::Validation;
    }

    FileModificationOutcome::Infrastructure
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn success_outcomes_are_stable() {
        assert_eq!(
            FileModificationOutcome::from_changed(true).as_str(),
            "changed"
        );
        assert_eq!(
            FileModificationOutcome::from_changed(false).as_str(),
            "already_applied"
        );
    }

    #[test]
    fn stale_context_errors_share_one_classification_contract() {
        for error in [
            "old_text not found in file.",
            "Patch context not found in file.",
            "expected_sha256 for src/app/App.test.tsx does not match the active session baseline",
        ] {
            assert_eq!(
                classify_file_modification_error(error),
                FileModificationOutcome::StaleContext
            );
        }
    }

    #[test]
    fn expected_match_errors_are_distinct_from_stale_context() {
        for error in [
            "expected_matches mismatch: expected 1, got 0",
            "Found 2 candidate matches at line(s): 1, 4",
        ] {
            assert_eq!(
                classify_file_modification_error(error),
                FileModificationOutcome::ExpectedMatch
            );
        }
    }

    #[test]
    fn validation_and_infrastructure_errors_are_distinct() {
        assert_eq!(
            classify_file_modification_error("patch is required"),
            FileModificationOutcome::Validation
        );
        assert_eq!(
            classify_file_modification_error("commit request failed: connection reset"),
            FileModificationOutcome::Infrastructure
        );
    }
}
