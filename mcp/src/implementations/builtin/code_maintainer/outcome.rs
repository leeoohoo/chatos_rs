// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileModificationOutcome {
    Changed,
    AlreadyApplied,
    Stale,
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
            Self::Stale => "stale",
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
        "expected_matches mismatch",
        "patch context not found in file",
        "no match satisfied line/context filters",
        "target not found for replace",
    ]
    .iter()
    .any(|marker| normalized.contains(marker))
    {
        return FileModificationOutcome::Stale;
    }

    if [
        " is required",
        "cannot be empty",
        "cannot be greater",
        "candidate matches",
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
    fn stale_errors_share_one_classification_contract() {
        for error in [
            "old_text not found in file.",
            "expected_matches mismatch: expected 1, got 0",
            "Patch context not found in file.",
        ] {
            assert_eq!(
                classify_file_modification_error(error),
                FileModificationOutcome::Stale
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
