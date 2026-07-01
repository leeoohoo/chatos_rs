// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const DEFAULT_ENGINE_SUMMARY_PROMPT_TEMPLATE: &str = "你是 memory engine 的消息总结引擎。你的任务不是泛泛概括聊天内容，而是为后续对话提供承上启下的会话摘要。\n请优先保留以下信息：\n1. 当前会话已经完成了什么，哪些动作、结论、修改或验证已经发生；\n2. 当前正在进行什么，眼下卡在哪一步，哪些问题还没有解决；\n3. 接下来最可能继续做什么，明确下一步待办、待确认项和推进顺序；\n4. 对后续续聊有决定作用的重要事实、约束、风险、假设、路径、文件、命令、环境信息和用户要求；\n5. 如果用户临时改变目标，也要保留目标切换前后的上下文关系。\n请避免空泛复述，避免写成长篇会议纪要；输出应简洁、连续、可直接帮助下一轮对话衔接。";

pub const DEFAULT_ENGINE_SUMMARY_PROMPT_TEMPLATE_EN: &str = "You are the message summarization engine for the memory engine. Your task is not to give a generic recap, but to produce a session summary that helps the next turn continue smoothly.\nPrioritize preserving the following information:\n1. What has already been completed in the current session, including actions taken, conclusions reached, code changes made, and validations performed;\n2. What is currently in progress, where things are blocked, and which issues remain unresolved;\n3. What is most likely to happen next, including concrete next steps, items pending confirmation, and the expected order of work;\n4. Important facts, constraints, risks, assumptions, paths, files, commands, environment details, and user requirements that will matter for future turns;\n5. If the user changes direction midstream, preserve the relationship between the old goal and the new goal.\nAvoid vague restatement and avoid turning the output into long meeting minutes. The result should be concise, continuous, and directly useful for resuming the conversation.";

pub const DEFAULT_ENGINE_ROLLUP_PROMPT_TEMPLATE: &str = "你是 memory engine 的高层知识归纳引擎。你的任务是把多份会话总结继续压缩成更稳定、更长期可复用的项目级知识。\n请优先沉淀以下内容：\n1. 当前项目或任务的整体全貌、核心目标、关键模块和阶段进展；\n2. 项目常用的技能、方法、工作套路、排障路径和协作方式；\n3. 重要的环境信息，例如技术栈、依赖、运行方式、部署方式、目录结构、接口边界、配置约定；\n4. 稳定成立的事实、决策、约束、风险边界和长期有效的经验；\n5. 对后续继续工作有指导意义的共识，而不是一次性的闲聊细节。\n请把内容组织成有层次的知识摘要，强调“整体认知”和“长期有效信息”，弱化瞬时噪声。";

pub const DEFAULT_ENGINE_ROLLUP_PROMPT_TEMPLATE_EN: &str = "You are the higher-level knowledge consolidation engine for the memory engine. Your task is to keep compressing multiple conversation summaries into more stable, longer-lived project knowledge.\nPrioritize distilling the following:\n1. The overall shape of the current project or task, including core goals, key modules, and stage progress;\n2. Commonly used skills, methods, working patterns, debugging paths, and collaboration habits in the project;\n3. Important environment details such as the tech stack, dependencies, runtime flow, deployment approach, directory structure, interface boundaries, and configuration conventions;\n4. Stable facts, decisions, constraints, risk boundaries, and lessons that remain useful over time;\n5. Shared understanding that will guide future work, rather than one-off chat details.\nOrganize the content as a structured knowledge summary. Emphasize overall understanding and long-lived information, while down-weighting transient noise.";

pub const DEFAULT_ENGINE_SUBJECT_MEMORY_PROMPT_TEMPLATE: &str = "你是 memory engine 的记忆提炼引擎。你的任务是把阶段性总结提炼成长期可召回的主体记忆，重点服务于用户理解和智能体常识积累。\n请优先沉淀以下内容：\n1. 用户画像：用户的做事风格、沟通方式、偏好、习惯、容忍度、关注重点、长期目标和决策倾向；\n2. 协作习惯：用户喜欢怎样推进工作、怎样接收结果、怎样给反馈、怎样定义完成；\n3. 智能体可复用的常识积累：在长期工作里逐渐验证出的经验、模式、准则、常见坑、稳定有效的方法；\n4. 能跨会话复用的关系信息，例如用户与项目、工具、环境、任务类型之间的稳定关联；\n5. 对后续个性化协作真正有帮助的长期信息，而不是一次性的临时状态。\n请尽量输出高密度、可复用、可长期保留的记忆条目，避免写成单次会话摘要。";

