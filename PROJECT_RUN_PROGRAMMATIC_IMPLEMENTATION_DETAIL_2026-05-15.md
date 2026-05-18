# 项目运行能力程序化改造细化方案

## 1. 目标

把当前“AI 生成启动脚本”的模式，改成“程序化分析 + 稳定执行”的模式。

核心要求：

- 自动识别项目入口。
- 自动发现可运行目标。
- 允许用户选择本机已安装的多个工具链版本。
- 支持 `start / stop / restart / status`。
- 不再依赖 AI 生成 `project_runner.sh`。

## 2. 现有基础

现有代码已经具备两个重要底座：

- `chat_app_server_rs/src/services/project_run/analyzer.rs`
  - 已能扫描 `package.json`、`pom.xml`、`build.gradle`、`pyproject.toml`、`go.mod`、`Cargo.toml` 等。
  - 已有 `ProjectRunCatalog` / `ProjectRunTarget`。
- `chat_app_server_rs/src/api/projects/run_handlers.rs`
  - 已有 `/run/analyze`、`/run/catalog`、`/run/execute`、`/run/default`。

所以这次不是重做系统，而是把“猜命令”升级成“运行计划 + 环境配置 + 工具链选择”。

## 3. 总体结构

建议拆成三层：

1. 运行目标层
   - 找出项目能跑什么。
2. 环境层
   - 找出机器上有哪些 JDK / Maven / Node / Python / Cargo 等版本。
3. 执行层
   - 把“目标 + 环境”合成最终命令并派发到终端。

## 4. 数据模型

### 4.1 运行目标

建议扩展 `ProjectRunTarget`：

- `language`: `java | rust | node | python | go | ...`
- `entrypoint`: 入口文件或主类
- `manifestPath`: `pom.xml` / `Cargo.toml` / `package.json` 等
- `requires`: 运行前必须存在的工具链
- `envOverrides`: 执行时注入的环境变量
- `healthCheck`: 可选健康检查方式

### 4.2 环境配置

新增一个项目级运行配置对象，例如：

```ts
ProjectRunEnvironmentConfig {
  projectId: string
  language: string
  selectedToolchains: {
    java?: string
    mvn?: string
    gradle?: string
    cargo?: string
    node?: string
    python?: string
    go?: string
  }
  envVars: Record<string, string>
  updatedAt: string
}
```

### 4.3 工具链候选项

前端下拉框直接来自后端探测结果：

```ts
ToolchainOption {
  id: string
  kind: string
  label: string
  version?: string
  path: string
  source: 'system' | 'sdkman' | 'asdf' | 'brew' | 'pyenv' | 'nvm' | 'manual'
  isDefault: boolean
}
```

## 5. 后端接口

建议补充这些接口：

- `GET /projects/:id/run/catalog`
  - 返回运行目标 + 默认目标 + 分析状态。
- `POST /projects/:id/run/analyze`
  - 重新扫描项目并刷新 catalog。
- `GET /projects/:id/run/environment/options`
  - 返回当前机器可用的工具链候选项。
- `GET /projects/:id/run/environment`
  - 返回当前项目已经保存的环境选择。
- `PUT /projects/:id/run/environment`
  - 保存用户选择的工具链与环境变量。
- `POST /projects/:id/run/execute`
  - 用“目标 + 环境”派发最终命令。

## 6. 语言策略

### 6.1 Java

识别规则：

- `pom.xml` -> Maven 项目。
- `build.gradle` / `build.gradle.kts` -> Gradle 项目。
- `src/main/java` -> Java 源码根。
- `public static void main` -> 入口候选。
- Spring Boot 主类 -> 优先级最高。

环境选择：

- `JAVA_HOME` 候选：从系统、SDKMAN、asdf、brew、手动路径中探测。
- `mvn` 候选：系统 Maven、brew Maven、手动路径。
- `gradle` 候选：wrapper 优先，其次系统 Gradle。

### 6.2 Rust

识别规则：

- `Cargo.toml` -> Rust 项目。
- `src/main.rs` -> 默认入口。
- `src/bin/*.rs` -> 多二进制入口。
- `[[bin]]` -> 显式二进制入口。

环境选择：

- `cargo`、`rustc`、`rustup` 候选探测。
- 默认优先 `cargo run`，多 bin 时给出下拉选择。

### 6.3 Node / Python / Go

同样按“manifest + 入口 + 工具链候选”模式处理：

- Node: `node / npm / pnpm / yarn`
- Python: `python / python3 / pyenv / venv`
- Go: `go`

## 7. 多版本下拉框

这是这次改造的重点。

当机器上存在多个 JDK / Python / Node / Go 版本时：

- 后端先探测出候选项。
- 前端用下拉框展示 `label + version + path`。
- 用户可直接选中某个版本。
- 选项不足时仍保留“手动输入路径”。
- 选择结果按项目保存，下次打开直接恢复。

## 8. UI 改造

建议在项目预览区增加“环境设置”面板：

- 语言识别后自动展示对应卡片。
- Java 卡片显示 JDK / Maven / Gradle。
- Rust 卡片显示 Cargo / Rustup。
- Node / Python / Go 同理。
- 提供“自动检测”“下拉选择”“手动填写”“恢复默认”。
- 启动前显示最终命令预览。

## 9. 存储策略

建议不要把运行环境塞进 AI 相关配置里，而是单独存：

- `project_run_catalogs`：保存分析结果。
- 新增 `project_run_environment_settings`：保存用户的工具链选择。

这样好处是：

- 运行目标和用户偏好分离。
- catalog 可以随时重扫。
- 环境选择可按项目长期保留。

## 10. 执行流程

1. 扫描项目并识别语言。
2. 探测本机可用工具链。
3. 用户在 UI 中选择工具链版本。
4. 保存项目级环境配置。
5. 生成最终执行命令。
6. 派发到终端。
7. 写入日志 / PID / 端口记录。
8. `stop` 只按 PID 回收。

## 11. 迁移步骤

1. 先增强 `project_run/analyzer.rs`，让它输出更完整的目标信息。
2. 增加环境探测接口。
3. 加运行环境选择 UI。
4. 保留旧脚本作为兼容层，但不再由 AI 生成。
5. 迁移到纯程序化执行。
6. 删除 AI 生成脚本路径。

## 12. 验收标准

- 多个 JDK 版本能在下拉框里直接选。
- Java / Rust / Node / Python / Go 都能自动识别候选环境。
- 启动前能看到最终使用的命令和工具链。
- `start / stop / restart` 不依赖 AI。
- `stop` 不会误杀其他项目进程。

