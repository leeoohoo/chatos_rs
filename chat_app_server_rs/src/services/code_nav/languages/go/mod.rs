// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

mod analysis;

use std::collections::{HashMap, HashSet};
use std::path::Path;

use crate::services::code_nav::fallback::extract_token_at_position;
use crate::services::code_nav::languages::go::analysis::{
    analyze_go_file, is_go_declaration_location, nav_location_from_symbol,
    resolve_go_declaration_kind, resolve_imported_symbol_files, score_go_definition_candidate,
    search_go_occurrences, GoSearchMatch, GoSymbol, GO_EXTENSIONS, GO_IGNORED_DIRS,
};
use crate::services::code_nav::languages::shared_nav::{
    impl_nav_search_match_like_for_field_struct, impl_nav_symbol_like_for_field_struct,
    indexed_symbols_from, is_type_like, push_current_file_symbol_definitions,
    push_definition_search_matches, push_indexed_definition_candidates, push_unique_location,
    search_occurrences_with_fallback, select_reference_locations, sort_and_truncate_nav_locations,
    HeuristicNavLanguage,
};
use crate::services::code_nav::symbol_index::project_symbol_index;
use crate::services::code_nav::types::{NavLocation, NavPositionRequest, ProjectContext};

const MAX_DEFINITION_RESULTS: usize = 20;
const MAX_REFERENCE_RESULTS: usize = 100;
const MAX_SYMBOL_RESULTS: usize = 200;

impl_nav_symbol_like_for_field_struct!(GoSymbol);
impl_nav_search_match_like_for_field_struct!(GoSearchMatch);

fn indexed_go_symbols(
    path: &Path,
) -> Result<Vec<crate::services::code_nav::symbol_index::IndexedSymbol>, String> {
    let analysis = analyze_go_file(path)?;
    Ok(indexed_symbols_from(&analysis.symbols))
}

#[derive(Default)]
pub struct GoCodeNavProvider;

impl HeuristicNavLanguage for GoCodeNavProvider {
    type Symbol = GoSymbol;

    const PROVIDER_ID: &'static str = "go";
    const LANGUAGE_ID: &'static str = "go";
    const FILE_EXTENSION: &'static str = "go";
    const MAX_SYMBOL_RESULTS: usize = self::MAX_SYMBOL_RESULTS;

    fn detect_project(ctx: &ProjectContext) -> bool {
        ctx.root.join("go.mod").exists()
    }

    fn definition(
        ctx: &ProjectContext,
        req: &NavPositionRequest,
    ) -> Result<Vec<NavLocation>, String> {
        go_definition(ctx, req)
    }

    fn references(
        ctx: &ProjectContext,
        req: &NavPositionRequest,
    ) -> Result<Vec<NavLocation>, String> {
        go_references(ctx, req)
    }

    fn analyze_document_symbols(file_path: &Path) -> Result<Vec<Self::Symbol>, String> {
        Ok(analyze_go_file(file_path)?.symbols)
    }
}

