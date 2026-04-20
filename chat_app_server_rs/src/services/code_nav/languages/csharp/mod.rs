use std::fs;
use std::path::Path;

use once_cell::sync::Lazy;
use regex::Regex;

use crate::services::code_nav::languages::basic::{
    count_char, find_balanced_end, find_column, last_identifier, make_symbol,
    strip_c_style_comments, strip_leading_attributes, BasicFileAnalysis, BasicLanguageSpec,
    BasicSymbol,
};
use crate::services::code_nav::types::{
    DocumentSymbolsRequest, DocumentSymbolsResponse, NavCapabilities, NavLocation,
    NavPositionRequest, ProjectContext,
};
use crate::services::code_nav::CodeNavProvider;

const CSHARP_IGNORED_DIRS: &[&str] = &[
    ".git",
    "node_modules",
    "dist",
    "build",
    "target",
    "out",
    ".idea",
    ".vs",
    "bin",
    "obj",
];

const CSHARP_EXTENSIONS: &[&str] = &["cs"];
const CSHARP_PROJECT_FILES: &[&str] = &[
    "global.json",
    "Directory.Build.props",
    "Directory.Build.targets",
    "NuGet.config",
];
const CSHARP_PROJECT_EXTENSIONS: &[&str] = &["csproj", "sln"];

static TYPE_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"^\s*(?:(?:public|private|protected|internal|static|abstract|sealed|partial|readonly|unsafe|new)\s+)*(class|interface|struct|enum|record(?:\s+(?:class|struct))?)\s+([A-Za-z_][A-Za-z0-9_]*)\b",
    )
    .unwrap()
});

const SPEC: BasicLanguageSpec = BasicLanguageSpec {
    provider_id: "csharp",
    language_id: "csharp",
    extensions: CSHARP_EXTENSIONS,
    ignored_dirs: CSHARP_IGNORED_DIRS,
    project_files: CSHARP_PROJECT_FILES,
    project_extensions: CSHARP_PROJECT_EXTENSIONS,
    analyze_file: analyze_csharp_file,
    classify_declaration: classify_csharp_declaration,
};

#[derive(Debug, Clone)]
struct CSharpTypeScope {
    name: String,
    body_depth: i32,
    start_line: usize,
}

#[derive(Default)]
pub struct CSharpCodeNavProvider;

