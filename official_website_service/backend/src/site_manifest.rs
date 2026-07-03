// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct SiteManifest {
    pub product_name: &'static str,
    pub tagline: &'static str,
    pub default_ports: Vec<DefaultPort>,
    pub services: Vec<ServiceInfo>,
    pub showcase_images: Vec<ShowcaseImage>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DefaultPort {
    pub name: &'static str,
    pub backend: Option<u16>,
    pub frontend: Option<u16>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ServiceInfo {
    pub name: &'static str,
    pub directory: &'static str,
    pub role: &'static str,
    pub capability: &'static str,
}

#[derive(Debug, Clone, Serialize)]
pub struct ShowcaseImage {
    pub id: &'static str,
    pub title: &'static str,
    pub path: &'static str,
    pub source_url: &'static str,
}

pub fn site_manifest() -> SiteManifest {
    SiteManifest {
        product_name: "Chatos RS",
        tagline: "让 AI 成为可以长期协作的联系人。",
        default_ports: vec![
            DefaultPort {
                name: "Chatos main",
                backend: Some(3997),
                frontend: Some(8088),
            },
            DefaultPort {
                name: "Memory Engine",
                backend: Some(7081),
                frontend: Some(4178),
            },
            DefaultPort {
                name: "Task Runner",
                backend: Some(39090),
                frontend: Some(39091),
            },
            DefaultPort {
                name: "User Service",
                backend: Some(39190),
                frontend: Some(39191),
            },
            DefaultPort {
                name: "Project Management",
                backend: Some(39210),
                frontend: Some(39211),
            },
            DefaultPort {
                name: "Sandbox Manager",
                backend: Some(8095),
                frontend: Some(8096),
            },
            DefaultPort {
                name: "Official Website",
                backend: Some(39250),
                frontend: Some(39251),
            },
        ],
        services: vec![
            ServiceInfo {
                name: "chat_app",
                directory: "chat_app/",
                role: "联系人驱动的主交互界面",
                capability: "选择虚拟联系人、发送消息、查看记忆、任务状态和运行上下文。",
            },
            ServiceInfo {
                name: "chat_app_server_rs",
                directory: "chat_app_server_rs/",
                role: "Rust 主编排后端",
                capability: "承载消息、流式响应、工具路由、运行上下文和跨服务编排。",
            },
            ServiceInfo {
                name: "memory_engine",
                directory: "memory_engine/",
                role: "长期记忆微服务",
                capability: "把线程、消息、摘要、主题记忆和上下文组装从主聊天中解耦。",
            },
            ServiceInfo {
                name: "task_runner_service",
                directory: "task_runner_service/",
                role: "异步执行链路",
                capability: "让复杂任务排队、执行、复核、回调，并保留可观察运行记录。",
            },
            ServiceInfo {
                name: "user_service",
                directory: "user_service/",
                role: "统一身份与模型配置",
                capability: "管理真实用户、agent account、令牌交换和共享模型配置。",
            },
            ServiceInfo {
                name: "project_management_service",
                directory: "project_management_service/",
                role: "工程计划管理",
                capability: "沉淀需求、技术方案、项目任务和依赖关系。",
            },
            ServiceInfo {
                name: "sandbox_manager_service",
                directory: "sandbox_manager_service/",
                role: "隔离执行底座",
                capability: "管理 Docker/Kata 沙箱租约、镜像初始化和沙箱 MCP 代理。",
            },
        ],
        showcase_images: vec![
            ShowcaseImage {
                id: "chatos-main",
                title: "联系人驱动的主聊天",
                path: "/showcase/chatos-main.png",
                source_url: "http://127.0.0.1:8088",
            },
            ShowcaseImage {
                id: "memory-engine",
                title: "Memory Engine 控制台",
                path: "/showcase/memory-engine.png",
                source_url: "http://127.0.0.1:4178",
            },
            ShowcaseImage {
                id: "task-runner",
                title: "Task Runner 运行台",
                path: "/showcase/task-runner.png",
                source_url: "http://127.0.0.1:39091",
            },
            ShowcaseImage {
                id: "sandbox-manager",
                title: "Sandbox Manager 管理台",
                path: "/showcase/sandbox-manager.png",
                source_url: "http://127.0.0.1:8096",
            },
            ShowcaseImage {
                id: "project-management",
                title: "Project Management 工作台",
                path: "/showcase/project-management.png",
                source_url: "http://127.0.0.1:39211",
            },
        ],
    }
}
