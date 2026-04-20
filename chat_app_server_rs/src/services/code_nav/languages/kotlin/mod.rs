use std::fs;
use std::path::Path;

use once_cell::sync::Lazy;
use regex::Regex;

use crate::services::code_nav::languages::basic::{
    count_char, find_balanced_end, find_column, make_symbol, strip_c_style_comments,
    BasicFileAnalysis, BasicLanguageSpec, BasicSymbol,
};
use crate::services::code_nav::types::{
    DocumentSymbolsRequest, DocumentSymbolsResponse, NavCapabilities, NavLocation,
    NavPositionRequest, ProjectContext,
};
use crate::services::code_nav::CodeNavProvider;

const KOTLIN_IGNORED_DIRS: &[&str] = &[
    ".git",
    "node_modules",
    "dist",
    "build",
    "target",
    "out",
    ".idea",
    ".gradle",
];

const KOTLIN_EXTENSIONS: &[&str] = &["kt", "kts"];
const KOTLIN_PROJECT_FILES: &[&str] = &[
    "build.gradle",
    "build.gradle.kts",
    "settings.gradle",
    "settings.gradle.kts",
    "pom.xml",
];
const KOTLIN_PROJECT_EXTENSIONS: &[&str] = &["gradle", "kts"];

static TYPE_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"^\s*(?:(?:public|private|protected|internal|abstract|open|final|sealed|data|enum|annotation|value|inner)\s+)*(class|interface|object)\s+([A-Za-z_][A-Za-z0-9_]*)",
    )
    .unwrap()
});
static FUNCTION_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"^\s*(?:(?:public|private|protected|internal|abstract|open|final|override|suspend|inline|operator|infix|tailrec|external)\s+)*fun\s*(?:<[^>{}]+>\s*)?(?:(?:[A-Za-z_][A-Za-z0-9_.$<>?,\s]*\.)\s*)?([A-Za-z_][A-Za-z0-9_]*)\s*\(",
    )
    .unwrap()
});
static PROPERTY_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"^\s*(?:(?:public|private|protected|internal|abstract|open|final|override|lateinit|const)\s+)*(val|var)\s+([A-Za-z_][A-Za-z0-9_]*)\b",
    )
    .unwrap()
});

const SPEC: BasicLanguageSpec = BasicLanguageSpec {
    provider_id: "kotlin",
    language_id: "kotlin",
    extensions: KOTLIN_EXTENSIONS,
    ignored_dirs: KOTLIN_IGNORED_DIRS,
    project_files: KOTLIN_PROJECT_FILES,
    project_extensions: KOTLIN_PROJECT_EXTENSIONS,
    analyze_file: analyze_kotlin_file,
    classify_declaration: classify_kotlin_declaration,
};

#[derive(Debug, Clone)]
struct KotlinTypeScope {
    body_depth: i32,
}

#[derive(Default)]
pub struct KotlinCodeNavProvider;

#[axum::async_trait]
impl CodeNavProvider for KotlinCodeNavProvider {
    fn provider_id(&self) -> &'static str {
        SPEC.provider_id
    }

    fn language_id(&self) -> &'static str {
        SPEC.language_id
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
        SPEC.supports_file(file_path)
    }

    fn detect_project(&self, ctx: &ProjectContext) -> bool {
        SPEC.detect_project(ctx)
    }

    fn capabilities(&self, _ctx: &ProjectContext) -> NavCapabilities {
        SPEC.capabilities()
    }

    async fn definition(
        &self,
        ctx: &ProjectContext,
        req: &NavPositionRequest,
    ) -> Result<Vec<NavLocation>, String> {
        SPEC.definition(ctx, req)
    }

    async fn references(
        &self,
        ctx: &ProjectContext,
        req: &NavPositionRequest,
    ) -> Result<Vec<NavLocation>, String> {
        SPEC.references(ctx, req)
    }

    async fn document_symbols(
        &self,
        ctx: &ProjectContext,
        _req: &DocumentSymbolsRequest,
    ) -> Result<DocumentSymbolsResponse, String> {
        SPEC.document_symbols(ctx)
    }
}

