mod analysis;

use std::collections::{HashMap, HashSet};
use std::path::Path;

use crate::services::code_nav::fallback::extract_token_at_position;
use crate::services::code_nav::languages::python::analysis::{
    analyze_python_file, is_python_declaration_location, nav_location_from_symbol,
    resolve_imported_symbol_paths, resolve_python_declaration_kind,
    score_python_definition_candidate, search_python_occurrences, PYTHON_EXTENSIONS,
    PYTHON_IGNORED_DIRS,
};
use crate::services::code_nav::languages::shared_nav::{is_type_like, push_unique_location};
use crate::services::code_nav::symbol_index::{
    nav_location_from_indexed_symbol, project_symbol_index, score_indexed_definition_candidate,
    IndexedSymbol,
};
use crate::services::code_nav::types::{
    DocumentSymbolItem, DocumentSymbolsRequest, DocumentSymbolsResponse, NavCapabilities,
    NavLocation, NavPositionRequest, ProjectContext,
};
use crate::services::code_nav::CodeNavProvider;

const MAX_DEFINITION_RESULTS: usize = 20;
const MAX_REFERENCE_RESULTS: usize = 100;
const MAX_SYMBOL_RESULTS: usize = 200;

fn indexed_python_symbols(path: &Path) -> Result<Vec<IndexedSymbol>, String> {
    let analysis = analyze_python_file(path)?;
    Ok(analysis
        .symbols
        .into_iter()
        .map(|symbol| IndexedSymbol {
            name: symbol.name,
            kind: symbol.kind,
            line: symbol.line,
            column: symbol.column,
            end_line: symbol.end_line,
            end_column: symbol.end_column,
        })
        .collect())
}

#[derive(Default)]
pub struct PythonCodeNavProvider;

