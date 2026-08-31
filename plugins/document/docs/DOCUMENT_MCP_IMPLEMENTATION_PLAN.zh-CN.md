# ChatOS Document MCP 完整实施方案

> 状态：实施中（P0–P3 核心能力已落地）
> 编写日期：2026-08-21
> 当前目录：`plugins/document`
> 暂定 npm 包名：`@chatos/document-mcp`（需确认 npm scope 所有权）
> 暂定可执行命令：`chatos-document-mcp`

## 当前实施进度（2026-08-21）

已完成：

- npm/TypeScript/stdio MCP 骨架、自包含 esbuild bundle 和 ChatOS `schemaVersion: 3` Manifest。
- ChatOS stdio Plugin MCP 的 `CHATOS_WORKSPACE` 安全注入、权限快照绑定和 Workspace 重绑定失效检查。
- DOCX/XLSX/PPTX/PDF 的安全检查与有界文本提取。
- OfficeCLI `v1.0.144` 六个平台二进制固定、SHA-256 验证、离线运行和高层 operation 白名单。
- Office 创建与批量编辑，以及 PDF 合并、抽页、重排、旋转、元数据和 AcroForm 读写。
- Word 标题、列表、表格、段落格式，Excel 有界范围读取、矩阵写入和工作表管理，以及 PowerPoint 幻灯片重排、删除和基础属性设置。
- `document_render`：PDFium WASM PDF 渲染、OfficeCLI HTML 渲染、逐页 PNG、render manifest、50 页和总像素限制。
- `document_convert`：完全离线的 DOCX/XLSX/PPTX → 图像型 PDF；DOCX 长图拆页、PPTX 按幻灯片渲染、XLSX 按工作表使用范围裁剪，并明确返回 `conversionMode: raster`、`searchableText: false`、`layoutFidelity: preview`。
- `document_validate`：结构检查、底层引擎重新打开及可选页面渲染验证。
- CycloneDX 1.5 SBOM、基于 esbuild 实际生产输入生成的完整 npm 许可证文本包、OfficeCLI/PDFium vendor 来源清单，以及测试和打包阶段的漂移校验。
- PDFium `chromium/7243` 传递组件工程审计已完成：固定 16 个实际链接或工具链运行时组件的版本/commit、源码 URL、许可证原文、SHA-256、二进制/构建证据和必须传递的 attribution；Skia、V8、XFA、PartitionAlloc、libpng 的排除依据亦已记录。专用清单位于 `PDFIUM_THIRD_PARTY_NOTICES.txt` 和 `vendor/pdfium-third-party-v7243.json`。
- 项目许可证已确定为 Apache-2.0，根目录 `LICENSE`/`NOTICE` 已加入候选包，npm package 已取消 `private` 并配置为 public/provenance 发布。
- packed artifact 离线冒烟：安全解包候选 `.tgz`，不安装依赖直接启动包内 MCP，核对 15 个工具，并用包内 OfficeCLI 创建真实 DOCX Artifact。
- MCP 协议、路径逃逸、四类格式、真实 Office/PDF 修改、渲染和 packed artifact 自动测试。

尚未完成：

- 高保真、可搜索文本的 Office 导出 PDF、更多 PowerPoint 高层编辑 operation 和复杂模板能力。当前图像型 PDF 转换已经完成；固定的 OfficeCLI `v1.0.144` 会把原生 PDF 导出委托给未随 release 提供的 exporter plugin，因此高保真模式仍需选择、固定并审核可离线打包的 exporter，不能在运行时安装。
- Windows/Linux packed artifact 的真实设备渲染回归和 golden 像素回归。PDFium `chromium/7243` 的传递第三方 notices 收集、SBOM 子组件建模和打包门禁已完成；该工程清单不是法律意见，发布者仍可自行决定是否另行聘请律师复核。
- Marketplace preview/stable 发布；仍需确认 npm publisher，以及发布者是否拥有 `@chatos` scope，从而决定保留 `@chatos/document-mcp` 或改用其他公开包名。

当前自动验收结果：Document MCP 8/8 测试通过（包含 DOCX 两页、XLSX 两工作表和 PPTX 选择顺序的 PDF 转换）；CycloneDX 1.5 SBOM 官方 Schema 校验通过并记录 40 个实际随包/链接运行时组件，其中 PDFium 子清单为 16 个；`npm audit --omit=dev` 为 0 个已知漏洞；packed `.tgz` 无安装依赖启动成功，列出 15 个工具并创建有效 DOCX；`npm publish --dry-run` 已按 public access 通过；ChatOS plugin 相关 Rust 测试 75 通过、1 ignored；六个平台 OfficeCLI、PDFium WASM 和全部 PDFium 第三方许可证文件均通过 SHA-256 校验；最新 universal npm 包 78,248,944 bytes、解包 215,402,408 bytes、44 个条目，SHA-1 为 `4bc4e3134e51abfa248fec741415b312e27da435`，低于 ChatOS 的 256 MiB、768 MiB 和 8,192 条目限制。

## 1. 项目目标

开发一个可通过 npm 安装、以 stdio 方式运行，并能上架 ChatOS Plugin Marketplace 的本地文件处理 MCP。首期支持：

