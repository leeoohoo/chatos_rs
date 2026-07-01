// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{HashMap, HashSet};
use std::path::Path;

use crate::services::code_nav::fallback::extract_token_at_position;
use crate::services::code_nav::languages::shared_nav::{
    impl_nav_symbol_like_for_field_struct, indexed_symbols_from, is_type_like,
    nav_location_from_coordinates, push_current_file_symbol_definitions,
    push_definition_search_matches, push_indexed_definition_candidates,
    search_occurrences_with_fallback, select_reference_locations, sort_and_truncate_nav_locations,
    HeuristicNavLanguage,
};
use crate::services::code_nav::symbol_index::project_symbol_index;
use crate::services::code_nav::types::{NavLocation, NavPositionRequest, ProjectContext};

mod analysis;
mod search;

use analysis::{analyze_rust_file, is_rust_declaration_location, resolve_rust_declaration_kind};
use search::{search_rust_occurrences, RustSearchMatch};

const RUST_IGNORED_DIRS: &[&str] = &[
    ".git",
    "node_modules",
    "dist",
    "build",
    "target",
    "out",
    ".idea",
];

const RUST_EXTENSIONS: &[&str] = &["rs"];
const MAX_DEFINITION_RESULTS: usize = 20;
const MAX_REFERENCE_RESULTS: usize = 100;
const MAX_SYMBOL_RESULTS: usize = 200;

#[derive(Debug, Clone)]
pub(crate) struct RustSymbol {
    name: String,
    kind: String,
    line: usize,
    column: usize,
    end_line: usize,
    end_column: usize,
}

impl_nav_symbol_like_for_field_struct!(RustSymbol);

fn indexed_rust_symbols(
    path: &Path,
) -> Result<Vec<crate::services::code_nav::symbol_index::IndexedSymbol>, String> {
    let analysis = analyze_rust_file(path)?;
    Ok(indexed_symbols_from(&analysis.symbols))
}

#[derive(Default)]
pub struct RustCodeNavProvider;

impl HeuristicNavLanguage for RustCodeNavProvider {
    type Symbol = RustSymbol;

    const PROVIDER_ID: &'static str = "rust";
    const LANGUAGE_ID: &'static str = "rust";
    const FILE_EXTENSION: &'static str = "rs";
    const MAX_SYMBOL_RESULTS: usize = self::MAX_SYMBOL_RESULTS;

    fn detect_project(ctx: &ProjectContext) -> bool {
        ctx.root.join("Cargo.toml").exists()
    }

    fn definition(
        ctx: &ProjectContext,
        req: &NavPositionRequest,
    ) -> Result<Vec<NavLocation>, String> {
        rust_definition(ctx, req)
    }

    fn references(
        ctx: &ProjectContext,
        req: &NavPositionRequest,
    ) -> Result<Vec<NavLocation>, String> {
        rust_references(ctx, req)
    }

    fn analyze_document_symbols(file_path: &Path) -> Result<Vec<Self::Symbol>, String> {
        Ok(analyze_rust_file(file_path)?.symbols)
    }
}

fn rust_definition(
    ctx: &ProjectContext,
    req: &NavPositionRequest,
) -> Result<Vec<NavLocation>, String> {
    let token = extract_token_at_position(&ctx.file_path, req.line, req.column)?;
    let Some(token) = token else {
        return Ok(Vec::new());
    };

    let current = analyze_rust_file(&ctx.file_path)?;
    let mut candidates = Vec::new();
    let mut seen = HashSet::new();

    push_current_file_symbol_definitions(
        &ctx.root,
        &ctx.file_path,
        &current.symbols,
        &token,
        req.line,
        9.0,
        nav_location_from_symbol,
        &mut candidates,
        &mut seen,
    )?;

    if let Ok(index) = project_symbol_index(
        &ctx.root,
        "rust",
        RUST_EXTENSIONS,
        RUST_IGNORED_DIRS,
        indexed_rust_symbols,
    ) {
        if let Some(symbols) = index.symbols_by_name.get(&token) {
            push_indexed_definition_candidates(
                ctx,
                req,
                symbols,
                |_| 0.0,
                &mut candidates,
                &mut seen,
            );
        }
    }

    if candidates.is_empty() {
        let mut analysis_cache = HashMap::new();
        let search_matches = search_occurrences_with_fallback(|case_sensitive, whole_word| {
            search_rust_occurrences(
                &ctx.root,
                &token,
                case_sensitive,
                whole_word,
                MAX_REFERENCE_RESULTS,
            )
        })?;

        push_definition_search_matches(
            ctx,
            req,
            &token,
            search_matches,
            |entry, token| resolve_rust_declaration_kind(&mut analysis_cache, entry, token),
            |entry, token, declaration_kind| {
                score_rust_definition_candidate(ctx, req, token, declaration_kind, entry)
            },
            &mut candidates,
            &mut seen,
        );
    }

    sort_and_truncate_nav_locations(&mut candidates, MAX_DEFINITION_RESULTS);

    Ok(candidates)
}

