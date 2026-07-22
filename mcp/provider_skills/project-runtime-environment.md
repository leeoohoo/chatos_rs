这个 MCP 只读返回当前项目的应用运行环境规划、初始化状态、依赖服务和应用镜像，供 Task Runner 执行 Agent 判断某项操作是否依赖项目级运行拓扑。

使用规则：
1. 开始处理项目任务时先调用 `get_project_runtime_environment_info`，不要根据历史对话猜测环境。
2. 返回内容包括当前生效环境变量、Agent 生成的环境配置文件、识别到的技术栈、依赖服务和已准备镜像。
3. 环境变量和生成文件是项目级应用运行环境的权威初始化结果。启动应用、连接依赖服务或操作 Project Gateway application target 时应以这些信息为准。
4. 这个 MCP 不修改项目、不生成镜像，也不更新环境；需要重新初始化环境时应交回 Project Environment Agent。
5. 如果环境状态不是 `ready`，不得假设项目应用、依赖服务或 Project Gateway application target 已经可运行；需要这些能力时必须明确说明缺失或失败信息。
6. 项目运行环境状态不等于当前 Task Runner 基础执行沙箱状态。如果本轮已经暴露 `TerminalController`、`CodeMaintainerRead` 或 `CodeMaintainerWrite`，这些工具就是当前运行真实可用的文件/终端执行面；通用代码维护、文件读写、版本检查和命令验证应直接调用并依据真实结果判断，不能仅因项目环境为 `pending` 而阻塞。
7. 只有明确依赖项目应用服务、依赖容器、项目环境变量、生成配置或 Project Gateway application target 的操作，才要求项目运行环境为 `ready`。不要把这一限制扩大到 Task Runner 已经准备好的独立基础沙箱。