fn go_definition(
    ctx: &ProjectContext,
    req: &NavPositionRequest,
) -> Result<Vec<NavLocation>, String> {
    let token = extract_token_at_position(&ctx.file_path, req.line, req.column)?;
    let Some(token) = token else {
        return Ok(Vec::new());
    };

    let current = analyze_go_file(&ctx.file_path)?;
    let resolved_import_files = resolve_imported_symbol_files(&ctx.root, &current, &token)?;
    let resolved_path_set: HashSet<String> = resolved_import_files
        .iter()
        .map(|path: &std::path::PathBuf| path.to_string_lossy().to_string())
        .collect();

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

    for path in resolved_import_files {
        let analysis = analyze_go_file(&path)?;
        for symbol in analysis.symbols.iter().filter(|item| item.name == token) {
            let score = if is_type_like(&token)
                && matches!(symbol.kind.as_str(), "struct" | "interface" | "type")
            {
                16.0
            } else {
                12.0
            };
            if let Some(location) = nav_location_from_symbol(&ctx.root, &path, symbol, score)? {
                push_unique_location(&mut candidates, &mut seen, location);
            }
        }
    }

    if let Ok(index) = project_symbol_index(
        &ctx.root,
        "go",
        GO_EXTENSIONS,
        GO_IGNORED_DIRS,
        indexed_go_symbols,
    ) {
        if let Some(symbols) = index.symbols_by_name.get(&token) {
            push_indexed_definition_candidates(
                ctx,
                req,
                symbols,
                |indexed| {
                    if resolved_path_set.contains(&indexed.path) {
                        10.0
                    } else {
                        0.0
                    }
                },
                &mut candidates,
                &mut seen,
            );
        }
    }

    if candidates.is_empty() {
        let mut analysis_cache = HashMap::new();
        let search_matches = search_occurrences_with_fallback(|case_sensitive, whole_word| {
            search_go_occurrences(
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
            |entry, token| resolve_go_declaration_kind(&mut analysis_cache, entry, token),
            |entry, token, declaration_kind| {
                score_go_definition_candidate(
                    ctx,
                    req,
                    token,
                    declaration_kind,
                    entry,
                    &resolved_path_set,
                )
            },
            &mut candidates,
            &mut seen,
        );
    }

    sort_and_truncate_nav_locations(&mut candidates, MAX_DEFINITION_RESULTS);

    Ok(candidates)
}

fn go_references(
    ctx: &ProjectContext,
    req: &NavPositionRequest,
) -> Result<Vec<NavLocation>, String> {
    let token = extract_token_at_position(&ctx.file_path, req.line, req.column)?;
    let Some(token) = token else {
        return Ok(Vec::new());
    };

    let matches = search_occurrences_with_fallback(|case_sensitive, whole_word| {
        search_go_occurrences(
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
        |location, token| is_go_declaration_location(&mut classification_cache, location, token),
    ))
}

#[cfg(test)]
mod tests {
    use super::{analyze_go_file, go_definition, go_references};
    use crate::services::code_nav::types::{NavPositionRequest, ProjectContext};
    use std::fs;
    use std::path::PathBuf;

    fn make_temp_go_project() -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "code_nav_go_provider_test_{}",
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(root.join("helper")).expect("create helper dir");
        fs::write(root.join("go.mod"), "module demo\n\ngo 1.22\n").expect("write go.mod");
        root
    }

    #[test]
    fn go_document_symbols_detect_types_and_functions() {
        let root = make_temp_go_project();
        let path = root.join("main.go");
        fs::write(
            &path,
            r#"package main

type User struct{}

func (u User) Greet() {}

func Helper() {}
"#,
        )
        .expect("write main.go");

        let analysis = analyze_go_file(&path).expect("analyze go file");
        let names: Vec<(String, String)> = analysis
            .symbols
            .iter()
            .map(|item| (item.name.clone(), item.kind.clone()))
            .collect();

        assert!(names.contains(&(String::from("User"), String::from("struct"))));
        assert!(names.contains(&(String::from("Greet"), String::from("method"))));
        assert!(names.contains(&(String::from("Helper"), String::from("function"))));

        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn go_definition_prefers_imported_package_function() {
        let root = make_temp_go_project();
        let helper = root.join("helper/helper.go");
        let main = root.join("main.go");
        fs::write(
            &helper,
            r#"package helper

func BuildUserRecord() string {
    return "ok"
}
"#,
        )
        .expect("write helper.go");
        fs::write(
            &main,
            r#"package main

import "demo/helper"

func main() {
    _ = helper.BuildUserRecord()
}
"#,
        )
        .expect("write main.go");

        let ctx = ProjectContext {
            root: root.clone(),
            file_path: main.clone(),
            relative_path: "main.go".to_string(),
            language: "go".to_string(),
        };
        let request = NavPositionRequest {
            project_root: root.to_string_lossy().to_string(),
            file_path: main.to_string_lossy().to_string(),
            line: 6,
            column: 16,
        };

        let locations = go_definition(&ctx, &request).expect("resolve go definition");
        assert!(
            locations
                .iter()
                .any(|item| item.relative_path.ends_with("helper/helper.go") && item.line == 3),
            "expected helper package function definition, got: {locations:?}"
        );

        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn go_references_skip_definition_when_usage_exists() {
        let root = make_temp_go_project();
        let path = root.join("main.go");
        fs::write(
            &path,
            r#"package main

func greet() {
    name := "demo"
    println(name)
}
"#,
        )
        .expect("write main.go");

        let ctx = ProjectContext {
            root: root.clone(),
            file_path: path.clone(),
            relative_path: "main.go".to_string(),
            language: "go".to_string(),
        };
        let request = NavPositionRequest {
            project_root: root.to_string_lossy().to_string(),
            file_path: path.to_string_lossy().to_string(),
            line: 5,
            column: 14,
        };

        let locations = go_references(&ctx, &request).expect("resolve go references");
        assert!(
            locations.iter().any(|item| item.line == 5),
            "expected usage line to appear in references: {locations:?}"
        );
        assert!(
            locations.iter().all(|item| item.line != 4),
            "definition line should be filtered when usages exist: {locations:?}"
        );

        fs::remove_dir_all(root).ok();
    }
}