- Word：`.docx`
- Excel：`.xlsx`
- PowerPoint：`.pptx`
- PDF：`.pdf`

产品需要同时满足四个目标：

1. 能读取、创建、编辑、转换、渲染和验证常见办公文件。
2. 对模型暴露少量、稳定、类型化的 MCP 工具，而不是任意命令执行入口。
3. 所有运行代码、依赖和原生二进制都包含在一个经过审核的 npm `.tgz` 中，安装后不再下载代码。
4. 文件访问严格受 ChatOS 授权的 Workspace 和 Artifact 目录约束，不能访问用户机器上的任意路径。

## 2. 范围与非目标

### 2.1 第一版范围

- 检查文件类型、元数据、页数、工作表、幻灯片和基础结构。
- 提取 Word、Excel、PowerPoint、PDF 的文本和结构化内容。
- 新建及批量编辑 `.docx`、`.xlsx`、`.pptx`。
- 导出 Office 文件为 PDF。
- 把 Office/PDF 页面渲染为 PNG，供模型或测试流程进行视觉检查。
- PDF 合并、拆分、抽页、旋转、表单读取和填写。
- 所有写操作默认生成新 Artifact，不直接覆盖原文件。
- macOS、Windows、Linux 的 x64/arm64 支持，以最终打包体积验证结果为准。

### 2.2 第一版明确不做

- 不支持旧二进制格式 `.doc`、`.xls`、`.ppt`。
- 不支持带宏格式 `.docm`、`.xlsm`、`.pptm`，也不执行任何宏。
- 不支持密码保护或加密文件。
- 不承诺 100% 还原 Microsoft Office 的高级排版、字体、SmartArt、动画和复杂公式计算结果。
- 不在 MCP 内调用云端 Office API，不上传用户文件。
- 不暴露 shell、任意 OfficeCLI 命令、任意脚本或任意路径访问工具。
- OCR、扫描件理解、修订模式和复杂批注工作流放到第二阶段。

## 3. 调研结论与技术决策

### 3.1 ChatOS 的交付约束

ChatOS 当前只接受完整、自包含的 npm package：

- 使用 `npm pack` 生成 `.tgz`。
- `package.json.bin` 必须声明 MCP stdio 入口。
- Plugin Manifest 使用 `schemaVersion: 3`，建议放在包根目录 `chatos.plugin.json`。
- 不允许运行时使用 `npx package@latest`。
- 不允许依赖 `postinstall`、`install` 等脚本下载未审核代码。
- 客户端安全解包后直接运行包内入口，不会替插件执行普通的依赖安装流程。

当前代码中的硬限制：

| 项目 | 限制 |
| --- | ---: |
| npm `.tgz` 大小 | 256 MiB |
| 单文件大小 | 128 MiB |
| 解包后总大小 | 768 MiB |
| 包内条目数 | 8,192 |
| MCP 工具数 | 200 |
| 工具快照 JSON | 512 KiB |
| 单次工具默认超时上限 | 120 秒 |

因此，本项目必须在构建期完成 TypeScript 打包、第三方二进制下载或编译、哈希校验、许可证归档和最终 `npm pack` 验证。

### 3.2 Office 引擎选择