fn analyze_kotlin_file(path: &Path) -> Result<BasicFileAnalysis, String> {
    let content = fs::read_to_string(path).map_err(|err| err.to_string())?;
    let mut symbols = Vec::new();
    let mut type_stack: Vec<KotlinTypeScope> = Vec::new();
    let mut brace_depth = 0i32;
    let mut in_block_comment = false;

    for (index, raw_line) in content.lines().enumerate() {
        let line_number = index + 1;
        let sanitized = strip_c_style_comments(raw_line, &mut in_block_comment);
        let trimmed = strip_kotlin_annotations(sanitized.trim());

        if !trimmed.is_empty() {
            if let Some(capture) = TYPE_RE.captures(trimmed) {
                let name = capture[2].to_string();
                let mut kind = capture[1].to_string();
                if trimmed.contains("enum class") {
                    kind = "enum".to_string();
                }
                push_symbol(&mut symbols, raw_line, name, kind.as_str(), line_number);
                let opens = count_char(&sanitized, '{') as i32;
                if opens > 0 {
                    type_stack.push(KotlinTypeScope {
                        body_depth: brace_depth + opens,
                    });
                }
            } else if let Some(capture) = FUNCTION_RE.captures(trimmed) {
                let name = capture[1].to_string();
                let kind = if type_stack.is_empty() {
                    "function"
                } else {
                    "method"
                };
                push_symbol(&mut symbols, raw_line, name, kind, line_number);
            } else if let Some(capture) = PROPERTY_RE.captures(trimmed) {
                let name = capture[2].to_string();
                push_symbol(&mut symbols, raw_line, name, "property", line_number);
            }
        }

        brace_depth += count_char(&sanitized, '{') as i32;
        brace_depth -= count_char(&sanitized, '}') as i32;
        while type_stack
            .last()
            .map(|scope| brace_depth < scope.body_depth)
            .unwrap_or(false)
        {
            type_stack.pop();
        }
    }

    symbols.sort_by(|left, right| {
        left.line
            .cmp(&right.line)
            .then(left.column.cmp(&right.column))
            .then(left.name.cmp(&right.name))
    });

    Ok(BasicFileAnalysis { symbols })
}

fn classify_kotlin_declaration(line: &str, token: &str) -> Option<&'static str> {
    let trimmed = strip_kotlin_annotations(line.trim());
    if let Some(capture) = TYPE_RE.captures(trimmed) {
        if capture.get(2).map(|value| value.as_str()) == Some(token) {
            if trimmed.contains("enum class") {
                return Some("enum");
            }
            return match capture.get(1).map(|value| value.as_str()) {
                Some("interface") => Some("interface"),
                Some("object") => Some("object"),
                _ => Some("class"),
            };
        }
    }
    if let Some(capture) = FUNCTION_RE.captures(trimmed) {
        if capture.get(1).map(|value| value.as_str()) == Some(token) {
            return Some("function");
        }
    }
    if let Some(capture) = PROPERTY_RE.captures(trimmed) {
        if capture.get(2).map(|value| value.as_str()) == Some(token) {
            return Some("property");
        }
    }
    None
}

fn push_symbol(
    symbols: &mut Vec<BasicSymbol>,
    raw_line: &str,
    name: String,
    kind: &str,
    line_number: usize,
) {
    let column = find_column(raw_line, &name).unwrap_or(1);
    symbols.push(make_symbol(name, kind, line_number, column));
}

fn strip_kotlin_annotations(mut line: &str) -> &str {
    loop {
        let trimmed = line.trim_start();
        if !trimmed.starts_with('@') {
            return trimmed;
        }
        let Some(rest) = consume_kotlin_annotation(trimmed) else {
            return trimmed;
        };
        line = rest;
        if line.trim().is_empty() {
            return "";
        }
    }
}

