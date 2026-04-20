# 项目目录代码搜索与跳转方案

## 结论

在你补充“项目语言不固定，可能是所有主流语言”之后，方案需要明确调整：

1. **全文搜索仍然可以先做，而且与语言无关**  
   这是第一优先级，收益最高，复杂度最低。

2. **定义跳转 / 引用查询必须走抽象层，不能写死某一种语言**  
   这部分不能做成“Java 专用功能塞在公共流程里”，而要做成：
   - 一个统一导航接口
   - 多个语言模块独立实现
   - 每个语言单独声明支持能力

3. **当前已经从首批 Java / TypeScript / JavaScript / Rust / Go / Python 扩展到 Kotlin / C / C++ / C#**  
   实现上继续保持“新增语言只加 provider，不改公共协议和前端交互”。

所以最终建议不是“直接做 Java 跳转”，而是：

- **P0：先做多语言通用的全文搜索 + 行定位**
- **P1：搭建统一代码导航抽象层**
- **P2：先接入主流语言模块**
  - Java
  - Kotlin
  - TypeScript
  - JavaScript
  - C
  - C++
  - C#
  - Rust
  - Go
  - Python
- **P3：逐步补齐更多语言与能力**

---

## 当前现状

## 当前实施进度

截至本轮，方案里的主干能力已经不是设计态，而是已有一版可工作的实现：

- 已完成语言无关全文搜索接口：`GET /api/fs/search-content`
- 已完成统一代码导航接口：
  - `POST /api/code-nav/capabilities`
  - `POST /api/code-nav/definition`
  - `POST /api/code-nav/references`
  - `POST /api/code-nav/document-symbols`
- 已完成前端项目树中的全文搜索、搜索结果跳转、高亮、预览区内“跳到定义 / 查找引用 / 文件符号”
- 已完成预览区 token 级兜底交互：选中 symbol 后可直接触发“项目内搜索”，并按 capabilities/fallback 决定是否展示定义/引用按钮
- 已完成搜索命中高亮：左侧全文搜索结果高亮关键词，右侧预览区在打开命中行时高亮对应匹配片段
- 已完成搜索选项接入：项目树全文搜索支持“区分大小写 / 全词匹配”，并且结果高亮规则与实际搜索参数保持一致
- 已完成搜索命中连续导航：左侧搜索区和右侧预览区都支持“上一处 / 下一处”，并显示当前命中序号
- 已完成搜索快捷键：在项目树搜索框内支持 `Enter` 下一处、`Shift+Enter` 上一处，并兼容输入法组合态
- 已完成预览区命中块直达：全文搜索结果按实际命中位置返回，右侧预览区的高亮命中块可直接点击切换当前命中，同一行多个匹配可分别导航
- 已完成多语言 provider 抽象与 fallback 编排
- 已完成 code-nav manager 进程内复用，避免每次请求重复构造 provider 列表
- 已完成公共轻量项目符号索引缓存，Java / Kotlin / C / C++ / C# / Rust / Go / Python 的 definition 可优先走符号索引，未命中再回退文本扫描
- 已完成 heuristic 搜索同一行多命中返回，引用查询不再只返回同一行第一处命中

当前语言支持分为两层：

- 已接入较强能力
  - TypeScript
  - JavaScript
    - 通过 TypeScript language service bridge 提供真实 definition / references / document symbols
- 已接入 provider-heuristic 能力
  - Java
  - Kotlin
  - C
  - C++
  - C#
  - Rust
  - Go
  - Python
    - 每种语言都在独立模块内实现 definition / references / document symbols
    - 不把语言细节写死在公共流程

当前还未完成的主要工作：

- 前端联调与真实项目手工验证
- 更多语言继续接入，例如 PHP / Ruby / Swift / Dart 等
- 逐步把 heuristic provider 升级为更强语义能力
- 继续细化符号索引失效策略，例如按文件 mtime 或文件变更事件精确刷新，而不是仅依赖短 TTL

## 1. 现在已有文件搜索入口，但不是全文搜索

- 前端已经封装了 `/fs/search`：
  - `chat_app/src/lib/api/client/workspace.ts:698`
