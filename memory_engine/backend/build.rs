// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

fn main() {
    // sqlx::migrate! embeds migrations in the binary. Cargo does not otherwise
    // know that adding a SQL file must rebuild the migration and service bins.
    println!("cargo:rerun-if-changed=migrations/postgres");
}
