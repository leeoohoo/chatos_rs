use std::path::Path;

const CONTROL_CHAR_RATIO_THRESHOLD: f64 = 0.02;

fn is_probably_text_payload(bytes: &[u8]) -> bool {
    if bytes.iter().any(|b| *b == 0) {
        return false;
    }
    if std::str::from_utf8(bytes).is_err() {
        return false;
    }
    if bytes.is_empty() {
        return true;
    }

    let control_count = bytes
        .iter()
        .filter(|b| {
            let value = **b;
            value < 0x20 && value != b'\n' && value != b'\r' && value != b'\t'
        })
        .count();
    let ratio = control_count as f64 / bytes.len() as f64;
    ratio <= CONTROL_CHAR_RATIO_THRESHOLD
}

pub fn should_render_text(path: &Path, bytes: &[u8], content_type: &str) -> bool {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    let file_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_lowercase();

    let is_text_ext = matches!(
        ext.as_str(),
        "rs" | "toml"
            | "lock"
            | "md"
            | "txt"
            | "json"
            | "yaml"
            | "yml"
            | "xml"
            | "html"
            | "htm"
            | "css"
            | "scss"
            | "less"
            | "js"
            | "jsx"
            | "ts"
            | "tsx"
            | "mjs"
            | "cjs"
            | "py"
            | "go"
            | "java"
            | "kt"
            | "swift"
            | "c"
            | "cc"
            | "cpp"
            | "h"
            | "hpp"
            | "cs"
            | "php"
            | "rb"
            | "sh"
            | "bash"
            | "zsh"
            | "ps1"
            | "bat"
            | "ini"
            | "conf"
            | "env"
            | "log"
            | "sql"
            | "vue"
            | "svelte"
            | "astro"
            | "dart"
            | "lua"
            | "r"
            | "m"
            | "mm"
            | "scala"
            | "gradle"
            | "make"
            | "cmake"
            | "dockerfile"
            | "properties"
            | "cfg"
            | "rc"
            | "proto"
            | "graphql"
    );

    let is_text_name = matches!(
        file_name.as_str(),
        "dockerfile"
            | "makefile"
            | "cmakelists.txt"
            | ".gitignore"
            | ".gitattributes"
            | ".editorconfig"
            | ".npmrc"
            | ".yarnrc"
            | ".yarnrc.yml"
            | ".prettierrc"
            | ".eslintrc"
            | ".babelrc"
            | ".env"
            | ".env.local"
            | ".env.development"
            | ".env.production"
    );

    let text_like_payload = is_probably_text_payload(bytes);
    let is_text_mime = content_type.starts_with("text/")
        || content_type == "application/json"
        || content_type == "application/xml"
        || content_type == "application/javascript"
        || content_type == "application/typescript";

    if is_text_mime || is_text_ext || is_text_name {
        return text_like_payload;
    }

    text_like_payload
}