- 当前这个接口已被项目文件选择器使用：
  - `chat_app/src/components/inputArea/useProjectFilePicker.ts:187`
- 但后端实现只搜文件名和路径，不搜文件内容：
  - `chat_app_server_rs/src/api/fs/query_handlers.rs:175`
  - `chat_app_server_rs/src/api/fs/search.rs:1`

结论：

当前“搜注释在哪个文件里”这件事，还没有真正实现。

## 2. 内置 MCP 里已经有全文搜索能力

- `search_text` 已经注册：
  - `chat_app_server_rs/src/builtin/code_maintainer/mod.rs:251`
- 底层实现可递归搜索文本内容并返回 `path + line + text`：
  - `chat_app_server_rs/src/builtin/code_maintainer/fs_ops.rs:140`

结论：

全文搜索的底层能力已经有一份可复用实现，适合抽出来做共享服务。

## 3. 代码预览区还不是可导航编辑器

- 右侧代码区当前是 `highlight.js` 静态渲染：
  - `chat_app/src/components/projectExplorer/PreviewPane.tsx:114`
- 逐行渲染到普通 `div`：
  - `chat_app/src/components/projectExplorer/PreviewPane.tsx:126`
  - `chat_app/src/components/projectExplorer/PreviewPane.tsx:139`
- 还没有目标行、目标列、符号跳转、引用列表这些状态：
  - `chat_app/src/components/projectExplorer/useProjectExplorerState.ts:14`

结论：

当前这块更像“文件预览器”，不是“可做 definition / references 的代码编辑器”。

---

## 方案方向修正

## 不能把“多语言跳转”理解成一个统一算法

这件事最容易踩坑的地方，就是试图用一套通用逻辑处理所有语言。

这样做会出问题：

- Java 要处理 classpath / module / overload / interface / inheritance
- TS/JS 要处理 tsconfig / jsconfig / path alias / declaration file
- Rust 要处理 crate / workspace / macro / cargo metadata
- Go 要处理 module / package / vendor / workspace
- Python 要处理 venv / import path / namespace package

这些语义模型差异很大，所以**实现必须是统一协议 + 按语言拆模块**，而不是统一解析器。

## 推荐的总架构

### 公共层只做三件事

1. 路由与鉴权
2. 统一请求/响应协议
3. 语言模块选择、能力探测、结果标准化

### 各语言模块各自负责

1. 工程识别
2. 语言服务启动与缓存
3. definition / references / symbols 的具体实现
4. 语言特有的路径与配置处理

### 没有语言模块命中时的退化策略

1. 先退到“弱语义导航”
2. 再退到全文搜索

这样用户永远不会遇到“点了完全没反应”。

---

## 推荐目标架构

## 后端模块结构

建议新增：

- `chat_app_server_rs/src/services/code_nav/mod.rs`
- `chat_app_server_rs/src/services/code_nav/types.rs`
- `chat_app_server_rs/src/services/code_nav/manager.rs`
- `chat_app_server_rs/src/services/code_nav/registry.rs`
- `chat_app_server_rs/src/services/code_nav/fallback.rs`
- `chat_app_server_rs/src/services/code_nav/workspace.rs`

语言实现按目录拆分：

- `chat_app_server_rs/src/services/code_nav/languages/java/`
- `chat_app_server_rs/src/services/code_nav/languages/kotlin/`
- `chat_app_server_rs/src/services/code_nav/languages/typescript/`
- `chat_app_server_rs/src/services/code_nav/languages/javascript/`
- `chat_app_server_rs/src/services/code_nav/languages/c/`
- `chat_app_server_rs/src/services/code_nav/languages/cpp/`
- `chat_app_server_rs/src/services/code_nav/languages/csharp/`
- `chat_app_server_rs/src/services/code_nav/languages/rust/`
- `chat_app_server_rs/src/services/code_nav/languages/go/`
- `chat_app_server_rs/src/services/code_nav/languages/python/`
- `chat_app_server_rs/src/services/code_nav/languages/basic.rs`

可选的共用 LSP 适配层：

- `chat_app_server_rs/src/services/code_nav/lsp/`

