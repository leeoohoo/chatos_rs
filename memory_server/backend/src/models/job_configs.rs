use serde::{Deserialize, Serialize};

use super::{default_agent_memory_max_level, default_i64_1, default_keep_raw_level0_count};

pub const DEFAULT_SUMMARY_PROMPT_TEMPLATE: &str = "你是 Memory Server 的总结引擎。请输出结构化简洁总结，重点保留事实、决策、风险、待办。目标长度约 {{target_tokens}} tokens。";
pub const REVIEW_REPAIR_SUMMARY_PROMPT_TEMPLATE: &str = r#"你是 Memory Server 的“复盘纠偏总结”引擎。你的首要目标不是压缩内容，而是纠正智能体幻觉、隔离脏上下文、提炼后续对话应继续信任的真实信息。

请严格遵守以下规则：
1. 只把消息中“有直接依据的事实”写入总结。
2. 如果某条 assistant 内容明显是猜测、臆断、误读项目、伪造文件/功能/结论，必须明确标记为“疑似幻觉/未证实”，不能当作事实沉淀。
3. 如果用户纠正过 assistant，必须以用户纠正后的说法为准。
4. 如果消息里缺少证据，宁可写“未知/待确认”，也不要补全。
5. 总结的目标是帮助后续模型“掰正方向”，避免继续沿着错误前提推理。
6. 不要编造没有在原消息出现过的文件、接口、功能、结论、决定。
7. assistant 自己说过的话不是证据；只有用户明确说明、工具输出、代码/日志片段、或其他可核验内容才能算依据。
8. 如果 assistant 曾基于错误前提连续推理，必须把那条错误链整体标出，提醒后续模型不要继承。
9. 对文件路径、接口名、数据库字段、配置项、命令执行结果这类容易幻觉化的细节，未被证据直接支持前一律视为待确认。

请输出以下结构，标题保持一致：

## 已确认事实
- 仅保留从用户消息、工具输出、或明确证据中可以确认的事实

## 已发现的错误认知 / 幻觉
- 列出 assistant 曾经说错、猜错、误判的点
- 对每条标注：错误内容 / 为什么不可信 / 正确状态（若可判断）

## 仍待确认
- 当前还没有证据、后续必须重新检查的问题

## 后续回答约束
- 明确告诉后续模型：哪些前提不能再默认成立
- 明确要求优先基于工具结果、代码、日志、用户最新说明来回答

## 用户当前真实诉求
- 用简洁语言概括用户现在真正要解决的问题

目标长度约 {{target_tokens}} tokens。任务标题：{{prompt_title}}"#;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SummaryJobConfig {
    pub user_id: String,
    #[serde(default = "default_i64_1")]
    pub enabled: i64,
    pub summary_model_config_id: Option<String>,
    #[serde(default)]
    pub summary_prompt: Option<String>,
    pub token_limit: i64,
    pub round_limit: i64,
    pub target_summary_tokens: i64,
    pub job_interval_seconds: i64,
    pub max_sessions_per_tick: i64,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpsertSummaryJobConfigRequest {
    pub user_id: String,
    pub enabled: Option<bool>,
    pub summary_model_config_id: Option<Option<String>>,
    pub summary_prompt: Option<Option<String>>,
    pub token_limit: Option<i64>,
    pub round_limit: Option<i64>,
    pub target_summary_tokens: Option<i64>,
    pub job_interval_seconds: Option<i64>,
    pub max_sessions_per_tick: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SummaryRollupJobConfig {
    pub user_id: String,
    #[serde(default = "default_i64_1")]
    pub enabled: i64,
    pub summary_model_config_id: Option<String>,
    #[serde(default)]
    pub summary_prompt: Option<String>,
    pub token_limit: i64,
    pub round_limit: i64,
    pub target_summary_tokens: i64,
    pub job_interval_seconds: i64,
    pub keep_raw_level0_count: i64,
    pub max_level: i64,
    pub max_sessions_per_tick: i64,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentMemoryJobConfig {
    pub user_id: String,
    #[serde(default = "default_i64_1")]
    pub enabled: i64,
    pub summary_model_config_id: Option<String>,
    #[serde(default)]
    pub summary_prompt: Option<String>,
    pub token_limit: i64,
    pub round_limit: i64,
    pub target_summary_tokens: i64,
    pub job_interval_seconds: i64,
    #[serde(default = "default_keep_raw_level0_count")]
    pub keep_raw_level0_count: i64,
    #[serde(default = "default_agent_memory_max_level")]
    pub max_level: i64,
    pub max_agents_per_tick: i64,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpsertAgentMemoryJobConfigRequest {
    pub user_id: String,
    pub enabled: Option<bool>,
    pub summary_model_config_id: Option<Option<String>>,
    pub summary_prompt: Option<Option<String>>,
    pub token_limit: Option<i64>,
    pub round_limit: Option<i64>,
    pub target_summary_tokens: Option<i64>,
    pub job_interval_seconds: Option<i64>,
    pub keep_raw_level0_count: Option<i64>,
    pub max_level: Option<i64>,
    pub max_agents_per_tick: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpsertSummaryRollupJobConfigRequest {
    pub user_id: String,
    pub enabled: Option<bool>,
    pub summary_model_config_id: Option<Option<String>>,
    pub summary_prompt: Option<Option<String>>,
    pub token_limit: Option<i64>,
    pub round_limit: Option<i64>,
    pub target_summary_tokens: Option<i64>,
    pub job_interval_seconds: Option<i64>,
    pub keep_raw_level0_count: Option<i64>,
    pub max_level: Option<i64>,
    pub max_sessions_per_tick: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobRun {
    pub id: String,
    pub job_type: String,
    pub session_id: Option<String>,
    pub status: String,
    pub trigger_type: Option<String>,
    pub input_count: i64,
    pub output_count: i64,
    #[serde(default)]
    pub pending_before_count: Option<i64>,
    #[serde(default)]
    pub selected_count: Option<i64>,
    #[serde(default)]
    pub marked_count: Option<i64>,
    #[serde(default)]
    pub pending_after_count: Option<i64>,
    pub error_message: Option<String>,
    pub started_at: String,
    pub finished_at: Option<String>,
}
