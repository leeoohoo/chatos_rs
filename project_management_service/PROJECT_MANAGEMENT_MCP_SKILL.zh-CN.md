---
name: project-management-mcp-agent-zh-cn
description: 中文指南，指导 AI agent 通过 Project Management MCP 管理项目基础资料、项目背景/介绍、需求、项目任务、依赖关系与需求技术总体文档。
---

# Project Management MCP Agent Skill

Project Management MCP 是项目管理微服务对外提供的项目结构化管理入口。它管理的是项目资料、需求、项目任务和依赖关系。

## 核心规则

- 把 `project_task` 理解为项目管理里的任务/工作项，也就是 `ProjectWorkItem`。
- 创建新需求或项目任务前，优先使用列表/概览工具检查是否已经存在同义内容，能更新就不要重复创建。
- 依赖工具使用“完整替换列表”语义。调用前先确认现有依赖，避免误删用户已维护的前置关系。
- 需求之间可以有前置需求；同一个需求下面的项目任务之间也可以有前置项目任务。
- 需求的技术总体文档对应 `upsert_requirement_technical_overview`，用于保存实现方案、架构说明、接口设计、数据结构、风险点等总体技术内容。
- 创建项目任务前，必须确保该需求的技术总体文档已有非空内容；如果为空，先调用 `upsert_requirement_technical_overview` 补齐，再调用 `create_project_task`。
- 当项目描述、项目背景或项目介绍为空、明显过短或已经落后于当前需求时，要主动维护这些项目资料。优先基于用户已提供的信息、项目名、根目录、Git 地址、已有需求、已有项目任务、需求技术总体文档以及当前上下文中可见的 README/docs/配置文件等线索整理；能确认的内容直接调用 `initialize_project` 补充，不能确认时先向用户提出关键问题，不要编造。
- 项目背景、项目介绍和需求技术总体文档都按 Markdown 长文档维护。优先使用清晰的小标题、列表、关键约束、范围边界和风险说明，避免只写一句口号式描述。

## 工具清单

- `get_project_overview`: 查询项目基础信息和一对一 profile。
- `initialize_project`: 初始化或增量更新项目基础资料、背景和介绍。
- `list_requirements`: 查询项目需求。
- `create_requirement`: 创建项目需求。
- `update_requirement`: 更新需求，并可同时替换前置需求。
- `set_requirement_dependencies`: 替换某个需求的前置需求列表。
- `upsert_requirement_technical_overview`: 创建或更新需求的实现技术总体文档。
- `get_requirement_technical_overview`: 读取需求的实现技术总体文档。
- `list_project_tasks`: 查询项目管理任务/工作项。
- `create_project_task`: 在某个需求下创建项目管理任务/工作项；要求该需求已有非空技术总体文档内容。
- `update_project_task`: 更新项目管理任务/工作项，并可同时替换前置项目任务。
- `set_project_task_dependencies`: 替换某个项目任务的前置项目任务列表。
- `get_project_dependency_graph`: 查询项目级需求、项目任务和依赖图。

## 推荐工作流

1. 调用 `get_project_overview`，了解当前项目已有资料。
2. 如果项目描述、背景或介绍缺失，先主动探测可用上下文：已有项目资料、需求、项目任务、技术总体文档，以及当前上下文中可见的仓库说明或配置。能归纳出可靠内容时，用 `initialize_project` 增量补齐。
3. 如果缺失资料无法从现有线索可靠推断，向用户询问少量关键问题，再写入项目资料。
4. 调用 `list_requirements` 检查已有需求，避免重复创建。
5. 使用 `create_requirement` 创建新需求，或用 `update_requirement` 调整已有需求。
6. 创建项目任务前，读取或维护该需求的技术总体文档；文档内容为空时，先调用 `upsert_requirement_technical_overview`。
7. 调用 `list_project_tasks` 或按需求查看已有项目任务后，再用 `create_project_task` 创建任务。
8. 使用 `set_requirement_dependencies` 和 `set_project_task_dependencies` 建立前置关系。
9. 调用 `get_project_dependency_graph` 复核依赖图是否符合用户意图。