#[axum::async_trait]
impl CodeNavProvider for CSharpCodeNavProvider {
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

fn analyze_csharp_file(path: &Path) -> Result<BasicFileAnalysis, String> {
    let content = fs::read_to_string(path).map_err(|err| err.to_string())?;
    let mut symbols = Vec::new();
    let mut type_stack: Vec<CSharpTypeScope> = Vec::new();
    let mut brace_depth = 0i32;
    let mut in_block_comment = false;

    for (index, raw_line) in content.lines().enumerate() {
        let line_number = index + 1;
        let sanitized = strip_c_style_comments(raw_line, &mut in_block_comment);
        let trimmed = strip_leading_attributes(sanitized.trim(), '[', ']');

        if !trimmed.is_empty() {
            if let Some(capture) = TYPE_RE.captures(trimmed) {
                let name = capture[2].to_string();
                let kind =
                    csharp_type_kind(capture.get(1).map(|value| value.as_str()).unwrap_or(""));
                push_symbol(&mut symbols, raw_line, name.clone(), kind, line_number);
                let opens = count_char(&sanitized, '{') as i32;
                type_stack.push(CSharpTypeScope {
                    name,
                    body_depth: brace_depth + if opens > 0 { opens } else { 1 },
                    start_line: line_number,
                });
            } else if let Some(current_type) = type_stack.last() {
                if let Some((name, kind)) =
                    extract_csharp_method_signature(trimmed, &current_type.name)
                {
                    push_symbol(&mut symbols, raw_line, name, kind, line_number);
                } else if let Some(name) = extract_csharp_property_name(trimmed) {
                    push_symbol(&mut symbols, raw_line, name, "property", line_number);
                } else if let Some(name) = extract_csharp_field_name(trimmed) {
                    push_symbol(&mut symbols, raw_line, name, "field", line_number);
                }
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

fn classify_csharp_declaration(line: &str, token: &str) -> Option<&'static str> {
    let mut in_block_comment = false;
    let sanitized = strip_c_style_comments(line, &mut in_block_comment);
    let trimmed = strip_leading_attributes(sanitized.trim(), '[', ']');
    if let Some(capture) = TYPE_RE.captures(trimmed) {
        if capture.get(2).map(|value| value.as_str()) == Some(token) {
            return Some(csharp_type_kind(
                capture.get(1).map(|value| value.as_str()).unwrap_or(""),
            ));
        }
    }
    if let Some((name, kind)) = extract_csharp_method_signature(trimmed, "") {
        if name == token {
            return Some(if kind == "constructor" {
                "constructor"
            } else {
                "method"
            });
        }
    }
    if extract_csharp_property_name(trimmed).as_deref() == Some(token) {
        return Some("property");
    }
    if extract_csharp_field_name(trimmed).as_deref() == Some(token) {
        return Some("field");
    }
    None
}

fn extract_csharp_method_signature(
    line: &str,
    current_type_name: &str,
) -> Option<(String, &'static str)> {
    let line = strip_leading_attributes(line.trim(), '[', ']');
    if line.is_empty() || is_csharp_non_method_line(line) {
        return None;
    }
    let open_paren = line.find('(')?;
    let before_params = line[..open_paren].trim_end();
    if before_params.contains('=') {
        return None;
    }
    let name = last_identifier(before_params)?;
    if is_csharp_keyword(&name) {
        return None;
    }
    if let Some(close_end) = find_balanced_end(&line[open_paren..], '(', ')') {
        let suffix = line
            .get(open_paren + close_end..)
            .unwrap_or("")
            .trim_start();
        if !(suffix.is_empty()
            || suffix.starts_with('{')
            || suffix.starts_with(';')
            || suffix.starts_with("=>")
            || suffix.starts_with("where "))
        {
            return None;
        }
    }

    if !current_type_name.is_empty() && name == current_type_name {
        return Some((name, "constructor"));
    }

    let prefix = before_params.strip_suffix(name.as_str())?.trim_end();
    if !has_csharp_return_type(prefix) {
        return None;
    }

    Some((name, "method"))
}

fn extract_csharp_property_name(line: &str) -> Option<String> {
    let line = strip_leading_attributes(line.trim(), '[', ']');
    if !line.contains('{') || line.contains('(') || is_csharp_non_method_line(line) {
        return None;
    }
    let before_body = line.split('{').next().unwrap_or("").trim_end();
    let name = last_identifier(before_body)?;
    if is_csharp_keyword(&name) {
        return None;
    }
    let prefix = before_body.strip_suffix(name.as_str())?.trim_end();
    if has_csharp_return_type(prefix) {
        Some(name)
    } else {
        None
    }
}

fn extract_csharp_field_name(line: &str) -> Option<String> {
    let line = strip_leading_attributes(line.trim(), '[', ']');
    if !line.ends_with(';') || line.contains('(') || is_csharp_non_method_line(line) {
        return None;
    }
    let declaration_head = line
        .trim_end_matches(';')
        .split('=')
        .next()
        .unwrap_or("")
        .trim_end();
    if declaration_head.contains('.') {
        return None;
    }
    let name = last_identifier(declaration_head)?;
    if is_csharp_keyword(&name) {
        return None;
    }
    let prefix = declaration_head.strip_suffix(name.as_str())?.trim_end();
    if has_csharp_return_type(prefix) {
        Some(name)
    } else {
        None
    }
}

fn has_csharp_return_type(prefix: &str) -> bool {
    let mut rest = prefix.trim();
    loop {
        let Some((word, after_word)) = split_first_word(rest) else {
            break;
        };
        if !is_csharp_modifier(word) {
            break;
        }
        rest = after_word.trim_start();
    }
    !rest.is_empty()
}

fn split_first_word(value: &str) -> Option<(&str, &str)> {
    let value = value.trim_start();
    let mut end = None;
    for (index, ch) in value.char_indices() {
        if index == 0 && !(ch == '_' || ch.is_alphabetic()) {
            return None;
        }
        if ch == '_' || ch.is_alphanumeric() {
            end = Some(index + ch.len_utf8());
        } else {
            break;
        }
    }
    let end = end?;
    Some((&value[..end], &value[end..]))
}

fn is_csharp_modifier(value: &str) -> bool {
    matches!(
        value,
        "public"
            | "private"
            | "protected"
            | "internal"
            | "static"
            | "abstract"
            | "virtual"
            | "override"
            | "sealed"
            | "partial"
            | "extern"
            | "unsafe"
            | "new"
            | "async"
            | "readonly"
            | "ref"
    )
}

fn is_csharp_non_method_line(line: &str) -> bool {
    [
        "return ",
        "throw ",
        "if ",
        "for ",
        "foreach ",
        "while ",
        "switch ",
        "case ",
        "using ",
        "namespace ",
        "class ",
        "interface ",
        "struct ",
        "enum ",
        "record ",
    ]
    .iter()
    .any(|prefix| line.starts_with(prefix))
}

fn is_csharp_keyword(value: &str) -> bool {
    matches!(
        value,
        "if" | "for"
            | "foreach"
            | "while"
            | "switch"
            | "case"
            | "return"
            | "throw"
            | "new"
            | "class"
            | "interface"
            | "struct"
            | "enum"
            | "record"
    )
}

fn csharp_type_kind(value: &str) -> &'static str {
    match value {
        "class" => "class",
        "interface" => "interface",
        "struct" => "struct",
        "enum" => "enum",
        _ if value.starts_with("record") => "record",
        _ => "type",
    }
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
    use super::{analyze_csharp_file, classify_csharp_declaration, CSharpCodeNavProvider};
    use crate::services::code_nav::CodeNavProvider;
    use std::fs;
    use std::path::PathBuf;

    fn make_temp_csharp_project() -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "code_nav_csharp_provider_test_{}",
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(root.join("App")).expect("create source dir");
        fs::write(
            root.join("App.csproj"),
            "<Project Sdk=\"Microsoft.NET.Sdk\" />\n",
        )
        .expect("write csproj");
        root
    }

    #[test]
    fn csharp_document_symbols_detect_types_methods_properties_and_fields() {
        let root = make_temp_csharp_project();
        let path = root.join("App/Sample.cs");
        fs::write(
            &path,
            r#"namespace Demo;

public class Sample
{
    private readonly string name;

    [HttpGet]
    public string Name { get; set; }

    public Sample(string name)
    {
        this.name = name;
    }

    public string Greet(string who) => name + who;
}
"#,
        )
        .expect("write csharp file");

        let analysis = analyze_csharp_file(&path).expect("analyze csharp file");
        let names: Vec<(String, String)> = analysis
            .symbols
            .iter()
            .map(|item| (item.name.clone(), item.kind.clone()))
            .collect();

        assert!(names.contains(&(String::from("Sample"), String::from("class"))));
        assert!(names.contains(&(String::from("name"), String::from("field"))));
        assert!(names.contains(&(String::from("Name"), String::from("property"))));
        assert!(names.contains(&(String::from("Sample"), String::from("constructor"))));
        assert!(names.contains(&(String::from("Greet"), String::from("method"))));
        assert_eq!(classify_csharp_declaration("[HttpGet]", "HttpGet"), None);

        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn csharp_provider_supports_cs_files() {
        let provider = CSharpCodeNavProvider;
        assert!(provider.supports_file(PathBuf::from("Program.cs").as_path()));
        assert!(!provider.supports_file(PathBuf::from("Program.java").as_path()));
    }
}
