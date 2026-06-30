# Project Management Service

独立项目管理微服务，后端使用 Rust + Axum + MongoDB，前端使用 React + Ant Design。

## 本地启动

先启动 `user_service`，项目管理服务会通过 User Service 校验登录令牌。

后端：

```bash
PROJECT_SERVICE_USER_SERVICE_BASE_URL=http://127.0.0.1:39190 \
PROJECT_SERVICE_DATABASE_URL=mongodb://admin:admin@127.0.0.1:27018/project_management_service?authSource=admin \
cargo run -p project_management_service_backend
```

默认数据库为 MongoDB。显式传入 `sqlite://...` 时仍可使用 SQLite fallback，主要用于本地快速测试。

前端：

```bash
cd project_management_service/frontend
npm install
npm run dev
```

默认地址：

- 后端：http://127.0.0.1:39210
- 前端：http://127.0.0.1:39211

## 领域边界

- `ProjectWorkItem` 是项目管理里的项目任务。
- `TaskRunnerTask` 是 TaskRunner 的执行任务。
- 二者不能复用同一张表。未来如需让项目任务触发 TaskRunner 执行，需要通过显式映射表关联。

## 从 TaskRunner 迁移项目

先给项目服务配置同步密钥：

```bash
export PROJECT_SERVICE_SYNC_SECRET=change_me_project_sync_secret
```

启动项目服务时也需要带上同一个 `PROJECT_SERVICE_SYNC_SECRET`。

预览迁移：

```bash
DRY_RUN=1 \
TASK_RUNNER_BASE_URL=http://127.0.0.1:39090 \
TASK_RUNNER_SYNC_SECRET=change_me_chatos_task_runner_secret \
PROJECT_SERVICE_BASE_URL=http://127.0.0.1:39210 \
PROJECT_SERVICE_SYNC_SECRET=change_me_project_sync_secret \
scripts/migrate_task_runner_projects_to_project_service.sh
```

执行迁移：

```bash
TASK_RUNNER_BASE_URL=http://127.0.0.1:39090 \
TASK_RUNNER_SYNC_SECRET=change_me_chatos_task_runner_secret \
PROJECT_SERVICE_BASE_URL=http://127.0.0.1:39210 \
PROJECT_SERVICE_SYNC_SECRET=change_me_project_sync_secret \
scripts/migrate_task_runner_projects_to_project_service.sh
```
