// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::env;
use std::fs;
use std::path::Path;

use chatos_plugin_management_sdk::{
    normalized_plugin_manifest_sha256, parse_plugin_manifest, plugin_manifest_source_from_path,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = env::args()
        .nth(1)
        .ok_or("usage: normalized-plugin-manifest-sha256 <manifest-path>")?;
    let source = plugin_manifest_source_from_path(Path::new(path.as_str())).ok_or(
        "manifest path must end in .chatos-plugin/plugin.json or .codex-plugin/plugin.json",
    )?;
    let raw = fs::read_to_string(path.as_str())?;
    let manifest = parse_plugin_manifest(raw.as_str(), source)?;
    println!("{}", normalized_plugin_manifest_sha256(&manifest)?);
    Ok(())
}
