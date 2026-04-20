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

const CPP_IGNORED_DIRS: &[&str] = &[
    ".git",
    "node_modules",
    "dist",
    "build",
    "target",
    "out",
    ".idea",
    ".cache",
    ".vs",
];

const CPP_EXTENSIONS: &[&str] = &["cpp", "cc", "cxx", "hpp", "hh", "h", "hxx", "ipp"];
const CPP_PROJECT_FILES: &[&str] = &[
    "CMakeLists.txt",
    "Makefile",
    "makefile",
    "compile_commands.json",
    "meson.build",
    "conanfile.txt",
    "vcpkg.json",
];
const CPP_PROJECT_EXTENSIONS: &[&str] = &["vcxproj", "sln", "mk"];

static TYPE_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"^\s*(?:template\s*<[^>{}]+>\s*)?(class|struct|enum(?:\s+class)?|union)\s+([A-Za-z_][A-Za-z0-9_]*)\b",
    )
    .unwrap()
});
static NAMESPACE_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^\s*namespace\s+([A-Za-z_][A-Za-z0-9_]*)\b").unwrap());
static USING_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^\s*using\s+([A-Za-z_][A-Za-z0-9_]*)\s*=").unwrap());
static TYPEDEF_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^\s*typedef\b.+\b([A-Za-z_][A-Za-z0-9_]*)\s*;").unwrap());
static MACRO_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^\s*#\s*define\s+([A-Za-z_][A-Za-z0-9_]*)\b").unwrap());

const SPEC: BasicLanguageSpec = BasicLanguageSpec {
    provider_id: "cpp",
    language_id: "cpp",
    extensions: CPP_EXTENSIONS,
    ignored_dirs: CPP_IGNORED_DIRS,
    project_files: CPP_PROJECT_FILES,
    project_extensions: CPP_PROJECT_EXTENSIONS,
    analyze_file: analyze_cpp_file,
    classify_declaration: classify_cpp_declaration,
};

#[derive(Debug, Clone)]
struct CppTypeScope {
    body_depth: i32,
    start_line: usize,
}

#[derive(Default)]
pub struct CppCodeNavProvider;

#[axum::async_trait]
impl CodeNavProvider for CppCodeNavProvider {
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

fn analyze_cpp_file(path: &Path) -> Result<BasicFileAnalysis, String> {
    let content = fs::read_to_string(path).map_err(|err| err.to_string())?;
    let mut symbols = Vec::new();
    let mut type_stack: Vec<CppTypeScope> = Vec::new();
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
            } else if let Some(capture) = NAMESPACE_RE.captures(trimmed) {
                push_symbol(
                    &mut symbols,
                    raw_line,
                    capture[1].to_string(),
                    "namespace",
                    line_number,
                );
            } else if let Some(capture) = TYPE_RE.captures(trimmed) {
                let kind = cpp_type_kind(capture.get(1).map(|value| value.as_str()).unwrap_or(""));
                push_symbol(
                    &mut symbols,
                    raw_line,
                    capture[2].to_string(),
                    kind,
                    line_number,
                );
                let opens = count_char(&sanitized, '{') as i32;
                type_stack.push(CppTypeScope {
                    body_depth: brace_depth + if opens > 0 { opens } else { 1 },
                    start_line: line_number,
                });
            } else if let Some(capture) = USING_RE.captures(trimmed) {
                push_symbol(
                    &mut symbols,
                    raw_line,
                    capture[1].to_string(),
                    "type",
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
            } else if let Some(name) = extract_cpp_function_name(trimmed) {
                let kind = if !type_stack.is_empty()
                    || trimmed[..trimmed.find('(').unwrap_or(0)].contains("::")
                {
                    "method"
                } else {
                    "function"
                };
                push_symbol(&mut symbols, raw_line, name, kind, line_number);
            } else if let Some(name) = extract_cpp_variable_name(trimmed) {
                let kind = if type_stack.is_empty() {
                    "variable"
                } else {
                    "field"
                };
                push_symbol(&mut symbols, raw_line, name, kind, line_number);
            }
        }

        brace_depth += count_char(&sanitized, '{') as i32;
        brace_depth -= count_char(&sanitized, '}') as i32;
        while type_stack
            .last()
            .map(|scope| line_number > scope.start_line && brace_depth < scope.body_depth)
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

fn classify_cpp_declaration(line: &str, token: &str) -> Option<&'static str> {
    let mut in_block_comment = false;
    let sanitized = strip_c_style_comments(line, &mut in_block_comment);
    let trimmed = sanitized.trim();
    if let Some(capture) = MACRO_RE.captures(trimmed) {
        if capture.get(1).map(|value| value.as_str()) == Some(token) {
            return Some("macro");
        }
    }
    if let Some(capture) = NAMESPACE_RE.captures(trimmed) {
        if capture.get(1).map(|value| value.as_str()) == Some(token) {
            return Some("namespace");
        }
    }
    if let Some(capture) = TYPE_RE.captures(trimmed) {
        if capture.get(2).map(|value| value.as_str()) == Some(token) {
            return Some(cpp_type_kind(
                capture.get(1).map(|value| value.as_str()).unwrap_or(""),
            ));
        }
    }
    if let Some(capture) = USING_RE.captures(trimmed) {
        if capture.get(1).map(|value| value.as_str()) == Some(token) {
            return Some("type");
        }
    }
    if let Some(capture) = TYPEDEF_RE.captures(trimmed) {
        if capture.get(1).map(|value| value.as_str()) == Some(token) {
            return Some("typedef");
        }
    }
    if extract_cpp_function_name(trimmed).as_deref() == Some(token) {
        return Some(
            if trimmed[..trimmed.find('(').unwrap_or(0)].contains("::") {
                "method"
            } else {
                "function"
            },
        );
    }
    if extract_cpp_variable_name(trimmed).as_deref() == Some(token) {
        return Some("variable");
    }
    None
}