推荐以 [iOfficeAI/OfficeCLI](https://github.com/iOfficeAI/OfficeCLI) 作为 `.docx/.xlsx/.pptx` 底层引擎，理由是：

- Apache-2.0 许可，适合在满足许可证义务的前提下再分发。
- 同时覆盖 Word、Excel 和 PowerPoint，减少三套编辑引擎之间的行为差异。
- 支持创建、读取、编辑、模板、图表、公式以及截图/渲染相关能力。
- 不要求本机安装 Microsoft Office。
- 提供多平台原生实现，适合本地执行。

但不能直接把 OfficeCLI 自带的“任意 CLI 命令”MCP 暴露给 ChatOS。Document MCP 必须在其上增加类型化工具、安全路径解析、参数白名单、超时、输出控制和审计边界。

OfficeCLI 的 npm 包如果在安装期下载二进制，不符合 ChatOS 自包含包规则。本项目应固定 OfficeCLI 版本或 commit，并在发布构建阶段把所需平台二进制打入 `.tgz`。

运行 OfficeCLI 子进程时至少设置：

```text
OFFICECLI_SKIP_UPDATE=1
OFFICECLI_NO_AUTO_INSTALL=1
OFFICECLI_NO_AUTO_RESIDENT=1
```

同时使用 `shell: false`、固定可执行文件路径、固定参数表和受控工作目录，禁止把用户文本拼接成命令字符串。

### 3.3 PDF 引擎选择

PDF 拆成三个职责实现：

| 职责 | 推荐实现 | 用途 |
| --- | --- | --- |
| 结构修改 | [`pdf-lib`](https://github.com/Hopding/pdf-lib) | 合并、拆分、旋转、元数据、AcroForm |
| 文本提取 | [`pdfjs-dist`](https://www.npmjs.com/package/pdfjs-dist) | 页面文本、坐标和基础结构 |
| 页面渲染 | PDFium 或经验证的自包含渲染器 | PDF 到 PNG，用于预览与视觉 QA |

PDFium 的具体分发方式必须在 P0 打包试验中验证许可证、平台二进制、单文件大小和总包体积。若通用包超限，优先评估 WASM/JS 渲染方案；不能退回运行时下载。

### 3.4 Codex 官方能力的使用边界

根据 OpenAI 官方文档，Codex Skills 是“指令、资源和可选脚本”的工作流封装；MCP 则用于把模型连接到工具和上下文。对本项目的结论是：

- 可以借鉴 Codex 文档类 Skills 的工作流思想：先结构检查，再渲染，再视觉检查，最后交付。
- 可以按各文件自身的公开许可证评估和复用 [`openai/skills`](https://github.com/openai/skills) 中公开资源。
- 不能因为某个文档工具存在于本机 Codex 运行时，就默认它允许被抽取、商业使用或随 npm 包再分发。
- 未公开发布且没有明确再分发许可的内部包或运行时组件，不进入生产依赖。
- 本项目不能依赖 Codex 才能运行；Codex 只是 MCP 的一个潜在客户端和 QA 参考。

官方参考：

- [Build skills](https://developers.openai.com/codex/skills)
- [Model Context Protocol](https://developers.openai.com/codex/mcp)

## 4. 总体架构

```text
ChatOS Task Runner
  -> MCP Management
  -> Local Connector Service
  -> Local Connector Client
  -> npm 包内 bin/chatos-document-mcp
  -> Node MCP Server
       -> 安全路径与授权层
       -> 类型化 Tool Router
       -> Office Adapter -> 包内 OfficeCLI 原生二进制
       -> PDF Adapter   -> pdf-lib / pdfjs-dist / PDF renderer
       -> Render + Validate QA Pipeline
  -> Workspace 只读输入 / Artifact 输出
```

### 4.1 分层职责

| 层 | 职责 |
| --- | --- |
| MCP Server | initialize、tools/list、tools/call、错误映射、取消与超时 |
| Tool Router | 参数 schema、工具权限、批量操作、返回结果裁剪 |
| Path Security | 根目录限制、路径规范化、软链接防逃逸、文件类型与大小检查 |
| Office Adapter | 把类型化操作翻译为受控 OfficeCLI 调用 |
| PDF Adapter | PDF 读取、修改、表单和页面处理 |
| Render Pipeline | Office/PDF 渲染为页面 PNG 和预览清单 |
| Validation Pipeline | 文件签名、OOXML/ZIP 结构、引用关系、打开测试和渲染检查 |
| Artifact Manager | 原子写入、命名、哈希、MIME、结果描述符 |

### 4.2 核心设计原则

1. 输入路径只允许相对于获批 Workspace 的路径。
2. 第一版输出只写入 `CHATOS_PLUGIN_ARTIFACT_DIR`，不原地覆盖 Workspace 文件。
3. 所有修改都先写临时文件，验证成功后再原子重命名为最终 Artifact。
4. 一个工具调用中的批量编辑要么全部成功，要么不产生可见的半成品。
5. 大内容写入文件并返回摘要、哈希和 Artifact 信息，不把整份二进制或超长文本塞进 MCP 返回值。
6. 同一个输入、参数和引擎版本应产生可追踪、尽量确定的结果。

## 5. ChatOS P0 平台改造

### 5.1 当前阻塞

当前 stdio Plugin MCP 启动时只注入：

```text
CHATOS_PLUGIN_ROOT
CHATOS_PLUGIN_DATA_DIR
CHATOS_PLUGIN_CACHE_DIR
CHATOS_PLUGIN_ARTIFACT_DIR
```

没有注入任务已绑定并获批的 Local Connector Workspace 根目录。Document MCP 如果接收任意绝对路径，会绕过 Workspace 权限；如果完全不接收路径，又无法操作项目文件。因此在生产接入前必须补齐 Workspace 授权契约。

### 5.2 最小可交付改造

在 ChatOS 为 Plugin MCP 准备 transport 时：

1. 从任务固定的 device/workspace snapshot 解析真实本地根目录。
2. 当组件声明 `workspace.read` 或 `workspace.write` 时，要求任务必须绑定 Local Connector Workspace。
3. 把已 canonicalize 的根目录以 `CHATOS_WORKSPACE` 注入 stdio MCP。
4. 把 Workspace identity/revision 纳入 runtime snapshot 或等价的不可变校验数据。
5. Workspace 不存在、已解绑、路径发生漂移或权限快照缺失时失败关闭。
6. 为准备、执行、重连、取消和权限拒绝增加测试。

更长期可以引入 `CHATOS_ALLOWED_FILE_ROOTS` 或 opaque `file_grant_id`，支持多个受控根目录及单文件授权；第一版不应因此阻塞最小的单 Workspace 根目录方案。

### 5.3 建议权限

Manifest 组件至少声明：

| 权限 | 首版 | 说明 |
| --- | --- | --- |
| `process.spawn` | 必需 | 启动 stdio MCP 及包内 Office/PDF 子进程 |
| `workspace.read` | 必需 | 读取用户在项目 Workspace 中指定的文件 |
| `artifact.create` | 必需 | 创建编辑后文件、渲染图片和报告 |
| `workspace.write` | 后续可选 | 第二阶段允许复制回或覆盖 Workspace 文件 |

`workspace.write` 不应在只生成 Artifact 的首版中设为必需权限。

## 6. MCP 工具设计

首版建议控制在 18 个工具以内。工具名使用稳定的领域名，不泄漏底层引擎。

### 6.1 通用工具

| 工具 | 访问类型 | 作用 |
| --- | --- | --- |
| `document_inspect` | 读 | 返回格式、大小、哈希、页/表/幻灯片数量、基础元数据和能力判断 |
| `document_extract_text` | 读 | 按页、工作表、幻灯片或段落提取有界文本/JSON |
| `document_render` | 读 + Artifact | 渲染指定页面为 PNG，并生成预览清单 |
| `document_validate` | 读 | 做结构、引用、可打开性和可选渲染验证 |
| `document_convert` | 读 + Artifact | 已实现 Office → 图像型 PDF；高保真可搜索 PDF 和其他文本/图片导出放后续扩展 |

### 6.2 Word 工具

| 工具 | 访问类型 | 作用 |
| --- | --- | --- |
| `word_create` | Artifact | 从结构化 blocks、样式和可选模板创建 `.docx` |
| `word_edit_batch` | 读 + Artifact | 一次执行查找替换、插入/删除段落、标题、表格、页眉页脚等受控操作 |

`word_edit_batch` 的每个 operation 使用有判别字段的联合 schema，例如 `replace_text`、`insert_paragraph`、`upsert_table`、`set_header`。不接受原始 XML 或任意 CLI 参数。

### 6.3 Excel 工具

| 工具 | 访问类型 | 作用 |
| --- | --- | --- |
| `spreadsheet_read_range` | 读 | 读取指定工作表区域，返回值、公式、类型和有限样式信息 |
| `spreadsheet_write_range` | 读 + Artifact | 批量写值/公式/基础样式，生成新 `.xlsx` |
| `spreadsheet_manage_sheets` | 读 + Artifact | 新建、重命名、删除、复制、排序工作表 |
| `spreadsheet_chart` | 读 + Artifact | 新建、更新或删除白名单图表类型 |

公式写入和“公式已重新计算”必须区分。若引擎不能可靠计算某类公式，结果应返回 `recalculationRequired: true`，不能伪造已计算状态。

### 6.4 PowerPoint 工具

| 工具 | 访问类型 | 作用 |
| --- | --- | --- |
| `presentation_create` | Artifact | 从主题、页面尺寸和 slide spec 创建 `.pptx` |
| `presentation_edit_batch` | 读 + Artifact | 编辑文字、图片、形状、表格、备注和基础布局 |
| `presentation_reorder_slides` | 读 + Artifact | 插入、复制、删除和调整幻灯片顺序 |

第一版对高级动画、SmartArt 和母版深度编辑只做保留，不承诺完整修改。

### 6.5 PDF 工具

| 工具 | 访问类型 | 作用 |
| --- | --- | --- |
| `pdf_merge` | 读 + Artifact | 按显式顺序合并多个 PDF |
| `pdf_split` | 读 + Artifact | 按页码范围拆分或抽取页面 |
| `pdf_transform` | 读 + Artifact | 旋转、重排、删除页面和更新基础元数据 |
| `pdf_form` | 读或读 + Artifact | 列出 AcroForm 字段，或填写字段后生成新 PDF |

### 6.6 统一参数约束

- `inputPath`、`inputPaths`：仅接受 Workspace 相对路径，禁止绝对路径和 `..`。
- `outputName`：仅接受文件名或受控的 Artifact 相对路径，不接受 Workspace 路径。
- `pages`、`slides`、`range`：使用明确、文档化的 1-based 语义；Excel A1 range 保持行业标准。
- `overwrite`：首版固定为 `false`，同名输出由服务端生成安全后缀或返回冲突。
- `maxChars`、`maxRows`、`renderDpi`：设置服务端硬上限，调用方只能降低不能提高。
- 所有对象 schema 使用 `additionalProperties: false`。

### 6.7 统一返回结构

```json
{
  "ok": true,
  "operation": "word_edit_batch",
  "source": {
    "relativePath": "docs/input.docx",
    "sha256": "..."
  },
  "artifact": {
    "relativePath": "result-edited.docx",
    "mimeType": "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
    "size": 12345,
    "sha256": "..."
  },
  "validation": {
    "status": "passed",
    "warnings": []
  },
  "summary": "Applied 3 edits and produced a validated DOCX artifact."
}
```

错误使用稳定代码，例如：`INVALID_PATH`、`UNSUPPORTED_FORMAT`、`FILE_TOO_LARGE`、`ENCRYPTED_FILE`、`ENGINE_TIMEOUT`、`VALIDATION_FAILED`、`OUTPUT_LIMIT_EXCEEDED`。

### 6.8 ChatOS Tool Policy

每个工具的 `_meta` 必须包含完整策略，而不是依赖工具名称推断：

```json
{
  "chatos/policyVersion": 1,
  "chatos/requiredPermissions": ["workspace.read", "artifact.create"],
  "chatos/riskLevel": "medium",
  "chatos/approvalMode": "none",
  "chatos/timeoutMs": 120000,
  "chatos/toolResultMaxChars": 30000
}
```

未来允许覆盖 Workspace 文件时，对应工具必须声明 `workspace.write`、`riskLevel: high` 和 `approvalMode: per_call`。

## 7. 文件安全设计

### 7.1 路径解析

每次访问都执行以下流程：

1. 拒绝 NUL、绝对路径、盘符路径、UNC 路径、空路径和 `..` 组件。
2. 将相对路径拼接到 Workspace 或 Artifact 根目录。
3. 对根目录和目标已存在的祖先执行 `realpath`/canonicalize。
4. 验证 canonical target 仍位于 canonical root 内。
5. 拒绝路径中的软链接、junction、reparse point 或其他可导致逃逸的特殊文件。
6. 打开文件后尽可能复核文件 identity，降低检查后替换的竞态风险。
7. 输出使用随机临时名、排他创建和原子 rename。

Windows 路径比较必须处理盘符大小写、分隔符和 reparse point，不能只用字符串 `startsWith`。

### 7.2 文件内容限制

首版建议默认硬限制：

| 项目 | 默认上限 |
| --- | ---: |
| 单个输入文件 | 100 MiB |
| 一次调用输入文件数 | 20 |
| OOXML 解压后总量 | 512 MiB |
| OOXML ZIP 条目数 | 10,000 |
| PDF 页数 | 2,000 |
| 单次渲染页面数 | 50 |
| 单次提取文本 | 30,000 字符返回；更大结果写 Artifact |
| 单次 Excel 读取单元格 | 100,000 |
| 单次批量编辑 operation | 500 |

这些数值应集中在配置模块中，并通过测试固定；不能让模型传入任意大值绕过。

### 7.3 内容风险

- 依据文件签名和内部结构识别格式，不只信任扩展名。
- 禁止外部实体、网络资源自动抓取和宏执行。
- 处理 OOXML ZIP bomb、重复/碰撞路径、超大 XML、关系引用逃逸。
- 图片等嵌入资源只从已验证文件包读取，不跟随外部链接。
- 日志不得记录用户正文、表格内容、完整工具参数或绝对路径。
- 诊断只记录 operation、格式、大小区间、耗时、引擎版本、结果状态和参数哈希。

## 8. 验证与视觉 QA

每个写工具完成后自动执行分层验证：

### 8.1 结构验证

- 输出扩展名、MIME 和文件签名一致。
- OOXML 是合法 ZIP，核心 `[Content_Types].xml` 和关系文件存在。
- Word 文档主体、Excel workbook/worksheets、PowerPoint presentation/slides 可解析。
- PDF header、xref/page tree 可读取。
- 不包含意外宏、外部路径引用或超限条目。

### 8.2 引擎打开验证

由底层引擎重新打开刚生成的文件并执行轻量 inspect。写成功但重新打开失败时，整个调用视为失败，临时输出不得发布为 Artifact。

### 8.3 渲染验证

对创建或编辑后的文件：

- 默认渲染第一页/第一张和发生修改的页面、工作表或幻灯片。
- 生成 `render-manifest.json`，记录页面、尺寸、PNG 哈希和警告。
- 检查空白页、零尺寸图片、渲染失败和页数异常。
- 测试环境对基准样例做像素差异或感知哈希比较。

视觉检查用于发现布局问题，但不能替代结构验证。Excel 的隐藏工作表、公式和结构问题也不能只靠截图判断。

## 9. npm 包与仓库结构

建议目录：

```text
docment_mcp/
├── docs/
│   └── DOCUMENT_MCP_IMPLEMENTATION_PLAN.zh-CN.md
├── package.json
├── package-lock.json
├── chatos.plugin.json
├── LICENSE
├── README.md
├── THIRD_PARTY_NOTICES.md
├── sbom.cdx.json
├── bin/
│   └── chatos-document-mcp
├── src/
│   ├── cli.ts
│   ├── server.ts
│   ├── config.ts
│   ├── errors.ts
│   ├── tools/
│   ├── security/
│   ├── engines/
│   │   ├── officecli.ts
│   │   ├── pdf-edit.ts
│   │   ├── pdf-extract.ts
│   │   └── pdf-render.ts
│   ├── validation/
│   └── artifacts/
├── scripts/
│   ├── fetch-vendor.mjs
│   ├── verify-vendor.mjs
│   ├── build.mjs
│   ├── generate-sbom.mjs
│   └── pack-and-verify.mjs
├── vendor/
│   ├── checksums.json
│   ├── licenses/
│   ├── officecli/<platform>/<arch>/...
│   └── pdf-renderer/<platform>/<arch>/...
├── dist/
│   └── server.mjs
└── test/
    ├── unit/
    ├── integration/
    ├── package/
    ├── security/
    └── fixtures/
```

### 9.1 JavaScript 打包策略

- TypeScript 编译并 bundle 为少量 ESM 文件，避免依赖运行时 `npm install`。
- 将 `@modelcontextprotocol/sdk` 及纯 JS 生产依赖打入 bundle。
- 必须显式复制 PDF.js worker、CMap、字体或其他动态资源。
- 禁止 `preinstall`、`install`、`postinstall`、`prepare` 生命周期脚本。
- npm `files` 白名单只包含运行所需内容、Manifest、许可证、SBOM 和文档。

### 9.2 原生二进制布局

launcher 根据 `process.platform` 和 `process.arch` 选择固定路径：

```text
darwin-arm64
darwin-x64
linux-arm64
linux-x64
win32-arm64
win32-x64
```

不支持的平台应在启动时返回明确错误。每个二进制在构建时校验 SHA-256，运行时可选择再次校验包内 checksum manifest。

### 9.3 P0 包体积试验

OfficeCLI 多平台二进制加 PDF renderer 可能逼近 ChatOS 的包体限制。正式编码前先制作最小 universal `.tgz`，执行：

```bash
npm pack
npm view ./chatos-document-mcp-*.tgz name version bin --json
tar -tzf ./chatos-document-mcp-*.tgz
```

并记录：

- `.tgz` 总大小。
- 解包后总大小。
- 最大单文件。
- 文件条目数。
- 六个平台 launcher 冒烟测试结果。

若超限，按以下顺序处理：

1. 对原生二进制 strip，删除调试符号和无用资源。
2. 用体积更小且许可合适的 PDF 渲染方案替换 PDFium 分发。
3. 评估只保留 ChatOS 当前正式支持的平台组合。
4. 若仍无法满足，再为 ChatOS 设计按平台/架构选择 artifact 的 Release 能力。

不能用安装后下载二进制作为规避方案。

## 10. Manifest 草案

实际字段须在实现时用 ChatOS schema 验证，首版方向如下。示例中的 `Apache-2.0` 是暂定的项目许可证，正式发布前必须由项目所有者确认：

```json
{
  "schemaVersion": 3,
  "name": "chatos-document-mcp",
  "version": "0.1.0",
  "description": "Read, create, edit, render and validate Word, Excel, PowerPoint and PDF files.",
  "author": {
    "name": "Chatos"
  },
  "license": "Apache-2.0",
  "mcpServers": {
    "document-mcp": {
      "type": "stdio",
      "bin": "chatos-document-mcp",
      "args": ["mcp"],
      "env": {}
    }
  },
  "interface": {
    "displayName": "Document Tools",
    "shortDescription": "Create and edit Office and PDF files locally",
    "longDescription": "Processes Word, Excel, PowerPoint and PDF files on the Local Connector Client with workspace boundaries and artifact outputs.",
    "developerName": "Chatos",
    "category": "Productivity"
  },
  "dependencies": {
    "minimumHostVersion": ">=2.1.0",
    "supportedPlatforms": ["macos", "windows", "linux"]
  },
  "permissions": [
    {
      "permission": "process.spawn",
      "required": true,
      "reason": "Launch the installed document MCP and bundled document engines.",
      "components": ["document-mcp"]
    },
    {
      "permission": "workspace.read",
      "required": true,
      "reason": "Read user-selected documents inside the bound project workspace.",
      "components": ["document-mcp"]
    },
    {
      "permission": "artifact.create",
      "required": true,
      "reason": "Create edited documents, PDFs, previews and validation reports.",
      "components": ["document-mcp"]
    }
  ]
}
```

npm package name 可以使用 scope，但 Manifest `name` 和 Marketplace plugin identity 的最终命名规则需要在首次上传前确认，避免发布后改 identity。

## 11. 测试方案

### 11.1 单元测试

- JSON schema 正确接受/拒绝每种 operation。
- 相对路径、Unicode 文件名、Windows 路径、软链接和路径穿越。
- 文件签名和扩展名不一致。
- page/range/slide 索引边界。
- 超时、取消、子进程退出码和 stderr 裁剪。
- Artifact 命名、冲突和原子提交。
- MCP 错误到稳定业务错误码映射。

### 11.2 引擎集成测试

- Word：标题、段落、表格、图片、页眉页脚、查找替换。
- Excel：多工作表、日期、数字格式、公式、合并单元格、图表。
- PowerPoint：文本、图片、形状、表格、复制/删除/排序页面。
- PDF：文本提取、合并、拆分、旋转、表单填写、渲染。
- 中英文、emoji、RTL 文本和缺失字体警告。
- 写入后重新打开、结构验证和渲染。

### 11.3 安全测试

- `../`、绝对路径、UNC、盘符、NUL、超长路径。
- Workspace 内指向外部的软链接/junction。
- 检查后替换文件的竞态测试。
- ZIP bomb、嵌套压缩、重复路径、case collision、XML 巨型节点。
- 恶意外部关系、宏文件、伪造扩展名、损坏 PDF。
- 超大页数、超大工作表、超大图片和内存压力。
- 用户正文、绝对路径和工具 payload 不进入日志。

### 11.4 MCP 协议测试

- initialize 成功并报告稳定 server/version。
- `tools/list` 工具数量、schema 大小和 `_meta` 合法。
- `tools/call` 成功、业务失败、超时、取消和并发。
- stdout 只输出 MCP 协议消息；诊断只写 stderr。
- 连续多次调用无临时文件泄漏和僵尸子进程。

### 11.5 npm 包测试

- `npm pack` 只包含白名单文件。
- 包中存在根目录 `chatos.plugin.json`。
- package/Manifest 版本完全一致。
- `package.json.bin` 指向安全普通文件。
- 没有 install lifecycle scripts。
- 所有 bundle 动态资源和六个平台二进制存在且哈希匹配。
- 从解包后的 `.tgz` 而不是源码目录启动 MCP 冒烟测试。
- 生成 CycloneDX SBOM 和完整 `THIRD_PARTY_NOTICES.md`。

### 11.6 ChatOS 端到端验收

1. 管理端上传 `.tgz` 并通过 analyze。
2. 发布不可变 Release。
3. Local Connector 下载、验签、校验并安装。
4. 绑定 Local Connector Workspace 的项目创建任务。
5. MCP initialize、tools/list 和读工具成功。
6. 编辑 Word/Excel/PPT、合并 PDF，产物进入会话 Artifact 目录。
7. 无 Workspace、无权限、客户端离线、Release revoked 时全部失败关闭。
8. 安装包任意字节被修改后安装失败。

## 12. CI/CD 与供应链

### 12.1 CI 阶段

```text
lint/typecheck
  -> unit/security tests
  -> build JS bundle
  -> fetch/build pinned vendor binaries
  -> verify SHA-256 and licenses
  -> integration tests per platform
  -> generate SBOM/notices
  -> npm pack
  -> inspect and test packed artifact
  -> publish candidate artifact
```

### 12.2 依赖固定

- `package-lock.json` 提交仓库，CI 使用 `npm ci`。
- OfficeCLI、PDF renderer 固定 release/commit 和每平台 SHA-256。
- 构建脚本只允许从固定 HTTPS URL 获取构建输入。
- 版本、URL、SHA-256、许可证和来源统一记录在 `vendor/checksums.json`。
- Dependabot/Renovate 只能创建升级 PR，不能自动发布。
- 发布前执行许可证扫描、漏洞扫描和 secret scan。

### 12.3 发布策略

- npm 使用公开包或组织 scope，先发布 `next`/候选版本。
- Marketplace 先进入 internal/preview channel，完成真实设备验收后再切 stable。
- Release 不原地覆盖；任何代码、依赖或二进制变化都提升版本。
- 高危供应链问题通过 revoke Release 处理。

## 13. 分阶段实施计划

### P0：可行性和平台契约（2–4 人日）

- 建立最小 npm/stdio MCP launcher。
- 制作包含 OfficeCLI 和 PDF renderer 的 universal 包体积试验。
- 验证六个平台二进制来源、许可、SHA 和单文件大小。
- 在 ChatOS 设计并实现 `CHATOS_WORKSPACE` 注入及权限快照校验。
- 确认 npm scope、Manifest identity 和最低 Host 版本。

退出条件：能从真实 `.tgz` 启动一个只读 `document_inspect`，并只能读取获批 Workspace 内文件。

### P1：MCP 骨架和安全底座（3–5 人日）

- TypeScript 项目、MCP SDK、错误模型、工具注册。
- Path Security、Artifact Manager、超时、取消、日志脱敏。
- `document_inspect`、`document_extract_text` 基础实现。
- package/manifest/version 一致性测试。

退出条件：路径穿越和软链接逃逸测试全部通过，MCP 协议测试稳定。

### P2：Office 能力（8–12 人日）

- OfficeCLI adapter 和平台选择。
- Word create/edit。
- Excel range/sheet/chart。
- PowerPoint create/edit/reorder。
- Office export PDF、重新打开验证和基础渲染。

退出条件：核心 Word/Excel/PPT golden fixtures 在三个操作系统上通过结构与渲染验收。

### P3：PDF 能力（5–8 人日）

- PDF 文本提取和页面信息。
- merge/split/transform/form。
- PDF render 和 render manifest。
- 损坏、加密、超大 PDF 的失败策略。

退出条件：PDF 功能、内存上限和恶意 fixture 测试通过。

### P4：质量与供应链（5–8 人日）

- 完整 Validation Pipeline。
- golden/render regression 测试。
- SBOM、第三方 notices、许可证和漏洞扫描。
- packed artifact 的多平台 CI。
- 性能基准和默认上限调优。

退出条件：候选 `.tgz` 满足 ChatOS 全部包限制，并可在干净环境离线运行。

### P5：ChatOS 集成与上架（3–5 人日）

- 管理端 analyze/publish。
- Local Connector 安装、权限和工具审批验收。
- 任务端 Artifact 展示和下载验证。
- preview channel 灰度、故障演练、撤销演练。
- README、用户说明、故障排查和发布清单。

退出条件：Marketplace stable 发布清单全部签字通过。

总体估算为 26–42 工程人日，不包含复杂字体兼容、旧 Office 格式、OCR 或对 ChatOS Release 模型做按平台 artifact 扩展的额外工作。单人预计约 5–8 周；两名熟悉 Node/Rust/文档格式的工程师可并行压缩日历时间。

## 14. 里程碑与交付物

| 里程碑 | 主要交付物 |
| --- | --- |
| M0 可行性完成 | ADR、universal `.tgz` 体积报告、Workspace 注入方案 |
| M1 安全只读版 | npm 骨架、inspect/extract、路径安全测试 |
| M2 Office MVP | Word/Excel/PPT 创建编辑、导出、渲染 |
| M3 PDF MVP | PDF 编辑、表单、渲染 |
| M4 Release Candidate | SBOM、许可证、跨平台 packed artifact、完整 QA |
| M5 Marketplace Stable | ChatOS 上架、操作手册、回滚与 revoke 方案 |

## 15. 风险与应对

| 风险 | 影响 | 应对 |
| --- | --- | --- |
| universal npm 包超过 256 MiB | 无法上架 | P0 先打包；strip/换 renderer/缩平台；必要时扩展平台 artifact 模型 |
| OfficeCLI 行为或 API 变化 | 编辑结果不稳定 | 固定版本和 SHA；adapter 隔离；golden regression |
| 复杂 Office 排版渲染差异 | 视觉质量不一致 | 明确能力边界；结构 + 多平台渲染 QA；输出 warnings |
| 字体缺失 | 换行和分页变化 | 记录字体警告；提供受许可字体策略；不静默宣称像素一致 |
| Excel 公式缓存未更新 | 用户看到旧结果 | 区分写入公式与计算；设置重算标记；返回 `recalculationRequired` |
| Workspace 根目录未安全注入 | 越权读写或功能不可用 | 作为 P0 阻塞项，未完成前不发布写文件 MCP |
| 软链接/竞态逃逸 | 访问 Workspace 外文件 | canonicalize、拒绝特殊链接、排他打开、平台安全测试 |
| PDF/OOXML 恶意文件耗尽资源 | 客户端崩溃 | 页数/条目/解压量/内存/时间硬上限，子进程隔离 |
| 第三方许可证不适合再分发 | 发布受阻 | P0 法务与 notices 审查；不可确认的组件不进入包 |
| 工具 schema 过大 | 超 512 KiB 快照 | 控制在约 18 工具；复用紧凑 schema；CI 固定检查 |

## 16. 上线验收标准

以下条件全部满足才可进入 stable：

- `.tgz`、单文件、解包大小、条目数均低于 ChatOS 限制。
- 包内无安装下载脚本、无未固定网络依赖、无 secret。
- macOS/Windows/Linux 声明支持的平台能从 packed artifact 完成 initialize、tools/list、tools/call。
- 所有文件输入都受 Workspace 根目录限制，所有首版输出都在 Artifact 目录。
- 读写工具的 `_meta` 权限、风险、审批、超时和结果上限通过 ChatOS 校验。
- Word、Excel、PowerPoint、PDF 的核心 golden fixtures 通过结构验证和渲染验证。
- 取消/超时不会留下僵尸子进程或半成品。
- 日志和审计不包含用户正文、绝对路径、凭据或完整 payload。
- SBOM、`THIRD_PARTY_NOTICES.md`、许可证和 vendor checksum 齐全。
- 安装、禁用、升级、撤销 Release 和客户端离线场景均验证。

## 17. 第二阶段候选能力

- Workspace 写回和原地覆盖，必须使用 per-call 审批、备份和原子替换。
- Word 修订、批注、接受/拒绝变更。
- Excel 更完整的公式计算和数据透视表。
- PowerPoint 母版、主题、图表和动画增强。
- OCR、扫描 PDF、图片转可搜索 PDF。
- 旧格式通过独立、受控转换器迁移到 OOXML。
- 模板资源、品牌规范和可复用文档工作流。
- 基于渲染图的自动视觉异常检测。
- ChatOS opaque file grants，替代向插件暴露完整 Workspace 根目录。

## 18. 实施开始前的决策清单

开始编码前只需确认以下产品级决策：

1. npm 最终名称及 publisher：是否使用 `@chatos/document-mcp` 和 Chatos publisher。
2. 第一版是否接受“Workspace 只读输入、所有修改生成 Artifact”的安全模型。
3. 正式支持的平台/架构是否必须一次覆盖六个组合。
4. OfficeCLI 固定版本和 PDF renderer 选型在 P0 试验后的结论。
5. ChatOS `CHATOS_WORKSPACE` 改造由本项目同时提交，还是由 ChatOS 团队单独实现。

除以上决策外，目录、工具边界、安全策略和分阶段计划可以直接作为第一轮开发基线。

## 19. 调研依据

ChatOS 本地资料：

- `clients/macos/Sources/ChatOSConnector/NativeLocalConnectorService+Plugins.swift`
- `clients/macos/Sources/ChatOSConnector/NativeLocalConnectorService+PluginRelay.swift`
- `clients/windows/src/ChatOS.Connector/Plugins/`
- `plugins/browser/npm/`
- `plugins/computer-use/scripts/`

开源项目：

- [iOfficeAI/OfficeCLI](https://github.com/iOfficeAI/OfficeCLI)
- [microsoft/markitdown](https://github.com/microsoft/markitdown)
- [SecurityRonin/docx-mcp](https://github.com/SecurityRonin/docx-mcp)
- [haris-musa/excel-mcp-server](https://github.com/haris-musa/excel-mcp-server)
- [pdf-lib](https://github.com/Hopding/pdf-lib)
- [PDF.js](https://github.com/mozilla/pdf.js)

OpenAI 官方资料：

- [Build skills](https://developers.openai.com/codex/skills)
- [Model Context Protocol](https://developers.openai.com/codex/mcp)
