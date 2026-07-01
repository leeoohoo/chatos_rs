// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use memory_engine_sdk::{
    SdkComposeContextRequest, SdkCountThreadRecordsRequest, SdkListThreadRecordsRequest,
};

use crate::models::{
    now_rfc3339, TaskMemoryContextOptions, TaskMemoryContextResponse, TaskMemoryRecordsOptions,
    TaskMemoryRecordsResponse, TaskMemorySummaryResponse, TaskRecord,
};

use super::memory_options::{
    sanitize_task_memory_context_policy, sanitize_task_memory_records_options,
};
use super::task_threads::ensure_task_thread_for_config;
use super::{save_task_if_tenant_aligned, TaskService};

impl TaskService {
    pub async fn get_task_memory_context(
        &self,
        id: &str,
        options: TaskMemoryContextOptions,
    ) -> Result<Option<TaskMemoryContextResponse>, String> {
        let Some(task) = self.get_task_with_aligned_memory_tenant(id).await? else {
            return Ok(None);
        };
        let client = self.require_memory_client()?;
        let thread = client
            .get_thread(&task.memory_thread_id, Some(&task.tenant_id))
            .await?;

        let total_record_count = if thread.is_some() {
            client
                .count_thread_records(
                    &task.memory_thread_id,
                    &SdkCountThreadRecordsRequest {
                        tenant_id: task.tenant_id.clone(),
                        role: None,
                        record_type: None,
                        summary_status: None,
                    },
                )
                .await?
        } else {
            0
        };

        let context = if thread.is_some() {
            Some(
                client
                    .compose_context(&SdkComposeContextRequest {
                        tenant_id: task.tenant_id.clone(),
                        subject_id: Some(task.subject_id.clone()),
                        related_subject_ids: None,
                        thread_id: task.memory_thread_id.clone(),
                        policy: Some(sanitize_task_memory_context_policy(options)),
                    })
                    .await?,
            )
        } else {
            None
        };

        Ok(Some(TaskMemoryContextResponse {
            task_id: task.id,
            memory_thread_id: task.memory_thread_id,
            tenant_id: task.tenant_id,
            subject_id: task.subject_id,
            thread,
            context,
            total_record_count,
        }))
    }

    pub async fn get_task_memory_records(
        &self,
        id: &str,
        options: TaskMemoryRecordsOptions,
    ) -> Result<Option<TaskMemoryRecordsResponse>, String> {
        let Some(task) = self.get_task_with_aligned_memory_tenant(id).await? else {
            return Ok(None);
        };
        let client = self.require_memory_client()?;
        let thread = client
            .get_thread(&task.memory_thread_id, Some(&task.tenant_id))
            .await?;
        let options = sanitize_task_memory_records_options(options);
        let limit = options.limit.unwrap_or(50);
        let offset = options.offset.unwrap_or(0);
        let order = options.order.clone().unwrap_or_else(|| "desc".to_string());

        let Some(thread) = thread else {
            return Ok(Some(TaskMemoryRecordsResponse {
                task_id: task.id,
                memory_thread_id: task.memory_thread_id,
                tenant_id: task.tenant_id,
                subject_id: task.subject_id,
                thread: None,
                total: 0,
                limit,
                offset,
                order,
                role: options.role,
                record_type: options.record_type,
                summary_status: options.summary_status,
                has_more: false,
                items: Vec::new(),
            }));
        };

        let page = client
            .list_thread_records_page(
                &task.memory_thread_id,
                &SdkListThreadRecordsRequest {
                    tenant_id: task.tenant_id.clone(),
                    role: options.role.clone(),
                    record_type: options.record_type.clone(),
                    summary_status: options.summary_status.clone(),
                    limit: Some(limit),
                    offset: Some(offset),
                    order: Some(order.clone()),
                },
            )
            .await?;

        Ok(Some(TaskMemoryRecordsResponse {
            task_id: task.id,
            memory_thread_id: task.memory_thread_id,
            tenant_id: task.tenant_id,
            subject_id: task.subject_id,
            thread: Some(thread),
            total: page.total,
            limit,
            offset,
            order,
            role: options.role,
            record_type: options.record_type,
            summary_status: options.summary_status,
            has_more: page.total > offset + page.items.len() as i64,
            items: page.items,
        }))
    }

    pub async fn summarize_task_memory(
        &self,
        id: &str,
    ) -> Result<Option<TaskMemorySummaryResponse>, String> {
        let Some(task) = self.get_task_with_aligned_memory_tenant(id).await? else {
            return Ok(None);
        };
        let client = self.require_memory_client()?;
        let result = client
            .run_thread_repair_summary(&task.memory_thread_id, &task.tenant_id)
            .await?;
        Ok(Some(TaskMemorySummaryResponse {
            task_id: task.id,
            memory_thread_id: task.memory_thread_id,
            tenant_id: task.tenant_id,
            requested_at: now_rfc3339(),
            result,
        }))
    }

    pub(super) fn require_memory_client(
        &self,
    ) -> Result<memory_engine_sdk::MemoryEngineClient, String> {
        self.config
            .memory_client()?
            .ok_or_else(|| "Memory Engine 未配置，无法读取任务上下文".to_string())
    }

    pub(super) async fn ensure_task_thread(&self, task: &TaskRecord) -> Result<(), String> {
        ensure_task_thread_for_config(&self.config, task).await
    }

    async fn get_task_with_aligned_memory_tenant(
        &self,
        id: &str,
    ) -> Result<Option<TaskRecord>, String> {
        let Some(task) = self.store.get_task(id).await? else {
            return Ok(None);
        };
        save_task_if_tenant_aligned(&self.store, task)
            .await
            .map(Some)
    }
}
