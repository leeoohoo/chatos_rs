# Frontend（React）

当前前端已实现一个可运行骨架（React + TypeScript + Vite）：
- 动态创建连接表单（按 db_type + auth_mode 联动字段）
- 连接列表与连接测试
- database summary 展示
- 元数据树懒加载浏览（按节点下钻/返回）
- 对象详情面板（列 / 索引 / 约束）

## 目录结构（已落地）

- `src/api/client.ts`：后端 API 调用
- `src/types/models.ts`：类型定义
- `src/components/layout`：页面壳
- `src/components/connections`：连接创建与列表
- `src/components/explorer`：对象树浏览
- `src/App.tsx`：页面编排

## 启动方式

```bash
cd db_connection_hub/frontend
npm install
npm run dev
```

默认端口：`5174`

## 环境变量

- `VITE_API_BASE_URL`（默认：`/api/v1`）
- `VITE_DEV_BACKEND_ORIGIN`（仅 dev 代理使用，默认：`http://127.0.0.1:8099`）

## 当前状态

- 已对接后端基础接口（列表/创建/测试/summary/nodes/object-detail）
- 组件和 API 已拆分，避免单文件过大
- 下一步可补分页节点加载、SQL 执行结果页、按数据库的高级认证字段校验