fn rust_references(
    ctx: &ProjectContext,
    req: &NavPositionRequest,
) -> Result<Vec<NavLocation>, String> {
    let token = extract_token_at_position(&ctx.file_path, req.line, req.column)?;
    let Some(token) = token else {
        return Ok(Vec::new());
    };

    let matches = search_occurrences_with_fallback(|case_sensitive, whole_word| {
        search_rust_occurrences(
            &ctx.root,
            &token,
            case_sensitive,
            whole_word,
            MAX_REFERENCE_RESULTS,
        )
    })?;
    let mut classification_cache = HashMap::new();
    Ok(select_reference_locations(
        ctx,
        req,
        &token,
        matches,
        MAX_REFERENCE_RESULTS,
        |location, token| is_rust_declaration_location(&mut classification_cache, location, token),
    ))
}

fn nav_location_from_symbol(
    root: &Path,
    path: &Path,
    symbol: &RustSymbol,
    score: f64,
) -> Result<Option<NavLocation>, String> {
    nav_location_from_coordinates(
        root,
        path,
        symbol.line,
        symbol.column,
        symbol.end_line,
        symbol.end_column,
        score,
    )
}

fn score_rust_definition_candidate(
    ctx: &ProjectContext,
    req: &NavPositionRequest,
    token: &str,
    declaration_kind: &str,
    entry: &RustSearchMatch,
) -> f64 {
    let mut score = 0.0;
    let is_same_file = entry.relative_path == ctx.relative_path;
    let is_same_line = is_same_file && entry.line == req.line;
    let file_stem = Path::new(&entry.relative_path)
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("");

    if file_stem == token {
        score += 4.0;
    }
    if is_same_file {
        score += 2.0;
    }
    if is_same_line {
        score -= 4.0;
    }

    score += match declaration_kind {
        "struct" | "enum" | "trait" | "type" | "module" => 7.0,
        "method" | "function" => 5.0,
        "constant" | "variable" => 3.0,
        _ => 1.0,
    };

    if is_type_like(token) && matches!(declaration_kind, "struct" | "enum" | "trait" | "type") {
        score += 2.0;
    }

    score
}

#[cfg(test)]
mod tests {
    use super::{analyze_rust_file, rust_definition, rust_references};
    use crate::services::code_nav::types::{NavPositionRequest, ProjectContext};
    use std::fs;
    use std::path::PathBuf;

    fn make_temp_rust_project() -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "code_nav_rust_provider_test_{}",
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(root.join("src")).expect("create src dir");
        fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = 'demo'\nversion = '0.1.0'\n",
        )
        .expect("write cargo");
        root
    }

    #[test]
    fn rust_document_symbols_detect_types_and_functions() {
        let root = make_temp_rust_project();
        let path = root.join("src/lib.rs");
        fs::write(
            &path,
            r#"pub struct User;

impl User {
    pub fn greet(&self) {}
}

pub fn helper() {}
"#,
        )
        .expect("write rust file");

        let analysis = analyze_rust_file(&path).expect("analyze rust file");
        let names: Vec<(String, String)> = analysis
            .symbols
            .iter()
            .map(|item| (item.name.clone(), item.kind.clone()))
            .collect();

        assert!(names.contains(&(String::from("User"), String::from("struct"))));
        assert!(names.contains(&(String::from("greet"), String::from("method"))));
        assert!(names.contains(&(String::from("helper"), String::from("function"))));

        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn rust_definition_prefers_function_declaration() {
        let root = make_temp_rust_project();
        let foo = root.join("src/foo.rs");
        let main = root.join("src/main.rs");
        fs::write(
            &foo,
            r#"pub fn build_user_record() -> &'static str {
    "ok"
}
"#,
        )
        .expect("write foo");
        fs::write(
            &main,
            r#"mod foo;

fn main() {
    let _ = foo::build_user_record();
}
"#,
        )
        .expect("write main");

        let ctx = ProjectContext {
            root: root.clone(),
            file_path: main.clone(),
            relative_path: "src/main.rs".to_string(),
            language: "rust".to_string(),
        };
        let request = NavPositionRequest {
            project_root: root.to_string_lossy().to_string(),
            file_path: main.to_string_lossy().to_string(),
            line: 4,
            column: 19,
        };

        let locations = rust_definition(&ctx, &request).expect("resolve rust definition");
        assert!(
            locations
                .iter()
                .any(|item| item.relative_path.ends_with("src/foo.rs") && item.line == 1),
            "expected foo.rs function definition, got: {locations:?}"
        );

        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn rust_references_skip_definition_when_usage_exists() {
        let root = make_temp_rust_project();
        let path = root.join("src/lib.rs");
        fs::write(
            &path,
            r#"pub fn greet() {
    let name = "demo";
    println!("{}", name);
}
"#,
        )
        .expect("write lib");

        let ctx = ProjectContext {
            root: root.clone(),
            file_path: path.clone(),
            relative_path: "src/lib.rs".to_string(),
            language: "rust".to_string(),
        };
        let request = NavPositionRequest {
            project_root: root.to_string_lossy().to_string(),
            file_path: path.to_string_lossy().to_string(),
            line: 3,
            column: 20,
        };

        let locations = rust_references(&ctx, &request).expect("resolve rust references");
        assert!(
            locations.iter().any(|item| item.line == 3),
            "expected usage line to appear in references: {locations:?}"
        );
        assert!(
            locations.iter().all(|item| item.line != 2),
            "definition line should be filtered when usages exist: {locations:?}"
        );

        fs::remove_dir_all(root).ok();
    }
}