pub const DEFAULT_ENGINE_SUBJECT_MEMORY_PROMPT_TEMPLATE_EN: &str = "You are the memory distillation engine for the memory engine. Your task is to turn stage summaries into long-lived, retrievable subject memories that mainly support user understanding and reusable agent knowledge.\nPrioritize distilling the following:\n1. A user profile: how the user works, communicates, what they prefer, their habits, tolerance, focus areas, long-term goals, and decision tendencies;\n2. Collaboration habits: how the user likes work to be advanced, how they prefer results to be presented, how they give feedback, and how they define completion;\n3. Reusable agent knowledge built over time: validated experience, recurring patterns, practical rules, common pitfalls, and methods that stay effective;\n4. Relationship information that can be reused across sessions, such as stable links between the user and projects, tools, environments, or task types;\n5. Long-term information that genuinely helps future personalized collaboration, rather than one-off temporary states.\nAim for dense, reusable, durable memory entries, and avoid writing this like a single-session summary.";

pub const DEFAULT_ENGINE_THREAD_REPAIR_PROMPT_TEMPLATE: &str = "你是 memory engine 的修复总结引擎。你的目标不是普通压缩，而是在上下文已经偏离时恢复可信的会话状态。\n请严格遵守：\n1. 把用户的明确消息当作最高优先级事实来源；\n2. 只保留能被会话记录支撑的事实；\n3. 如果助手说法带有猜测、缺少依据或被用户纠正，请标记为未验证或错误；\n4. 优先保留用户纠正、用户约束、明确证据、当前真实进展和下一步要求；\n5. 不知道的内容直接说明未知，不要补全空白；\n6. 最终总结必须帮助下一轮助手从修正后的理解继续，而不是继承旧错误。\n请按以下固定小节输出：\n已确认事实\n错误或未验证说法\n仍不清楚\n下一轮约束\n当前用户目标";

pub const DEFAULT_ENGINE_THREAD_REPAIR_PROMPT_TEMPLATE_EN: &str = "You are generating a repair summary for a memory engine.\nYour goal is not ordinary compression. Your goal is to restore a trustworthy context state when the assistant has drifted into a wrong direction.\n\nCore rules:\n1. Treat explicit user messages as the primary source of truth.\n2. Keep only facts grounded in the conversation records.\n3. If assistant claims look speculative, unsupported, or contradicted by the user, mark them as unverified or incorrect.\n4. Prefer explicit user corrections, user constraints, and concrete evidence from the conversation.\n5. If something is unknown, say it is unknown rather than filling gaps.\n6. Highlight what has already been done, what is actually in progress, and what the next model should do next.\n7. The final summary must help the next assistant continue from the corrected understanding instead of inheriting the old mistake.\n\nOutput sections with these exact headings:\nConfirmed Facts\nIncorrect Or Unverified Claims\nStill Unclear\nNext-Turn Constraints\nCurrent User Goal";

pub const DEFAULT_ENGINE_MEMORY_ROLLUP_PROMPT_TEMPLATE: &str = "你是 memory engine 的人格与长期特质归纳引擎。你的任务是把已经形成的主体记忆继续向上抽象，沉淀出智能体在长期工作中逐步形成的稳定人格和性格特征。\n请优先归纳以下内容：\n1. 智能体在长期协作中逐渐稳定下来的处事风格、表达风格、判断方式和价值倾向；\n2. 在不同任务场景下反复体现出来的性格特质，例如谨慎、直接、耐心、系统化、偏执行或偏探索；\n3. 与用户长期磨合后形成的合作气质、互动节奏和角色定位；\n4. 能代表智能体“长期自我”的高层特征，而不是零散习惯或短期状态；\n5. 这些人格特征如何帮助后续工作保持一致性、连续性和可预期性。\n请输出更高层、更抽象但仍然真实可落地的人格描述，避免空洞标签化。";

pub const DEFAULT_ENGINE_MEMORY_ROLLUP_PROMPT_TEMPLATE_EN: &str = "You are the personality and long-term trait consolidation engine for the memory engine. Your task is to keep abstracting existing subject memories upward so they capture the stable personality and character traits an agent develops through long-term work.\nPrioritize synthesizing the following:\n1. The agent's increasingly stable working style, expression style, judgment style, and value tendencies in long-term collaboration;\n2. Personality traits that repeatedly show up across task settings, such as caution, directness, patience, systematic thinking, an execution bias, or an exploration bias;\n3. The collaborative tone, interaction rhythm, and role positioning formed after long-term coordination with the user;\n4. Higher-level traits that represent the agent's long-term self, rather than scattered habits or short-lived states;\n5. How those personality traits help future work remain consistent, continuous, and predictable.\nProduce a higher-level, more abstract personality description that still feels grounded and real, and avoid empty label-driven wording.";

pub const PROMPT_LANGUAGE_ZH: &str = "zh";
pub const PROMPT_LANGUAGE_EN: &str = "en";

fn default_enabled() -> bool {
    true
}

