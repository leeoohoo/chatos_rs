use std::path::Path;

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

    let utf8_ok = std::str::from_utf8(bytes).is_ok();
    let is_text_mime = content_type.starts_with("text/")
        || content_type == "application/json"
        || content_type == "application/xml"
        || content_type == "application/javascript"
        || content_type == "application/typescript";

    utf8_ok && (is_text_mime || is_text_ext || is_text_name)
}