## 核心 trait 建议

```rust
pub trait CodeNavProvider: Send + Sync {
    fn language_id(&self) -> &'static str;

    fn detect_project(&self, ctx: &ProjectContext) -> bool;

    fn capabilities(&self, ctx: &ProjectContext) -> NavCapabilities;

    async fn definition(
        &self,
        ctx: &ProjectContext,
        req: NavPositionRequest,
    ) -> Result<Vec<NavLocation>, String>;

    async fn references(
        &self,
        ctx: &ProjectContext,
        req: NavPositionRequest,
    ) -> Result<Vec<NavLocation>, String>;

    async fn document_symbols(
        &self,
        ctx: &ProjectContext,
        req: DocumentSymbolsRequest,
    ) -> Result<Vec<DocumentSymbolItem>, String>;
}
```

说明：

- `ProjectContext` 负责携带项目根目录、用户、缓存目录、环境信息
- `NavPositionRequest` 统一表示 `file + line + column`
- `NavLocation` 统一表示 `path + range + preview + score`
- 每个语言模块只实现这个接口

## 管理层职责

`manager.rs` 建议负责：

- 根据文件后缀和工程标记选择 provider
- 做 provider 生命周期管理
- 处理 session / cache / timeout
- 结果标准化
- fallback 链路编排

选择逻辑建议：

1. 先按文件扩展名粗匹配
2. 再按项目特征确认
3. 命中多个 provider 时按优先级决策

例如：

- `.ts/.tsx`
  - 优先 `typescript`
- `.js/.jsx`
  - 优先 `javascript`
  - 如项目存在 `tsconfig.json` 或 JS 开启 TS server，可复用 `typescript`
- `.java`
  - `java`
- `.rs`
  - `rust`
- `.go`
  - `go`
- `.py`
  - `python`

---

## 推荐能力分层

## 第一层：语言无关能力

这层先做，所有项目都能收益。

- 全文搜索
- 打开文件定位到行
- 结果高亮
- 基础 token 提取

## 第二层：弱语义能力

这层不依赖完整语言服务，可在任何语言上作为 fallback。

- 当前 token 搜索
- 常见定义模式识别
- 同文件 / 同目录优先排序
- 文件名与 symbol 名匹配加权

## 第三层：强语义能力

这层由每个语言模块单独负责。

- 定义跳转
- 查找引用
- 文档符号
- 可选 hover / implementation / type definition

---

## 多语言实现策略

## 推荐主路径：LSP 优先，但不是所有语言共享一份业务逻辑

我建议主流语言优先走 LSP，但**不是把 LSP 直接散落在业务代码里**，而是：

- 语言模块独立
- 能复用 LSP 的模块，内部再调用公共 LSP 适配层

也就是两层抽象：

1. `CodeNavProvider`
2. `LspClientAdapter`（可选共用）

这样能同时满足：

- 公共协议一致
- 语言实现独立
- 以后接非 LSP 语言也不受影响

## 主流语言建议

### Java

建议单独模块：

- `languages/java/mod.rs`

职责：

- 识别 `pom.xml` / `build.gradle` / `settings.gradle`
- 管理 Java 语言服务会话
- 处理 definition / references / symbols

### TypeScript

建议单独模块：

- `languages/typescript/mod.rs`

职责：

- 识别 `tsconfig.json`
- 处理 path alias / baseUrl / project reference
- definition / references / symbols

### JavaScript

建议单独模块：

- `languages/javascript/mod.rs`

职责：

- 识别 `package.json` / `jsconfig.json`
- 可内部共享 TS server 适配层
- 但对外仍然保持独立 provider

原因：

- JS 和 TS 运行时共用很多东西
- 但在产品语义上最好仍然是两个 provider，便于后续独立开关和能力披露

### Rust

建议单独模块：

- `languages/rust/mod.rs`

职责：

- 识别 `Cargo.toml`
- workspace / crate 管理
- definition / references / symbols

### Go

建议单独模块：

- `languages/go/mod.rs`

职责：

- 识别 `go.mod`
- module / package / vendor 环境处理
- definition / references / symbols

### Python

