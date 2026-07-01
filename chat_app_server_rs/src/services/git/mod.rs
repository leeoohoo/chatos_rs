// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

pub mod contracts;

mod inspection;
mod parsing;
mod process;
mod query_ops;
mod shared;
mod validation;
mod write_ops;

pub use contracts::*;
pub use query_ops::{branches, client_info, compare, file_diff, status, summary};
pub use validation::discover_repo_root;
pub use write_ops::{
    checkout, commit, create_branch, discard, fetch, merge, pull, push, stage, unstage,
};
