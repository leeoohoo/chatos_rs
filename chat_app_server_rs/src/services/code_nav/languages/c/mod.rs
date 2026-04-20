use std::fs;
use std::path::Path;

use once_cell::sync::Lazy;
use regex::Regex;

use crate::services::code_nav::languages::basic::{
    count_char, find_balanced_end, find_column, last_identifier, make_symbol,
    strip_c_style_comments, BasicFileAnalysis, BasicLanguageSpec, BasicSymbol,
};
use crate::services::code_nav::types::{
    DocumentSymbolsRequest, DocumentSymbolsResponse, NavCapabilities, NavLocation,
    NavPositionRequest, ProjectContext,
};
use crate::services::code_nav::CodeNavProvider;

const C_IGNORED_DIRS: &[&str] = &[
    ".git",
    "node_modules",
    "dist",
    "build",
    "target",
    "out",
    ".idea",
    ".cache",
];

const C_EXTENSIONS: &[&str] = &["c"];
const C_PROJECT_FILES: &[&str] = &[
    "CMakeLists.txt",
    "Makefile",
    "makefile",
    "compile_commands.json",
    "meson.build",
    "configure.ac",
];
const C_PROJECT_EXTENSIONS: &[&str] = &["mk"];

static TYPE_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^\s*(?:typedef\s+)?(struct|enum|union)\s+([A-Za-z_][A-Za-z0-9_]*)\b").unwrap()
});
static TYPEDEF_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^\s*typedef\b.+\b([A-Za-z_][A-Za-z0-9_]*)\s*;").unwrap());
static MACRO_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^\s*#\s*define\s+([A-Za-z_][A-Za-z0-9_]*)\b").unwrap());

const SPEC: BasicLanguageSpec = BasicLanguageSpec {
    provider_id: "c",
    language_id: "c",
    extensions: C_EXTENSIONS,
    ignored_dirs: C_IGNORED_DIRS,
    project_files: C_PROJECT_FILES,
    project_extensions: C_PROJECT_EXTENSIONS,
    analyze_file: analyze_c_file,
    classify_declaration: classify_c_declaration,
};

#[derive(Default)]
pub struct CCodeNavProvider;

#[axum::async_trait]
impl CodeNavProvider for CCodeNavProvider {
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

fn analyze_c_file(path: &Path) -> Result<BasicFileAnalysis, String> {
    let content = fs::read_to_string(path).map_err(|err| err.to_string())?;
    let mut symbols = Vec::new();
    let mut brace_depth = 0i32;
    let mut in_block_comment = false;

    for (index, raw_line) in content.lines().enumerate() {
        let line_number = index + 1;
        let sanitized = strip_c_style_comments(raw_line, &mut in_block_comment);
        let trimmed = sanitized.trim();

        if !trimmed.is_empty() {
            if let Some(capture) = MACRO_RE.captures(trimmed) {
                push_symbol(
                    &mut symbols,
                    raw_line,
                    capture[1].to_string(),
                    "macro",
                    line_number,
                );
            } else if brace_depth == 0 {
                if let Some(capture) = TYPE_RE.captures(trimmed) {
                    push_symbol(
                        &mut symbols,
                        raw_line,
                        capture[2].to_string(),
                        capture.get(1).map(|value| value.as_str()).unwrap_or("type"),
                        line_number,
                    );
                } else if let Some(capture) = TYPEDEF_RE.captures(trimmed) {
                    push_symbol(
                        &mut symbols,
                        raw_line,
                        capture[1].to_string(),
                        "typedef",
                        line_number,
                    );
                } else if let Some(name) = extract_c_function_name(trimmed) {
                    push_symbol(&mut symbols, raw_line, name, "function", line_number);
                } else if let Some(name) = extract_c_variable_name(trimmed) {
                    push_symbol(&mut symbols, raw_line, name, "variable", line_number);
                }
            }
        }

        brace_depth += count_char(&sanitized, '{') as i32;
        brace_depth -= count_char(&sanitized, '}') as i32;
    }

    symbols.sort_by(|left, right| {
        left.line
            .cmp(&right.line)
            .then(left.column.cmp(&right.column))
            .then(left.name.cmp(&right.name))
    });

    Ok(BasicFileAnalysis { symbols })
}

fn classify_c_declaration(line: &str, token: &str) -> Option<&'static str> {
    let mut in_block_comment = false;
    let trimmed = strip_c_style_comments(line, &mut in_block_comment)
        .trim()
        .to_string();
    if let Some(capture) = MACRO_RE.captures(&trimmed) {
        if capture.get(1).map(|value| value.as_str()) == Some(token) {
            return Some("macro");
        }
    }
    if let Some(capture) = TYPE_RE.captures(&trimmed) {
        if capture.get(2).map(|value| value.as_str()) == Some(token) {
            return capture.get(1).map(|value| match value.as_str() {
                "struct" => "struct",
                "enum" => "enum",
                "union" => "type",
                _ => "type",
            });
        }
    }
    if let Some(capture) = TYPEDEF_RE.captures(&trimmed) {
        if capture.get(1).map(|value| value.as_str()) == Some(token) {
            return Some("typedef");
        }
    }
    if extract_c_function_name(&trimmed).as_deref() == Some(token) {
        return Some("function");
    }
    if extract_c_variable_name(&trimmed).as_deref() == Some(token) {
        return Some("variable");
    }
    None
}