#[axum::async_trait]
impl CodeNavProvider for PythonCodeNavProvider {
    fn provider_id(&self) -> &'static str {
        "python"
    }

    fn language_id(&self) -> &'static str {
        "python"
    }

    fn definition_mode(&self) -> &'static str {
        "provider-heuristic"
    }

    fn references_mode(&self) -> &'static str {
        "provider-heuristic"
    }

    fn document_symbols_mode(&self) -> &'static str {
        "provider-heuristic"
    }

    fn supports_file(&self, file_path: &Path) -> bool {
        file_path.extension().and_then(|value| value.to_str()) == Some("py")
    }

    fn detect_project(&self, ctx: &ProjectContext) -> bool {
        ctx.root.join("pyproject.toml").exists()
            || ctx.root.join("requirements.txt").exists()
            || ctx.root.join("setup.py").exists()
    }

    fn capabilities(&self, _ctx: &ProjectContext) -> NavCapabilities {
        NavCapabilities {
            supports_definition: true,
            supports_references: true,
            supports_document_symbols: true,
        }
    }

    async fn definition(
        &self,
        ctx: &ProjectContext,
        req: &NavPositionRequest,
    ) -> Result<Vec<NavLocation>, String> {
        python_definition(ctx, req)
    }

    async fn references(
        &self,
        ctx: &ProjectContext,
        req: &NavPositionRequest,
    ) -> Result<Vec<NavLocation>, String> {
        python_references(ctx, req)
    }

    async fn document_symbols(
        &self,
        ctx: &ProjectContext,
        _req: &DocumentSymbolsRequest,
    ) -> Result<DocumentSymbolsResponse, String> {
        let analysis = analyze_python_file(&ctx.file_path)?;
        let mut symbols: Vec<DocumentSymbolItem> = analysis
            .symbols
            .into_iter()
            .map(|item| DocumentSymbolItem {
                name: item.name,
                kind: item.kind,
                line: item.line,
                column: item.column,
                end_line: item.end_line,
                end_column: item.end_column,
            })
            .collect();
        if symbols.len() > MAX_SYMBOL_RESULTS {
            symbols.truncate(MAX_SYMBOL_RESULTS);
        }

        Ok(DocumentSymbolsResponse {
            provider: self.provider_id().to_string(),
            language: self.language_id().to_string(),
            mode: self.document_symbols_mode().to_string(),
            symbols,
        })
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

    for symbol in current
        .symbols
        .iter()
        .filter(|item| item.name == token && item.line != req.line)
    {
        if let Some(location) = nav_location_from_symbol(&ctx.root, &ctx.file_path, symbol, 9.0)? {
            push_unique_location(&mut candidates, &mut seen, location);
        }
    }

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
            for indexed in symbols {
                if indexed.relative_path == ctx.relative_path && indexed.symbol.line == req.line {
                    continue;
                }
                let mut score = score_indexed_definition_candidate(ctx, req, indexed);
                if resolved_path_set.contains(&indexed.path) {
                    score += 10.0;
                }
                let location = match nav_location_from_indexed_symbol(&ctx.root, indexed, score) {
                    Ok(location) => location,
                    Err(_) => continue,
                };
                push_unique_location(&mut candidates, &mut seen, location);
            }
        }
    }

    if candidates.is_empty() {
        let mut analysis_cache = HashMap::new();
        let mut search_matches =
            search_python_occurrences(&ctx.root, &token, true, true, MAX_REFERENCE_RESULTS)?;
        if search_matches.is_empty() {
            search_matches =
                search_python_occurrences(&ctx.root, &token, false, true, MAX_REFERENCE_RESULTS)?;
        }

        for entry in search_matches {
            let Some(declaration_kind) =
                resolve_python_declaration_kind(&mut analysis_cache, &entry, &token)
            else {
                continue;
            };
            let score = score_python_definition_candidate(
                ctx,
                req,
                &token,
                declaration_kind,
                &entry,
                &resolved_path_set,
            );
            let location = NavLocation {
                path: entry.path,
                relative_path: entry.relative_path,
                line: entry.line,
                column: entry.column,
                end_line: entry.line,
                end_column: entry.column + token.chars().count().saturating_sub(1),
                preview: entry.text,
                score,
            };
            push_unique_location(&mut candidates, &mut seen, location);
        }
    }

    candidates.sort_by(|left, right| {
        right
            .score
            .partial_cmp(&left.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(left.relative_path.cmp(&right.relative_path))
            .then(left.line.cmp(&right.line))
            .then(left.column.cmp(&right.column))
    });
    if candidates.len() > MAX_DEFINITION_RESULTS {
        candidates.truncate(MAX_DEFINITION_RESULTS);
    }

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

    let mut matches =
        search_python_occurrences(&ctx.root, &token, true, true, MAX_REFERENCE_RESULTS)?;
    if matches.is_empty() {
        matches = search_python_occurrences(&ctx.root, &token, false, true, MAX_REFERENCE_RESULTS)?;
    }

    let mut locations = Vec::new();
    let mut seen = HashSet::new();

    for entry in matches {
        let location = NavLocation {
            score: if entry.relative_path == ctx.relative_path {
                1.5
            } else {
                1.0
            },
            path: entry.path,
            relative_path: entry.relative_path,
            line: entry.line,
            column: entry.column,
            end_line: entry.line,
            end_column: entry.column + token.chars().count().saturating_sub(1),
            preview: entry.text,
        };
        push_unique_location(&mut locations, &mut seen, location);
    }

    let mut declarations = Vec::new();
    let mut references = Vec::new();
    let mut classification_cache = HashMap::new();
    for location in locations {
        if is_python_declaration_location(&mut classification_cache, &location, &token) {
            declarations.push(location);
        } else {
            references.push(location);
        }
    }

    let mut out = if references.is_empty() {
        declarations
    } else {
        references
    };
    out.sort_by(|left, right| {
        (left.relative_path != ctx.relative_path)
            .cmp(&(right.relative_path != ctx.relative_path))
            .then(left.relative_path.cmp(&right.relative_path))
            .then(left.line.cmp(&right.line))
            .then(left.column.cmp(&right.column))
    });
    if out.len() > MAX_REFERENCE_RESULTS {
        out.truncate(MAX_REFERENCE_RESULTS);
    }

    Ok(out)
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
