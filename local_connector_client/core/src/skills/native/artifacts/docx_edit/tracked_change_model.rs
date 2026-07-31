// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use anyhow::{anyhow, Result};

#[derive(Clone, Copy)]
pub(super) enum DocxTrackedRevisionAction {
    Accept,
    Reject,
}

#[derive(Clone, Copy)]
pub(super) enum DocxTrackedRevisionKind {
    Insertion,
    Deletion,
}

impl DocxTrackedRevisionKind {
    pub(super) fn closing(self) -> &'static str {
        match self {
            Self::Insertion => "</w:ins>",
            Self::Deletion => "</w:del>",
        }
    }

    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Insertion => "insertion",
            Self::Deletion => "deletion",
        }
    }

    pub(super) fn text_tag(self) -> &'static str {
        match self {
            Self::Insertion => "w:t",
            Self::Deletion => "w:delText",
        }
    }
}

impl DocxTrackedRevisionAction {
    pub(super) fn parse(value: &str) -> Result<Self> {
        match value {
            "accept" => Ok(Self::Accept),
            "reject" => Ok(Self::Reject),
            _ => Err(anyhow!(
                "action must be either accept or reject for DOCX tracked changes"
            )),
        }
    }

    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::Accept => "accept",
            Self::Reject => "reject",
        }
    }
}

pub(super) struct ResolvedTrackedRevisionStats {
    pub(super) insertions: usize,
    pub(super) deletions: usize,
    pub(super) resolved_revision_ids: Vec<u32>,
    pub(super) total_revisions: usize,
    pub(super) remaining_revisions: usize,
}

pub(super) struct SimpleTrackedRevision<'a> {
    pub(super) start: usize,
    pub(super) end: usize,
    pub(super) id: u32,
    pub(super) kind: DocxTrackedRevisionKind,
    pub(super) opening: &'a str,
    pub(super) content: &'a str,
}