fn extract_c_function_name(line: &str) -> Option<String> {
    let trimmed = line.trim_start();
    if is_c_non_declaration_line(trimmed) || trimmed.ends_with(',') {
        return None;
    }
    let open_paren = trimmed.find('(')?;
    let before_params = trimmed[..open_paren].trim_end();
    if before_params.contains('=') || before_params.contains("->") || before_params.contains('.') {
        return None;
    }
    let name = last_identifier(before_params)?;
    if is_c_keyword(&name) {
        return None;
    }
    let prefix = before_params.strip_suffix(name.as_str())?.trim_end();
    if prefix.is_empty() || is_c_non_declaration_line(prefix.trim_start()) {
        return None;
    }
    if let Some(close_end) = find_balanced_end(&trimmed[open_paren..], '(', ')') {
        let suffix = trimmed
            .get(open_paren + close_end..)
            .unwrap_or("")
            .trim_start();
        if !(suffix.is_empty() || suffix.starts_with('{') || suffix.starts_with(';')) {
            return None;
        }
    }
    Some(name)
}

fn extract_c_variable_name(line: &str) -> Option<String> {
    let trimmed = line.trim();
    if !trimmed.ends_with(';') || trimmed.contains('(') || is_c_non_declaration_line(trimmed) {
        return None;
    }
    let declaration_head = trimmed
        .trim_end_matches(';')
        .split('=')
        .next()
        .unwrap_or("")
        .trim_end();
    let name = last_identifier(declaration_head)?;
    let prefix = declaration_head.strip_suffix(name.as_str())?.trim_end();
    if prefix.is_empty() || is_c_keyword(&name) {
        None
    } else {
        Some(name)
    }
}

fn is_c_non_declaration_line(line: &str) -> bool {
    [
        "#",
        "return ",
        "if ",
        "for ",
        "while ",
        "switch ",
        "case ",
        "else",
        "do ",
        "sizeof",
        "typedef struct {",
        "typedef enum {",
        "typedef union {",
    ]
    .iter()
    .any(|prefix| line.starts_with(prefix))
}

fn is_c_keyword(value: &str) -> bool {
    matches!(
        value,
        "if" | "for"
            | "while"
            | "switch"
            | "case"
            | "return"
            | "sizeof"
            | "struct"
            | "enum"
            | "union"
            | "typedef"
    )
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

#[cfg(test)]
mod tests {
    use super::{analyze_c_file, classify_c_declaration, CCodeNavProvider};
    use crate::services::code_nav::CodeNavProvider;
    use std::fs;
    use std::path::PathBuf;

    fn make_temp_c_project() -> PathBuf {
        let root =
            std::env::temp_dir().join(format!("code_nav_c_provider_test_{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(root.join("src")).expect("create source dir");
        fs::write(root.join("CMakeLists.txt"), "project(demo C)\n").expect("write cmake");
        root
    }

    #[test]
    fn c_document_symbols_detect_functions_types_and_macros() {
        let root = make_temp_c_project();
        let path = root.join("src/sample.c");
        fs::write(
            &path,
            r#"#define BUFFER_SIZE 128

struct Sample {
    int value;
};

static int add(int left, int right) {
    return left + right;
}

int global_value;
"#,
        )
        .expect("write c file");

        let analysis = analyze_c_file(&path).expect("analyze c file");
        let names: Vec<(String, String)> = analysis
            .symbols
            .iter()
            .map(|item| (item.name.clone(), item.kind.clone()))
            .collect();

        assert!(names.contains(&(String::from("BUFFER_SIZE"), String::from("macro"))));
        assert!(names.contains(&(String::from("Sample"), String::from("struct"))));
        assert!(names.contains(&(String::from("add"), String::from("function"))));
        assert!(names.contains(&(String::from("global_value"), String::from("variable"))));
        assert_eq!(
            classify_c_declaration("return add(left, right);", "add"),
            None
        );

        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn c_provider_supports_c_files() {
        let provider = CCodeNavProvider;
        assert!(provider.supports_file(PathBuf::from("main.c").as_path()));
        assert!(!provider.supports_file(PathBuf::from("main.cpp").as_path()));
    }
}