建议单独模块：

- `languages/python/mod.rs`

职责：

- 识别 `pyproject.toml` / `requirements.txt`
- venv / import path 处理
- definition / references / symbols

---

## 为什么要“每个语言独立模块”

这是你这次强调的重点，我完全同意，而且我建议写成明确约束：

### 约束一

公共层不得出现某个语言的业务判断，例如：

- 不要在 manager 里写 Java 的类名匹配逻辑
- 不要在公共接口里写死 TS 的 path alias 字段

### 约束二

语言模块之间不直接互相依赖业务逻辑。

允许共享：

- LSP 传输层
- 进程管理
- 缓存与超时控制
- 日志工具

不建议共享：

- definition 结果后处理规则
- references 排序规则
- 工程探测逻辑

### 约束三

前端不感知某个语言的内部差异，只感知能力。

也就是说前端只看：

- 当前文件语言
- 当前 provider
- 是否支持 definition
- 是否支持 references
- 是否已退化为 fallback search

而不是关心这个语言具体用了什么底层实现。

---

## 推荐接口设计

## 一、全文搜索接口

### `GET /api/fs/search-content`

请求参数：

- `path`
- `q`
- `limit`
- `case_sensitive`
- `whole_word`

响应：

- `path`
- `query`
- `entries`
  - `path`
  - `relative_path`
  - `line`
  - `column`
  - `text`
- `truncated`

这部分语言无关，先做。

## 二、代码导航能力接口

### `POST /api/code-nav/capabilities`

请求：

- `project_root`
- `file_path`

响应：

- `language`
- `provider`
- `supports_definition`
- `supports_references`
- `supports_document_symbols`
- `fallback_available`

这个接口很重要，因为前端可以按能力展示 UI，不需要猜。

## 三、定义跳转接口

### `POST /api/code-nav/definition`

请求：

- `project_root`
- `file_path`
- `line`
- `column`

响应：

- `provider`
- `language`
- `locations`
  - `path`
  - `line`
  - `column`
  - `end_line`
  - `end_column`
  - `preview`
  - `score`
- `mode`
  - `semantic`
  - `fallback`

## 四、引用查询接口

### `POST /api/code-nav/references`

请求同上。  
响应结构同样统一。

## 五、文档符号接口

### `POST /api/code-nav/document-symbols`

这个接口对前端很有价值，可做：

- 文件内符号树
- 面包屑
- 当前光标附近符号高亮

---

## 前端改造建议

## 1. 先升级代码预览区

如果未来要做 definition / references，我建议右侧代码区最终迁移到 Monaco。

原因：

- 可做只读编辑器，不影响当前预览体验
- 原生适合处理 range、decorations、revealLine、go to location
- 后续扩 capability 更自然

当前还没有 Monaco 依赖：

- `chat_app/package.json`

## 2. 前端状态设计要语言无关

建议在项目目录状态里增加：

- `searchQuery`
- `searchResults`
- `activeHit`
- `targetLocation`
- `navCapabilities`
- `navLocations`
- `navMode`
- `navLoading`

建议改动点：

- `chat_app/src/components/projectExplorer/useProjectExplorerState.ts`
- `chat_app/src/components/ProjectExplorer.tsx`
- `chat_app/src/components/projectExplorer/PreviewPane.tsx`
- `chat_app/src/components/projectExplorer/TreePane.tsx`

## 3. 点击行为分两档

### 普通点击

- 打开文件

### 导航点击

- Alt+Click / Cmd+Click
- 或右键菜单：
  - 跳到定义
  - 查找引用

如果当前 provider 不支持语义导航：

- 自动走 fallback search
- UI 提示“已退化为文本搜索结果”

---

## 推荐分阶段实施

## P0：多语言通用全文搜索

### 目标

- 搜注释
- 搜方法名
- 搜字符串
- 点击结果定位行

### 后端

从这里抽共享能力：

- `chat_app_server_rs/src/builtin/code_maintainer/fs_ops.rs:140`

建议新增：

- `chat_app_server_rs/src/services/workspace_search/`
- `/api/fs/search-content`

### 前端

