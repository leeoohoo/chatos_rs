# DB Connection Hub Service（方案草案）

这是一个独立的新服务目录，目标是提供统一的数据库连接与元数据浏览能力：
- 后端：Rust
- 前端：React
- 支持主流数据库：PostgreSQL、MySQL/MariaDB、SQLite、SQL Server、Oracle、MongoDB（可扩展）

当前阶段目标：
- 先做“纯数据库连接工具”
- AI 相关能力暂缓到下一阶段

## 目录结构

- `docs/implementation-plan.md`：整体架构与落地方案（纯连接工具）
- `docs/authentication-support-matrix.md`：主流数据库认证方式支持矩阵
- `docs/metadata-explorer-design.md`：数据库对象树浏览设计（database/table/index/view 等）
- `docs/api-contract.md`：核心 API 草案
- `mockups/dashboard.svg`：连接管理与运行概览
- `mockups/connection-wizard.svg`：新建连接向导
- `mockups/sql-workbench.svg`：SQL 工作台页面草图
- `backend/README.md`：Rust 服务分层建议
- `frontend/README.md`：React 前端分层建议

## 设计重点

1. 插件化数据库驱动层（统一抽象 + 按数据库实现）
2. 完整认证支持（账号密码、TLS/mTLS、Token、Kerberos 等分阶段落地）
3. 对象树浏览统一抽象（不同数据库结构差异统一映射）
4. 元数据统计能力（database 数量、table/index/view 数量）
5. 稳定性与安全（连接池、超时、审计、凭据加密）

## 核心交互目标

1. 点击某个连接：看到该连接下有多少 `database`（或同层级对象）
2. 点击某个 database：看到 `table / view / index / procedure / function ...` 各类对象数量
3. 继续下钻：按层级浏览具体对象详情（字段、索引列、约束等）

## 下一步建议

1. 先完成 Rust 后端：连接管理 + 认证适配 + 元数据树 API
2. 再完成 React：连接列表、连接向导、对象浏览树
3. 已完成 PostgreSQL/MySQL/SQLite/SQL Server/MongoDB 真实驱动，Oracle 已接入第一阶段真实驱动（网络探测 + 部分元数据）

## 当前进度（已落地代码）

1. 后端（Rust）已完成模块化骨架并可运行：
- `GET/POST/PUT datasources`
- `POST datasources/{id}/test`
- `GET datasources/{id}/databases/summary`
- `GET datasources/{id}/databases`
- `GET datasources/{id}/databases/{db}/object-stats`
- `GET metadata/nodes`
- `GET metadata/object-detail`
- `POST queries/execute` 与 `POST queries/{id}/cancel`

2. 前端（React）已完成模块化页面骨架：
- 创建连接
- 连接列表 + 测试连接
- database summary 展示
- 元数据树下钻浏览

3. 当前驱动层采用“混合模式”：`PostgreSQL`、`MySQL`、`SQLite`、`SQL Server`、`MongoDB` 已接真实驱动，`Oracle` 已接入第一阶段真实驱动（仍有部分能力待补齐）。
4. Oracle 第一阶段当前能力：
- 真实网络探测与认证参数校验
- metadata 树已支持：`database -> schema -> table/view/materialized_view/sequence/procedure/function/synonym/package`，`table -> index/trigger`
- `object-stats` 返回上述对象类型的统计（`partial=true`）
5. 元数据对象详情当前能力补充：
- PostgreSQL / MySQL / SQLite：已支持 `table/view` 详情，以及 `index/trigger` 详情下钻
- SQL Server：已支持 `table/view` 详情，以及 `index/trigger/procedure/function/sequence/synonym` 详情下钻
- MongoDB：已支持 `collection/view` 详情，以及 `index` 详情下钻