fn consume_kotlin_annotation(line: &str) -> Option<&str> {
    let mut index = '@'.len_utf8();
    let chars: Vec<(usize, char)> = line.char_indices().collect();
    while let Some((offset, ch)) = chars.iter().find(|(offset, _)| *offset == index) {
        if ch.is_alphanumeric() || *ch == '_' || *ch == '.' || *ch == ':' {
            index = offset + ch.len_utf8();
        } else {
            break;
        }
    }

    let rest = line.get(index..)?.trim_start();
    if rest.starts_with('(') {
        let end = find_balanced_end(rest, '(', ')')?;
        return rest.get(end..);
    }
    Some(rest)
}

#[cfg(test)]
mod tests {
    use super::{analyze_kotlin_file, classify_kotlin_declaration, KotlinCodeNavProvider, SPEC};
    use crate::services::code_nav::types::{NavPositionRequest, ProjectContext};
    use crate::services::code_nav::CodeNavProvider;
    use std::fs;
    use std::path::PathBuf;

    fn make_temp_kotlin_project() -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "code_nav_kotlin_provider_test_{}",
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(root.join("src/main/kotlin/com/example")).expect("create source dir");
        fs::write(
            root.join("build.gradle.kts"),
            "plugins { kotlin(\"jvm\") }\n",
        )
        .expect("write gradle file");
        root
    }

    #[test]
    fn kotlin_document_symbols_detect_types_functions_and_properties() {
        let root = make_temp_kotlin_project();
        let path = root.join("src/main/kotlin/com/example/Sample.kt");
        fs::write(
            &path,
            r#"package com.example

class Sample {
    val name = "demo"

    @JvmStatic
    fun greet(who: String): String = name + who
}

fun topLevel() = Unit
"#,
        )
        .expect("write kotlin file");

        let analysis = analyze_kotlin_file(&path).expect("analyze kotlin file");
        let names: Vec<(String, String)> = analysis
            .symbols
            .iter()
            .map(|item| (item.name.clone(), item.kind.clone()))
            .collect();

        assert!(names.contains(&(String::from("Sample"), String::from("class"))));
        assert!(names.contains(&(String::from("name"), String::from("property"))));
        assert!(names.contains(&(String::from("greet"), String::from("method"))));
        assert!(names.contains(&(String::from("topLevel"), String::from("function"))));
        assert_eq!(
            classify_kotlin_declaration("@Bean fun provide(): Any", "provide"),
            Some("function")
        );

        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn kotlin_provider_supports_kt_and_kts_files() {
        let provider = KotlinCodeNavProvider;
        assert!(provider.supports_file(PathBuf::from("Demo.kt").as_path()));
        assert!(provider.supports_file(PathBuf::from("build.gradle.kts").as_path()));
    }

    #[test]
    fn kotlin_definition_and_references_use_language_symbols() {
        let root = make_temp_kotlin_project();
        let path = root.join("src/main/kotlin/com/example/Main.kt");
        fs::write(
            &path,
            r#"package com.example

fun greet(): String = "hello"

fun run(): String = greet() + greet()
"#,
        )
        .expect("write kotlin file");

        let ctx = ProjectContext {
            root: root.clone(),
            file_path: path.clone(),
            relative_path: "src/main/kotlin/com/example/Main.kt".to_string(),
            language: "kotlin".to_string(),
        };
        let request = NavPositionRequest {
            project_root: root.to_string_lossy().to_string(),
            file_path: path.to_string_lossy().to_string(),
            line: 5,
            column: 22,
        };

        let definitions = SPEC
            .definition(&ctx, &request)
            .expect("resolve kotlin definition");
        assert!(
            definitions.iter().any(|item| item.line == 3),
            "expected greet definition, got: {definitions:?}"
        );

        let references = SPEC
            .references(&ctx, &request)
            .expect("resolve kotlin references");
        assert!(
            references.iter().any(|item| item.line == 5),
            "expected greet usage, got: {references:?}"
        );
        assert_eq!(
            references.iter().filter(|item| item.line == 5).count(),
            2,
            "expected both same-line greet usages, got: {references:?}"
        );
        assert!(
            references.iter().all(|item| item.line != 3),
            "definition line should be filtered when usage exists: {references:?}"
        );

        fs::remove_dir_all(root).ok();
    }
}
