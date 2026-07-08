// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

mod analysis;

use std::collections::{HashMap, HashSet};
use std::path::Path;

use crate::services::code_nav::fallback::extract_token_at_position;
use crate::services::code_nav::languages::python::analysis::{
    analyze_python_file, is_python_declaration_location, nav_location_from_symbol,
    resolve_imported_symbol_paths, resolve_python_declaration_kind,
    score_python_definition_candidate, search_python_occurrences, PythonSearchMatch, PythonSymbol,
    PYTHON_EXTENSIONS, PYTHON_IGNORED_DIRS,
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

impl_nav_symbol_like_for_field_struct!(PythonSymbol);
impl_nav_search_match_like_for_field_struct!(PythonSearchMatch);

fn indexed_python_symbols(
    path: &Path,
) -> Result<Vec<crate::services::code_nav::symbol_index::IndexedSymbol>, String> {
    let analysis = analyze_python_file(path)?;
    Ok(indexed_symbols_from(&analysis.symbols))
}

#[derive(Default)]
pub struct PythonCodeNavProvider;

impl HeuristicNavLanguage for PythonCodeNavProvider {
    type Symbol = PythonSymbol;

    const PROVIDER_ID: &'static str = "python";
    const LANGUAGE_ID: &'static str = "python";
    const FILE_EXTENSION: &'static str = "py";
    const MAX_SYMBOL_RESULTS: usize = self::MAX_SYMBOL_RESULTS;

    fn detect_project(ctx: &ProjectContext) -> bool {
        ctx.root.join("pyproject.toml").exists()
            || ctx.root.join("requirements.txt").exists()
            || ctx.root.join("setup.py").exists()
    }

    fn definition(
        ctx: &ProjectContext,
        req: &NavPositionRequest,
    ) -> Result<Vec<NavLocation>, String> {
        python_definition(ctx, req)
    }

    fn references(
        ctx: &ProjectContext,
        req: &NavPositionRequest,
    ) -> Result<Vec<NavLocation>, String> {
        python_references(ctx, req)
    }

    fn analyze_document_symbols(file_path: &Path) -> Result<Vec<Self::Symbol>, String> {
        Ok(analyze_python_file(file_path)?.symbols)
    }
}

fn python_definition(
    ctx: &ProjectContext,
    req: &NavPositionRequest,
) -> Result<Vec<NavLocation>, String> {
    let token = extract_token_at_position(&ctx.file_path, req.line, req.column)?;
    let Some(token) = token else {
        return Ok(Vec::new());
    };

    let current = analyze_python_file(&ctx.file_path)?;
    let resolved_imports = resolve_imported_symbol_paths(&ctx.root, &current, &token)?;
    let resolved_path_set: HashSet<String> = resolved_imports
        .iter()
        .map(|item| item.path.to_string_lossy().to_string())
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

    for resolved in resolved_imports {
        let analysis = analyze_python_file(&resolved.path)?;
        for symbol in analysis
            .symbols
            .iter()
            .filter(|item| item.name == resolved.symbol_name)
        {
            let score = if is_type_like(&resolved.symbol_name) {
                15.0
            } else {
                12.0
            };
            if let Some(location) =
                nav_location_from_symbol(&ctx.root, &resolved.path, symbol, score)?
            {
                push_unique_location(&mut candidates, &mut seen, location);
            }
        }
    }

    if let Ok(index) = project_symbol_index(
        &ctx.root,
        "python",
        PYTHON_EXTENSIONS,
        PYTHON_IGNORED_DIRS,
        indexed_python_symbols,
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
            search_python_occurrences(
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
            |entry, token| resolve_python_declaration_kind(&mut analysis_cache, entry, token),
            |entry, token, declaration_kind| {
                score_python_definition_candidate(
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

fn python_references(
    ctx: &ProjectContext,
    req: &NavPositionRequest,
) -> Result<Vec<NavLocation>, String> {
    let token = extract_token_at_position(&ctx.file_path, req.line, req.column)?;
    let Some(token) = token else {
        return Ok(Vec::new());
    };

    let matches = search_occurrences_with_fallback(|case_sensitive, whole_word| {
        search_python_occurrences(
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
        |location, token| {
            is_python_declaration_location(&mut classification_cache, location, token)
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::{analyze_python_file, python_definition, python_references};
    use crate::services::code_nav::types::{NavPositionRequest, ProjectContext};
    use std::fs;
    use std::path::PathBuf;

    fn make_temp_python_project() -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "code_nav_python_provider_test_{}",
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(root.join("app")).expect("create package dir");
        fs::write(root.join("pyproject.toml"), "[project]\nname = 'demo'\n")
            .expect("write pyproject");
        root
    }

    #[test]
    fn python_document_symbols_detect_classes_and_functions() {
        let root = make_temp_python_project();
        let path = root.join("app/sample.py");
        fs::write(
            &path,
            r#"class Sample:
    def greet(self, who):
        return who

def helper():
    return "ok"
"#,
        )
        .expect("write sample python file");

        let analysis = analyze_python_file(&path).expect("analyze python file");
        let names: Vec<(String, String)> = analysis
            .symbols
            .iter()
            .map(|item| (item.name.clone(), item.kind.clone()))
            .collect();

        assert!(names.contains(&(String::from("Sample"), String::from("class"))));
        assert!(names.contains(&(String::from("greet"), String::from("method"))));
        assert!(names.contains(&(String::from("helper"), String::from("function"))));

        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn python_definition_prefers_imported_function_file() {
        let root = make_temp_python_project();
        let helpers = root.join("app/helpers.py");
        let main = root.join("app/main.py");
        fs::write(
            &helpers,
            r#"def greet():
    return "hello"
"#,
        )
        .expect("write helpers");
        fs::write(
            &main,
            r#"from app.helpers import greet

def run():
    return greet()
"#,
        )
        .expect("write main");

        let ctx = ProjectContext {
            root: root.clone(),
            file_path: main.clone(),
            relative_path: "app/main.py".to_string(),
            language: "python".to_string(),
        };
        let request = NavPositionRequest {
            project_root: root.to_string_lossy().to_string(),
            file_path: main.to_string_lossy().to_string(),
            line: 4,
            column: 14,
        };

        let locations = python_definition(&ctx, &request).expect("resolve python definition");
        assert!(
            locations
                .iter()
                .any(|item| item.relative_path.ends_with("app/helpers.py") && item.line == 1),
            "expected helpers.py function definition, got: {locations:?}"
        );

        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn python_references_skip_definition_when_usage_exists() {
        let root = make_temp_python_project();
        let path = root.join("app/sample.py");
        fs::write(
            &path,
            r#"name = "demo"

def greet():
    return name
"#,
        )
        .expect("write sample");

        let ctx = ProjectContext {
            root: root.clone(),
            file_path: path.clone(),
            relative_path: "app/sample.py".to_string(),
            language: "python".to_string(),
        };
        let request = NavPositionRequest {
            project_root: root.to_string_lossy().to_string(),
            file_path: path.to_string_lossy().to_string(),
            line: 4,
            column: 13,
        };

        let locations = python_references(&ctx, &request).expect("resolve python references");
        assert!(
            locations.iter().any(|item| item.line == 4),
            "expected usage line to appear in references: {locations:?}"
        );
        assert!(
            locations.iter().all(|item| item.line != 1),
            "definition line should be filtered when usages exist: {locations:?}"
        );

        fs::remove_dir_all(root).ok();
    }
}
