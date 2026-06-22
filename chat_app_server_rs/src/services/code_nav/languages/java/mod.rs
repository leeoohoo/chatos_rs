mod analysis;

use std::collections::{HashMap, HashSet};
use std::path::Path;

use crate::services::code_nav::fallback::extract_token_at_position;
use crate::services::code_nav::languages::java::analysis::{
    analyze_java_file, is_java_declaration_location, nav_location_from_symbol,
    resolve_imported_type_paths, resolve_java_declaration_kind, score_java_definition_candidate,
    search_java_occurrences, JavaSearchMatch, JavaSymbol, JAVA_EXTENSIONS, JAVA_IGNORED_DIRS,
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

impl_nav_symbol_like_for_field_struct!(JavaSymbol);
impl_nav_search_match_like_for_field_struct!(JavaSearchMatch);

fn indexed_java_symbols(
    path: &Path,
) -> Result<Vec<crate::services::code_nav::symbol_index::IndexedSymbol>, String> {
    let analysis = analyze_java_file(path)?;
    Ok(indexed_symbols_from(&analysis.symbols))
}

#[derive(Default)]
pub struct JavaCodeNavProvider;

impl HeuristicNavLanguage for JavaCodeNavProvider {
    type Symbol = JavaSymbol;

    const PROVIDER_ID: &'static str = "java";
    const LANGUAGE_ID: &'static str = "java";
    const FILE_EXTENSION: &'static str = "java";
    const MAX_SYMBOL_RESULTS: usize = self::MAX_SYMBOL_RESULTS;

    fn detect_project(ctx: &ProjectContext) -> bool {
        ctx.root.join("pom.xml").exists()
            || ctx.root.join("build.gradle").exists()
            || ctx.root.join("settings.gradle").exists()
    }

    fn definition(
        ctx: &ProjectContext,
        req: &NavPositionRequest,
    ) -> Result<Vec<NavLocation>, String> {
        java_definition(ctx, req)
    }

    fn references(
        ctx: &ProjectContext,
        req: &NavPositionRequest,
    ) -> Result<Vec<NavLocation>, String> {
        java_references(ctx, req)
    }

    fn analyze_document_symbols(file_path: &Path) -> Result<Vec<Self::Symbol>, String> {
        Ok(analyze_java_file(file_path)?.symbols)
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
            search_java_occurrences(
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
            |entry, token| {
                resolve_java_declaration_kind(
                    &mut analysis_cache,
                    entry,
                    token,
                    current.primary_type.as_deref(),
                )
            },
            |entry, token, declaration_kind| {
                score_java_definition_candidate(
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

fn java_references(
    ctx: &ProjectContext,
    req: &NavPositionRequest,
) -> Result<Vec<NavLocation>, String> {
    let token = extract_token_at_position(&ctx.file_path, req.line, req.column)?;
    let Some(token) = token else {
        return Ok(Vec::new());
    };

    let current = analyze_java_file(&ctx.file_path)?;
    let matches = search_occurrences_with_fallback(|case_sensitive, whole_word| {
        search_java_occurrences(
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
            is_java_declaration_location(
                &mut classification_cache,
                location,
                token,
                current.primary_type.as_deref(),
            )
        },
    ))
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
