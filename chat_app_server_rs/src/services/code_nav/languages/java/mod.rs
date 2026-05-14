mod analysis;

use std::collections::{HashMap, HashSet};
use std::path::Path;

use crate::services::code_nav::fallback::extract_token_at_position;
use crate::services::code_nav::languages::java::analysis::{
    analyze_java_file, is_java_declaration_location, nav_location_from_symbol,
    resolve_imported_type_paths, resolve_java_declaration_kind, score_java_definition_candidate,
    search_java_occurrences, JAVA_EXTENSIONS, JAVA_IGNORED_DIRS,
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

fn indexed_java_symbols(path: &Path) -> Result<Vec<IndexedSymbol>, String> {
    let analysis = analyze_java_file(path)?;
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
pub struct JavaCodeNavProvider;

#[axum::async_trait]
impl CodeNavProvider for JavaCodeNavProvider {
    fn provider_id(&self) -> &'static str {
        "java"
    }

    fn language_id(&self) -> &'static str {
        "java"
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
        file_path.extension().and_then(|value| value.to_str()) == Some("java")
    }

    fn detect_project(&self, ctx: &ProjectContext) -> bool {
        ctx.root.join("pom.xml").exists()
            || ctx.root.join("build.gradle").exists()
            || ctx.root.join("settings.gradle").exists()
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
        java_definition(ctx, req)
    }

    async fn references(
        &self,
        ctx: &ProjectContext,
        req: &NavPositionRequest,
    ) -> Result<Vec<NavLocation>, String> {
        java_references(ctx, req)
    }

    async fn document_symbols(
        &self,
        ctx: &ProjectContext,
        _req: &DocumentSymbolsRequest,
    ) -> Result<DocumentSymbolsResponse, String> {
        let analysis = analyze_java_file(&ctx.file_path)?;
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

fn java_definition(
    ctx: &ProjectContext,
    req: &NavPositionRequest,
) -> Result<Vec<NavLocation>, String> {
    let token = extract_token_at_position(&ctx.file_path, req.line, req.column)?;
    let Some(token) = token else {
        return Ok(Vec::new());
    };

    let current = analyze_java_file(&ctx.file_path)?;
    let resolved_type_paths = resolve_imported_type_paths(&ctx.root, &current, &token)?;
    let resolved_path_set: HashSet<String> = resolved_type_paths
        .iter()
        .map(|path| path.to_string_lossy().to_string())
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

    for path in resolved_type_paths {
        let analysis = analyze_java_file(&path)?;
        for symbol in analysis.symbols.iter().filter(|item| item.name == token) {
            let score = if is_type_like(&token)
                && matches!(
                    symbol.kind.as_str(),
                    "class" | "interface" | "enum" | "record" | "type"
                ) {
                16.0
            } else {
                11.0
            };
            if let Some(location) = nav_location_from_symbol(&ctx.root, &path, symbol, score)? {
                push_unique_location(&mut candidates, &mut seen, location);
            }
        }
    }

    if let Ok(index) = project_symbol_index(
        &ctx.root,
        "java",
        JAVA_EXTENSIONS,
        JAVA_IGNORED_DIRS,
        indexed_java_symbols,
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
            search_java_occurrences(&ctx.root, &token, true, true, MAX_REFERENCE_RESULTS)?;
        if search_matches.is_empty() {
            search_matches =
                search_java_occurrences(&ctx.root, &token, false, true, MAX_REFERENCE_RESULTS)?;
        }

        for entry in search_matches {
            let Some(declaration_kind) = resolve_java_declaration_kind(
                &mut analysis_cache,
                &entry,
                &token,
                current.primary_type.as_deref(),
            ) else {
                continue;
            };
            let score = score_java_definition_candidate(
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

fn java_references(
    ctx: &ProjectContext,
    req: &NavPositionRequest,
) -> Result<Vec<NavLocation>, String> {
    let token = extract_token_at_position(&ctx.file_path, req.line, req.column)?;
    let Some(token) = token else {
        return Ok(Vec::new());
    };

    let current = analyze_java_file(&ctx.file_path)?;
    let mut matches =
        search_java_occurrences(&ctx.root, &token, true, true, MAX_REFERENCE_RESULTS)?;
    if matches.is_empty() {
        matches = search_java_occurrences(&ctx.root, &token, false, true, MAX_REFERENCE_RESULTS)?;
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
        if is_java_declaration_location(
            &mut classification_cache,
            &location,
            &token,
            current.primary_type.as_deref(),
        ) {
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
    use super::analysis::{
        classify_java_declaration, extract_field_name, extract_method_signature,
    };
    use super::{analyze_java_file, java_definition, java_references};
    use crate::services::code_nav::fallback::extract_token_at_position;
    use crate::services::code_nav::types::{NavPositionRequest, ProjectContext};
    use std::fs;
    use std::path::PathBuf;

    fn make_temp_java_project() -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "code_nav_java_provider_test_{}",
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(root.join("src/main/java/com/example")).expect("create source dir");
        fs::write(root.join("pom.xml"), "<project/>").expect("write pom");
        root
    }

    #[test]
    fn java_document_symbols_detect_type_and_members() {
        let root = make_temp_java_project();
        let path = root.join("src/main/java/com/example/Sample.java");
        fs::write(
            &path,
            r#"package com.example;

public class Sample {
    private String name;

    public Sample() {}

    public String greet(String who) {
        return name + who;
    }
}
"#,
        )
        .expect("write java file");

        let analysis = analyze_java_file(&path).expect("analyze java file");
        let names: Vec<(String, String)> = analysis
            .symbols
            .iter()
            .map(|item| (item.name.clone(), item.kind.clone()))
            .collect();

        assert!(names.contains(&(String::from("Sample"), String::from("class"))));
        assert!(names.contains(&(String::from("Sample"), String::from("constructor"))));
        assert!(names.contains(&(String::from("greet"), String::from("method"))));
        assert!(names.contains(&(String::from("name"), String::from("field"))));

        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn java_document_symbols_ignore_annotation_line_and_detect_bean_method() {
        let root = make_temp_java_project();
        let path = root.join("src/main/java/com/example/KafkaConfig.java");
        fs::write(
            &path,
            r#"package com.example;

public class KafkaConfig {
    @Bean("kafkaProducerFactory")
    public ProducerFactory<Object, Object> kafkaProducerFactory(
        ObjectProvider<DefaultKafkaProducerFactoryCustomizer> customizers
    ) {
        return null;
    }
}
"#,
        )
        .expect("write KafkaConfig");

        let analysis = analyze_java_file(&path).expect("analyze java file");
        let names: Vec<(String, String)> = analysis
            .symbols
            .iter()
            .map(|item| (item.name.clone(), item.kind.clone()))
            .collect();

        assert!(names.contains(&(String::from("KafkaConfig"), String::from("class"))));
        assert!(names.contains(&(String::from("kafkaProducerFactory"), String::from("method"))));
        assert!(!names.contains(&(String::from("n"), String::from("method"))));
        assert!(!names.contains(&(String::from("Bean"), String::from("method"))));
        assert!(
            extract_method_signature("@Bean(\"kafkaProducerFactory\")", "KafkaConfig").is_none()
        );
        assert_eq!(
            extract_method_signature(
                "@Bean(\"kafkaProducerFactory\") public ProducerFactory<Object, Object> kafkaProducerFactory() {",
                "KafkaConfig"
            )
            .map(|(name, kind)| (name, kind)),
            Some((String::from("kafkaProducerFactory"), String::from("method")))
        );
        assert_eq!(
            extract_method_signature(
                "public void configured(@Qualifier(\"main\") String name,",
                "KafkaConfig"
            )
            .map(|(name, kind)| (name, kind)),
            Some((String::from("configured"), String::from("method")))
        );

        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn java_field_heuristics_detect_simple_field_declaration() {
        assert_eq!(
            extract_field_name("private String name;").as_deref(),
            Some("name")
        );
        assert_eq!(
            classify_java_declaration("private String name;", "name", Some("Sample")),
            Some("field")
        );
        assert_eq!(extract_field_name("return name;"), None);
        assert_eq!(
            classify_java_declaration("return name;", "name", Some("Sample")),
            None
        );
    }

    #[test]
    fn java_extract_token_reads_field_usage_identifier() {
        let root = make_temp_java_project();
        let path = root.join("src/main/java/com/example/TokenSample.java");
        fs::write(
            &path,
            r#"package com.example;

public class TokenSample {
    private String name;

    public String greet() {
        return name;
    }
}
"#,
        )
        .expect("write TokenSample");

        let token = extract_token_at_position(&path, 7, 16).expect("extract token");
        assert_eq!(token.as_deref(), Some("name"));

        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn java_definition_prefers_imported_type_file() {
        let root = make_temp_java_project();
        let foo = root.join("src/main/java/com/example/Foo.java");
        let bar = root.join("src/main/java/com/example/Bar.java");
        fs::write(
            &foo,
            r#"package com.example;

public class Foo {
}
"#,
        )
        .expect("write Foo");
        fs::write(
            &bar,
            r#"package com.example;

import com.example.Foo;

public class Bar {
    private Foo foo = new Foo();
}
"#,
        )
        .expect("write Bar");

        let ctx = ProjectContext {
            root: root.clone(),
            file_path: bar.clone(),
            relative_path: "src/main/java/com/example/Bar.java".to_string(),
            language: "java".to_string(),
        };
        let request = NavPositionRequest {
            project_root: root.to_string_lossy().to_string(),
            file_path: bar.to_string_lossy().to_string(),
            line: 6,
            column: 13,
        };

        let locations = java_definition(&ctx, &request).expect("resolve java definition");
        assert!(
            locations
                .iter()
                .any(|item| item.relative_path.ends_with("com/example/Foo.java") && item.line == 3),
            "expected Foo.java class definition, got: {locations:?}"
        );

        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn java_references_skip_definition_when_usage_exists() {
        let root = make_temp_java_project();
        let path = root.join("src/main/java/com/example/RefSample.java");
        fs::write(
            &path,
            r#"package com.example;

public class RefSample {
    private String name;

    public String greet() {
        return name;
    }
}
"#,
        )
        .expect("write RefSample");

        let ctx = ProjectContext {
            root: root.clone(),
            file_path: path.clone(),
            relative_path: "src/main/java/com/example/RefSample.java".to_string(),
            language: "java".to_string(),
        };
        let request = NavPositionRequest {
            project_root: root.to_string_lossy().to_string(),
            file_path: path.to_string_lossy().to_string(),
            line: 7,
            column: 16,
        };

        let locations = java_references(&ctx, &request).expect("resolve java references");
        assert!(
            locations.iter().any(|item| item.line == 7),
            "expected usage line to appear in references: {locations:?}"
        );
        assert!(
            locations.iter().all(|item| item.line != 4),
            "definition line should be filtered when usages exist: {locations:?}"
        );

        fs::remove_dir_all(root).ok();
    }
}