fn default_prompt_language() -> String {
    PROMPT_LANGUAGE_ZH.to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineModelProfile {
    pub id: String,
    pub owner_user_id: Option<String>,
    pub owner_username: Option<String>,
    pub name: String,
    pub provider: String,
    pub model: String,
    pub base_url: Option<String>,
    pub api_key: Option<String>,
    #[serde(default)]
    pub supports_images: bool,
    #[serde(default)]
    pub supports_reasoning: bool,
    #[serde(default)]
    pub supports_responses: bool,
    pub temperature: Option<f64>,
    pub thinking_level: Option<String>,
    #[serde(default)]
    pub is_default: bool,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpsertEngineModelProfileRequest {
    pub id: Option<String>,
    pub name: String,
    pub provider: String,
    #[serde(alias = "model_name")]
    pub model: String,
    pub base_url: Option<String>,
    pub api_key: Option<String>,
    pub supports_images: Option<bool>,
    pub supports_reasoning: Option<bool>,
    pub supports_responses: Option<bool>,
    pub temperature: Option<f64>,
    pub thinking_level: Option<String>,
    pub is_default: Option<bool>,
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineJobPolicy {
    pub job_type: String,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    pub model_profile_id: Option<String>,
    pub summary_prompt: Option<String>,
    #[serde(default)]
    pub summary_prompt_zh: Option<String>,
    #[serde(default)]
    pub summary_prompt_en: Option<String>,
    #[serde(default = "default_prompt_language")]
    pub summary_prompt_language: String,
    pub rollup_summary_prompt: Option<String>,
    #[serde(default)]
    pub rollup_summary_prompt_zh: Option<String>,
    #[serde(default)]
    pub rollup_summary_prompt_en: Option<String>,
    #[serde(default = "default_prompt_language")]
    pub rollup_summary_prompt_language: String,
    pub token_limit: Option<i64>,
    pub target_summary_tokens: Option<i64>,
    pub interval_seconds: Option<i64>,
    pub max_threads_per_tick: Option<i64>,
    pub count_limit: Option<i64>,
    pub keep_level0_count: Option<i64>,
    pub max_level: Option<i64>,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpsertEngineJobPolicyRequest {
    pub enabled: Option<bool>,
    pub model_profile_id: Option<Option<String>>,
    pub summary_prompt: Option<Option<String>>,
    pub summary_prompt_zh: Option<Option<String>>,
    pub summary_prompt_en: Option<Option<String>>,
    pub summary_prompt_language: Option<String>,
    pub rollup_summary_prompt: Option<Option<String>>,
    pub rollup_summary_prompt_zh: Option<Option<String>>,
    pub rollup_summary_prompt_en: Option<Option<String>>,
    pub rollup_summary_prompt_language: Option<String>,
    pub token_limit: Option<Option<i64>>,
    pub target_summary_tokens: Option<Option<i64>>,
    pub interval_seconds: Option<Option<i64>>,
    pub max_threads_per_tick: Option<Option<i64>>,
    pub count_limit: Option<Option<i64>>,
    pub keep_level0_count: Option<Option<i64>>,
    pub max_level: Option<Option<i64>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenerateJobPolicyPromptRequest {
    pub prompt_field: String,
    pub user_input: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenerateJobPolicyPromptResponse {
    pub prompt_zh: String,
    pub prompt_en: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineJobRun {
    pub id: String,
    pub job_type: String,
    pub trigger_type: String,
    pub tenant_id: Option<String>,
    pub source_id: Option<String>,
    pub thread_id: Option<String>,
    pub subject_id: Option<String>,
    pub thread_label: Option<String>,
    pub thread_display_name: Option<String>,
    pub status: String,
    pub input_count: i64,
    pub output_count: i64,
    pub processed_count: i64,
    pub success_count: i64,
    pub error_count: i64,
    pub metadata: Option<Value>,
    pub error_message: Option<String>,
    pub started_at: String,
    pub finished_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobRunsBundleResponse {
    pub thread_runs: Vec<EngineJobRun>,
    pub scheduler_runs: Vec<EngineJobRun>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DashboardOverviewResponse {
    pub source_count: i64,
    pub model_count: i64,
    pub policy_count: i64,
    pub job_stats: serde_json::Value,
}

#[derive(Debug, Clone)]
pub struct CreateEngineJobRunRequest {
    pub job_type: String,
    pub trigger_type: String,
    pub tenant_id: Option<String>,
    pub source_id: Option<String>,
    pub thread_id: Option<String>,
    pub subject_id: Option<String>,
    pub thread_label: Option<String>,
    pub metadata: Option<Value>,
}

#[derive(Debug, Clone)]
pub struct FinishEngineJobRunRequest {
    pub status: String,
    pub input_count: i64,
    pub output_count: i64,
    pub processed_count: i64,
    pub success_count: i64,
    pub error_count: i64,
    pub metadata: Option<Value>,
    pub error_message: Option<String>,
}
