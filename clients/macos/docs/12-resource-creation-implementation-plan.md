# 资源创建真实逻辑与实施清单

## 产品决定

- Swift 客户端完整替换旧 Local Connector Client，不嵌入 ChatOS Web 前端。
- 左侧“新建资源”不再提供“新建联系人”。新用户由 ChatOS 初始化服务创建唯一默认联系人“叽咕狸”。
- 新建项目成功后自动绑定“叽咕狸”，不再要求用户重复选择联系人。
- 本机终端直接使用 Swift 客户端内置终端，不再创建一条 ChatOS Terminal 资源，因此移除“新建终端”。
- 远端连接继续由 ChatOS 后端保存，但连接测试和实际 SSH 执行目标必须选择在线 Local Connector 工作区。

## 已核对的旧版项目创建流程

旧前端入口：

- `chatos/frontend/src/components/sessionList/useSessionListActions.ts`
- `chatos/frontend/src/components/sessionList/CreateResourceModals.tsx`

后端入口：

- `POST /local-connectors/projects`
- `chatos/backend/src/api/local_connectors.rs::create_project`

真实流程如下：

1. 获取当前在线 Local Connector device/workspace。
2. 在 workspace 内浏览目录，并允许新建目录。
3. 从所选目录名推导项目名，用户仍可修改。
4. 后端校验设备归属、在线状态、工作区状态和目录存在性。
5. 后端创建 ChatOS Project，并生成 `local://connector/<device>/<workspace>/<relativePath>` 内部路径。
6. 后端导入项目到 Harness。
7. 后端创建 `mcp` 与 `terminal` 两种 project binding。
8. 后端同步 Memory Engine；任一步失败时回滚项目、binding 与 Memory 状态。
9. Swift 客户端调用 `POST /projects/{projectId}/contacts`，默认绑定联系人“叽咕狸”。
10. 刷新资源列表并选中新项目。

因此，Swift 客户端不能绕过 `/local-connectors/projects` 自己创建项目或拼接多套后端状态。

## 远端连接真实字段

旧版远端连接不是简单的 Host/Port 表单，完整配置包括：

- 名称、Host、Port、用户名、默认远端目录。
- 认证方式：私钥、私钥 + 证书、密码。
- Host Key 策略：严格校验或首次接受。
- Local Connector 执行目标：device + workspace。
- 可选跳板机：复用已有连接，或手工填写跳板机凭据。
- 草稿连接测试、已保存连接测试。
- SSH 二次验证码挑战。

接口：

- `GET /remote-connections`
- `POST /remote-connections`
- `PUT /remote-connections/{id}`
- `DELETE /remote-connections/{id}`
- `POST /remote-connections/test`
- `POST /remote-connections/{id}/test`

## Swift 分层

| 层 | 职责 |
| --- | --- |
| ChatOSCore | 创建草稿、远端连接模型、Service protocol；不依赖 UI 和 HTTP |
| ChatOSAPI | 调用 ChatOS 项目/联系人/远端连接接口，DTO 映射 |
| ChatOSConnector | 在本机工作区内列目录、建目录、执行 SSH/Terminal relay |
| ChatOSApp ViewModel | 表单校验、加载/保存状态、错误恢复、默认联系人规则 |
| ChatOSApp SwiftUI | 原生 sheet、目录浏览、表单、空状态与进度反馈 |

## 按依赖排序的实施任务

### R1 新建项目

- [x] 核对旧版前后端真实流程。
- [x] 新增 Project Creation Core 模型和 API Service。
- [x] 原生目录浏览：工作区切换、进入/返回目录、新建文件夹、刷新。
- [x] 项目名由当前目录自动推导，同时允许修改。
- [x] 调用 `/local-connectors/projects`。
- [x] 自动绑定“叽咕狸”。
- [x] 创建后刷新并选中新项目。
- [x] 覆盖网关未连接、无工作区、无默认联系人、目录加载失败和部分创建失败。
- [x] API 单元测试与 Swift 全量测试。

验收：不打开 Web 页面、不调用旧客户端；选择真实本机目录后，项目能出现在侧栏、目录页可直接打开，并已绑定“叽咕狸”。

### R2 资源菜单

- [x] 删除“新建联系人”。
- [x] 删除“新建终端”。
- [x] 保留“新建项目”和“新建远端连接”。
- [x] 空状态下菜单和 sheet 布局不崩溃。

验收：菜单里没有无效入口，每一个可见入口都有真实操作。

### R3 远端连接管理

- [x] 新增远端连接 Core 模型与 API Service。
- [x] 加载真实远端连接列表并展示加载/错误状态。
- [x] 原生创建/编辑表单，覆盖完整旧版字段。
- [x] 私钥和证书使用 macOS 文件选择器。
- [x] 支持连接测试、Host Key 错误和二次验证码挑战。
- [x] 支持删除、刷新和快速重测。

验收：Swift 客户端可以独立完成旧前端已有的远端连接创建、编辑、测试和删除。

### R4 远端工作区

- [ ] 远端 Terminal 连接和断线恢复。
- [ ] 运行日志展开与分页。
- [ ] SFTP 目录浏览、上传、下载、重命名和删除。
- [ ] 空状态、离线、权限错误和大目录分页。

验收：点击侧栏远端连接进入真实工作区，不再显示占位页面。

### R5 回归与交付

- [x] 全量测试、Release 构建和启动验证。
- [ ] 检查无数据、慢网络和 Local Connector 离线状态。
- [x] 提交并推送到 `leeoohoo/chatos_swift`。
