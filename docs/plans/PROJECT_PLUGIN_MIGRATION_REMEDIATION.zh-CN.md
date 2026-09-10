# 项目管理插件与 Task Runner Bridge 删除清单

状态：代码删除与跨平台静态验证完成，待客户端真机验收
更新日期：2026-09-10

本清单记录最终删除范围。此前“完成项目管理插件”的方案已经取消，验收目标改为彻底移除插件及其专属基础设施。

## 已确认的问题

- 删除原生 Plan 后曾错误地把项目管理插件放入项目页面，造成产品入口重复。
- 项目管理插件的界面与功能未达到可交付产品标准。
- 插件退出后，专门为它增加的 Task Runner Bridge、任务工作区和默认模型字段失去消费者。
- Windows 与 macOS、共享 SDK、部署脚本、官网和文档都可能留下单边残留，不能只删除一个插件目录。

## 删除项

- [x] 删除 `plugins/project-management` 全部受跟踪源码和本机构建产物。
- [x] 删除插件构建、测试、打包、发布、镜像和官网展示入口。
- [x] 删除 macOS Task Runner Bridge、插件任务工作区及默认模型设置。
- [x] 删除 Windows Task Runner Bridge、插件任务工作区及默认模型设置。
- [x] 删除共享 SDK 中三个专属 Bridge capability 与反序列化枚举。
- [x] 删除 User Service / ChatOS 的 `task_runner_default_model_config_id` 字段、校验和透传。
- [x] 删除两端旧项目清单导入协议、macOS 导入界面及本地导入回执存储。
- [x] 删除旧 Connector 工作区自动迁移和插件未声明上下文时的隐式 workspace 注入。
- [x] 将 Windows 插件 capability 白名单收缩为 `host.context.read`。
- [x] 更新项目绑定文档，明确客户端项目注册表是唯一权威。
- [x] 完成 Rust、macOS 与 Windows 非 WinUI 目标编译测试及发布脚本验证。
- [ ] 完成 macOS 重装验收和 Windows 真机验收。
- [x] 提交删除变更。
- [ ] GitHub 网络恢复后推送变更。

## 不删除项

- 通用 Task Runner 服务及普通会话的后台任务能力。
- 客户端“应用”侧栏和通用插件应用宿主。
- 其他插件使用的项目 scope、运行数据隔离和 `host.context.read`。
- 客户端项目注册表、Git、文件、终端与运行能力。

## 搜索门禁

生产代码必须不再出现：

```text
task.batch.prepare
task.batch.status
task.workspace.open
TaskRunnerHostService
PluginTaskWorkspace
task_runner_default_model_config_id
plugins/project-management
ProjectManagementPlugin
```

历史设计资料如保留，必须明确标注已废弃，不能被 README、构建或发布流程引用。
