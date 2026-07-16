这个 MCP 只负责当前项目的运行环境状态，不负责修改项目代码或创建 Task Runner 任务。

使用规则：
1. 开始分析时先读取当前项目运行环境，确认已有状态、沙箱开关、Provider 和历史结果。
2. 第一阶段必须先扫描整个项目的环境变量：列目录、全局搜索变量引用，读取 `.env*`、Spring 配置、Compose/Dockerfile、Kubernetes/Helm、CI、启动脚本、README 和命中的源代码。形成完整变量清单前禁止生成镜像 Dockerfile 计划。
3. 第二阶段确定每个变量唯一的当前值：适用的项目值直接使用；缺失或不适配时由 AI 生成当前沙箱可用值，并记录原因、必填和敏感属性。
4. 第三阶段确定运行时、依赖服务、端口和启动方式，然后根据扫描结果和唯一变量值生成环境专用配置文件，例如 `.env.chatos`、`application-chatos.yml` 或配置中心 YAML。不得覆盖项目原文件，敏感值使用环境变量占位符。
5. 环境配置文件生成完成后，为应用运行时和每个依赖服务分别生成完整 Dockerfile 和可选 `custom_build_script`，记录状态为 `planned`。本轮不实际申请镜像，真实构建由用户在页面点击“生成镜像”触发。
6. 完成分析后必须调用更新工具写回状态，并提交 `environment_variable_scan` 扫描证据、`environment_variables`、`generated_config_files`、依赖服务和镜像构建计划。项目确实不需要配置文件时提交空数组。
7. 采用 provisioning-first 策略：识别到应用运行时就准备应用镜像；识别到 Redis、MongoDB、MySQL、Postgres、Nacos、RabbitMQ 等依赖就准备对应本地环境并生成连接变量。远程配置或连接信息缺失不能作为 `not_runnable` 理由。
8. 环境变量中出现 MYSQL、MONGO、REDIS、NACOS、POSTGRES、RABBITMQ、KAFKA、ELASTICSEARCH 或 MINIO 等引用，也必须视为对应依赖；不得通过关闭配置开关或漏写依赖来跳过计划生成。应用运行时和每个依赖服务都要有独立 Dockerfile。
9. 只有项目为空、没有可执行入口或构建清单、仅有文档/零散配置且无法识别任何可启动组件时才允许写入 `not_runnable`。需要无法自动生成的第三方业务凭据时使用 `pending_configuration`。
10. 可运行且所有 Dockerfile 计划准备完成时提交分析结果；整体状态由服务保存为 `pending_image_build`，不得伪造 image_id 或提前写成已生成。
11. 只更新当前项目，不臆造依赖、镜像 ID、端口、环境变量、配置内容或执行结果。