fn extract_cpp_function_name(line: &str) -> Option<String> {
    let trimmed = strip_cpp_template_prefix(line.trim_start());
    if is_cpp_non_declaration_line(trimmed) || trimmed.ends_with(',') {
        return None;
    }
    let open_paren = trimmed.find('(')?;
    let before_params = trimmed[..open_paren].trim_end();
    if before_params.contains('=') || before_params.contains("->*") || before_params.contains('.') {
        return None;
    }
    let name = last_identifier(before_params)?;
    if is_cpp_keyword(&name) {
        return None;
    }
    let prefix = before_params.strip_suffix(name.as_str())?.trim_end();
    if prefix.is_empty() || is_cpp_non_declaration_line(prefix.trim_start()) {
        return None;
    }
    if let Some(close_end) = find_balanced_end(&trimmed[open_paren..], '(', ')') {
        let suffix = trimmed
            .get(open_paren + close_end..)
            .unwrap_or("")
            .trim_start();
        if !(suffix.is_empty()
            || suffix.starts_with('{')
            || suffix.starts_with(';')
            || suffix.starts_with("const")
            || suffix.starts_with("noexcept")
            || suffix.starts_with("override")
            || suffix.starts_with("final")
            || suffix.starts_with("->")
            || suffix.starts_with("= default")
            || suffix.starts_with("= delete"))
        {
            return None;
        }
    }
    Some(name)
}

fn extract_cpp_variable_name(line: &str) -> Option<String> {
    let trimmed = line.trim();
    if !trimmed.ends_with(';')
        || trimmed.contains('(')
        || is_cpp_non_declaration_line(trimmed)
        || trimmed.starts_with("using ")
    {
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
    if prefix.is_empty() || is_cpp_keyword(&name) {
        None
    } else {
        Some(name)
    }
}

fn strip_cpp_template_prefix(line: &str) -> &str {
    let trimmed = line.trim_start();
    if !trimmed.starts_with("template") {
        return trimmed;
    }
    let rest = trimmed.trim_start_matches("template").trim_start();
    if !rest.starts_with('<') {
        return trimmed;
    }
    let Some(end) = find_balanced_end(rest, '<', '>') else {
        return trimmed;
    };
    rest.get(end..).unwrap_or("").trim_start()
}

fn cpp_type_kind(value: &str) -> &'static str {
    match value {
        "class" => "class",
        "struct" => "struct",
        "union" => "type",
        _ if value.starts_with("enum") => "enum",
        _ => "type",
    }
}

fn is_cpp_non_declaration_line(line: &str) -> bool {
    [
        "#",
        "return ",
        "co_return ",
        "if ",
        "for ",
        "while ",
        "switch ",
        "case ",
        "else",
        "do ",
        "sizeof",
        "using ",
        "typedef struct {",
        "typedef enum {",
        "typedef union {",
        "class ",
        "struct ",
        "enum ",
        "namespace ",
    ]
    .iter()
    .any(|prefix| line.starts_with(prefix))
}

fn is_cpp_keyword(value: &str) -> bool {
    matches!(
        value,
        "if" | "for"
            | "while"
            | "switch"
            | "case"
            | "return"
            | "sizeof"
            | "class"
            | "struct"
            | "enum"
            | "union"
            | "namespace"
            | "template"
            | "operator"
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
    use super::{analyze_cpp_file, classify_cpp_declaration, CppCodeNavProvider};
    use crate::services::code_nav::CodeNavProvider;
    use std::fs;
    use std::path::PathBuf;

    fn make_temp_cpp_project() -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "code_nav_cpp_provider_test_{}",
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(root.join("src")).expect("create source dir");
        fs::write(root.join("CMakeLists.txt"), "project(demo CXX)\n").expect("write cmake");
        root
    }

    #[test]
    fn cpp_document_symbols_detect_types_methods_and_functions() {
        let root = make_temp_cpp_project();
        let path = root.join("src/sample.cpp");
        fs::write(
            &path,
            r#"#define BUFFER_SIZE 128

namespace demo {
class Sample {
public:
    void greet();
    int value;
};

void Sample::greet() {
}
}
"#,
        )
        .expect("write cpp file");

        let analysis = analyze_cpp_file(&path).expect("analyze cpp file");
        let names: Vec<(String, String)> = analysis
            .symbols
            .iter()
            .map(|item| (item.name.clone(), item.kind.clone()))
            .collect();

        assert!(names.contains(&(String::from("BUFFER_SIZE"), String::from("macro"))));
        assert!(names.contains(&(String::from("demo"), String::from("namespace"))));
        assert!(names.contains(&(String::from("Sample"), String::from("class"))));
        assert!(names.contains(&(String::from("greet"), String::from("method"))));
        assert!(names.contains(&(String::from("value"), String::from("field"))));
        assert_eq!(classify_cpp_declaration("return greet();", "greet"), None);

        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn cpp_provider_supports_common_cpp_files() {
        let provider = CppCodeNavProvider;
        assert!(provider.supports_file(PathBuf::from("main.cpp").as_path()));
        assert!(provider.supports_file(PathBuf::from("sample.hpp").as_path()));
        assert!(provider.supports_file(PathBuf::from("legacy.h").as_path()));
    }
}
