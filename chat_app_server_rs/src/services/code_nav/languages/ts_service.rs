use std::path::{Path, PathBuf};

use serde::Deserialize;
use tokio::process::Command;

use crate::services::code_nav::types::{
    DocumentSymbolItem, DocumentSymbolsResponse, NavCapabilities, NavLocation, NavPositionRequest,
    ProjectContext,
};

const TS_BRIDGE_SCRIPT_RELATIVE: &str = "scripts/code_nav/typescript_language_service.cjs";
const TYPESCRIPT_RUNTIME_RELATIVE: &str = "../chat_app/node_modules/typescript/lib/typescript.js";

#[derive(Debug, Clone, Copy)]
pub enum TsServiceMode {
    Definition,
    References,
    DocumentSymbols,
}

#[derive(Debug, Deserialize)]
struct TsServiceLocation {
    path: String,
    relative_path: String,
    line: usize,
    column: usize,
    end_line: usize,
    end_column: usize,
    preview: String,
    score: Option<f64>,
}

#[derive(Debug, Deserialize)]
struct TsServiceNavResponse {
    locations: Vec<TsServiceLocation>,
}

#[derive(Debug, Deserialize)]
struct TsServiceSymbol {
    name: String,
    kind: String,
    line: usize,
    column: usize,
    end_line: usize,
    end_column: usize,
}

#[derive(Debug, Deserialize)]
struct TsServiceSymbolsResponse {
    symbols: Vec<TsServiceSymbol>,
}

pub fn semantic_capabilities() -> NavCapabilities {
    let available = bridge_available();
    NavCapabilities {
        supports_definition: available,
        supports_references: available,
        supports_document_symbols: available,
    }
}

pub fn supports_typescript_file(file_path: &Path) -> bool {
    matches!(
        file_path.extension().and_then(|value| value.to_str()),
        Some("ts") | Some("tsx") | Some("mts") | Some("cts")
    )
}

pub fn supports_javascript_file(file_path: &Path) -> bool {
    matches!(
        file_path.extension().and_then(|value| value.to_str()),
        Some("js") | Some("jsx") | Some("mjs") | Some("cjs")
    )
}

pub async fn get_semantic_locations(
    mode: TsServiceMode,
    ctx: &ProjectContext,
    req: &NavPositionRequest,
) -> Result<Vec<NavLocation>, String> {
    let stdout = run_bridge(mode, ctx, Some(req)).await?;
    let response: TsServiceNavResponse = serde_json::from_str(&stdout)
        .map_err(|err| format!("解析 TypeScript 语义导航结果失败: {err}"))?;

    Ok(response
        .locations
        .into_iter()
        .map(|item| NavLocation {
            path: item.path,
            relative_path: item.relative_path,
            line: item.line,
            column: item.column,
            end_line: item.end_line,
            end_column: item.end_column,
            preview: item.preview,
            score: item.score.unwrap_or(1.0),
        })
        .collect())
}

pub async fn get_semantic_document_symbols(
    ctx: &ProjectContext,
) -> Result<DocumentSymbolsResponse, String> {
    let stdout = run_bridge(TsServiceMode::DocumentSymbols, ctx, None).await?;
    let response: TsServiceSymbolsResponse = serde_json::from_str(&stdout)
        .map_err(|err| format!("解析 TypeScript 文件符号结果失败: {err}"))?;

    Ok(DocumentSymbolsResponse {
        provider: ctx.language.clone(),
        language: ctx.language.clone(),
        mode: "semantic".to_string(),
        symbols: response
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
            .collect(),
    })
}

fn bridge_available() -> bool {
    bridge_script_path().exists() && typescript_runtime_path().exists()
}

fn bridge_script_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(TS_BRIDGE_SCRIPT_RELATIVE)
}

fn typescript_runtime_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(TYPESCRIPT_RUNTIME_RELATIVE)
}

async fn run_bridge(
    mode: TsServiceMode,
    ctx: &ProjectContext,
    req: Option<&NavPositionRequest>,
) -> Result<String, String> {
    let script_path = bridge_script_path();
    if !script_path.exists() {
        return Err(format!(
            "TypeScript 语义导航脚本不存在: {}",
            script_path.to_string_lossy()
        ));
    }

    let runtime_path = typescript_runtime_path();
    if !runtime_path.exists() {
        return Err(format!(
            "TypeScript 运行时不存在: {}",
            runtime_path.to_string_lossy()
        ));
    }

    let mut command = Command::new("node");
    command
        .arg(script_path)
        .arg("--mode")
        .arg(mode.as_str())
        .arg("--language")
        .arg(ctx.language.as_str())
        .arg("--project-root")
        .arg(ctx.root.to_string_lossy().to_string())
        .arg("--file")
        .arg(ctx.file_path.to_string_lossy().to_string())
        .current_dir(&ctx.root)
        .kill_on_drop(true);

    if let Some(req) = req {
        command
            .arg("--line")
            .arg(req.line.to_string())
            .arg("--column")
            .arg(req.column.to_string());
    }

    let output = command
        .output()
        .await
        .map_err(|err| format!("启动 TypeScript 语义导航进程失败: {err}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let message = if !stderr.is_empty() {
            stderr
        } else if !stdout.is_empty() {
            stdout
        } else {
            "TypeScript 语义导航进程执行失败".to_string()
        };
        return Err(message);
    }

    String::from_utf8(output.stdout)
        .map(|value| value.trim().to_string())
        .map_err(|err| format!("TypeScript 语义导航输出不是合法 UTF-8: {err}"))
}

impl TsServiceMode {
    fn as_str(&self) -> &'static str {
        match self {
            TsServiceMode::Definition => "definition",
            TsServiceMode::References => "references",
            TsServiceMode::DocumentSymbols => "document-symbols",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{get_semantic_locations, semantic_capabilities, TsServiceMode};
    use crate::services::code_nav::types::{NavPositionRequest, ProjectContext};
    use std::fs;
    use std::path::PathBuf;

    fn make_temp_project() -> PathBuf {
        let root =
            std::env::temp_dir().join(format!("code_nav_ts_service_test_{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&root).expect("create temp project");
        fs::write(
            root.join("tsconfig.json"),
            r#"{
  "compilerOptions": {
    "target": "ES2020",
    "module": "ESNext",
    "moduleResolution": "Node"
  }
}"#,
        )
        .expect("write tsconfig");
        fs::write(root.join("a.ts"), "export const foo = 1;\n").expect("write a.ts");
        fs::write(
            root.join("b.ts"),
            "import { foo } from './a';\n\nconst value = foo;\n",
        )
        .expect("write b.ts");
        root
    }

    #[tokio::test]
    async fn semantic_definition_resolves_exported_symbol() {
        if !semantic_capabilities().supports_definition {
            return;
        }

        let root = make_temp_project();
        let file_path = root.join("b.ts");
        let ctx = ProjectContext {
            root: root.clone(),
            file_path,
            relative_path: "b.ts".to_string(),
            language: "typescript".to_string(),
        };
        let req = NavPositionRequest {
            project_root: root.to_string_lossy().to_string(),
            file_path: ctx.file_path.to_string_lossy().to_string(),
            line: 3,
            column: 15,
        };

        let locations = get_semantic_locations(TsServiceMode::Definition, &ctx, &req)
            .await
            .expect("resolve definition");

        assert!(
            locations
                .iter()
                .any(|item| item.relative_path == "a.ts" && item.line == 1),
            "expected semantic definition to resolve to a.ts, got: {locations:?}"
        );

        fs::remove_dir_all(root).ok();
    }
}