- 项目目录头部加搜索框
- 搜索结果列表
- 文件打开后滚动定位到命中行
- 高亮当前命中行

### 价值

这个阶段完全不依赖具体语言，收益最大，必须先做。

## P1：代码导航抽象层

### 目标

把公共协议和语言模块骨架先搭起来，即使只先接入一两个语言，也不能破坏未来扩展。

### 本阶段必须完成

- `CodeNavProvider` trait
- `manager/registry/types`
- `capabilities/definition/references/document-symbols` 四类统一接口
- fallback 策略

### 本阶段先不追求

- 一口气支持所有语言
- 一开始就做完所有高级能力

## P2：主流语言第一批接入

建议先接入：

1. Java
2. TypeScript
3. JavaScript
4. Rust

然后第二批：

1. Go
2. Python

原因：

- Java / TS / JS / Rust 在工程类项目里高频
- 这几类语言的“点定义/查引用”需求最刚性

## P3：增强与扩展

- 文件内符号树
- hover
- implementation
- type definition
- workspace symbol
- 更多语言

---

## Fallback 设计

这部分我建议明确写进产品行为，否则体验会很割裂。

## fallback 顺序

1. 语义 definition / references
2. 弱语义 token 搜索
3. 全文搜索

## 返回时必须告诉前端当前模式

例如：

- `mode=semantic`
- `mode=heuristic`
- `mode=text-search`

这样前端能准确展示：

- “已跳到定义”
- “未命中语义服务，已展示近似匹配结果”
- “当前语言暂不支持语义导航，已退化为全文搜索”

---

## 风险与注意点

## 1. 不要把现有 `/api/fs/search` 改成全文搜索

因为它已经被文件选择器使用：

- `chat_app/src/components/inputArea/useProjectFilePicker.ts:203`

应该新增专用接口，而不是改旧接口语义。

## 2. 不要先写 Java 特判再“以后抽象”

这类代码一旦进公共层，后面再抽会很痛。

正确做法是：

- 先定义 provider 抽象
- 再让 Java 成为第一个 provider

## 3. JS 和 TS 可以共享底层能力，但不建议合并成同一个业务模块

因为未来你们很可能需要：

- 单独开关
- 单独能力展示
- 单独统计和问题定位

所以建议：

- 对外两个 provider
- 对内可共享一层语言服务适配

## 4. 语义导航必须做超时和缓存

否则第一次打开大项目会很慢。

当前已有一版轻量实现：

- code-nav manager 已做进程内静态复用
- 公共 `symbol_index` 已做 project root + provider 级符号索引缓存，覆盖 basic provider 与 Java / Rust / Go / Python 独立 provider
- 缓存命中用于 definition 快速定位，缓存项失效或读不到文件时跳过并回退

建议 manager 层负责：

- project root 级缓存
- provider 会话复用
- 冷启动超时
- 自动失效与重建

## 5. 前端不要依赖某个语言特定字段

前端只消费统一结构：

- location
- range
- preview
- mode
- capability

---

## 最终建议

如果按你现在的目标，我的最新建议是：

1. **先把全文搜索做出来**
   因为这部分完全多语言通用，而且马上就能解决“搜注释在哪个文件”的问题。

2. **同时把代码导航抽象层先搭好**
   即使第一批只接 Java / TS / JS / Rust，也必须先有统一 provider 架构。

3. **每个语言单独模块实现**
   这不是“后面再整理”的优化项，而是第一天就该定下来的结构约束。

4. **前端按能力驱动**
   不按语言硬编码按钮，而是根据 `capabilities` 决定显示“跳到定义”“查找引用”“文本搜索”。

---

## 这版方案对应的最小可落地版本

如果现在立刻进入开发，我建议第一阶段就做下面这组：

1. `search-content` 多语言通用接口
2. 项目目录内容搜索 UI
3. 打开文件并定位高亮
4. `code-nav` 抽象层骨架
5. Java provider
6. TypeScript provider
7. JavaScript provider

这样做的好处是：

- 搜索马上可用
- 架构不会写死
- 主流前端/后端语言先覆盖
- 后面扩 Rust / Go / Python 时不需要推翻协议
