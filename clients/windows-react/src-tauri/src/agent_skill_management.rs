use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    fs,
    io::Write,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, OnceLock},
};
use tauri::State;
use unicode_segmentation::UnicodeSegmentation;

use crate::{authentication::AuthenticationManager, host_lifecycle::validate_owner};

const SKILL_CATALOG: &str =
    include_str!("../../../macos/Sources/ChatOSCore/AgentSkills/Catalog/ChatOSSkillCatalog.json");
const SKILL_EXAMPLES: &str = include_str!("../resources/agent-skill-examples.json");
const MAXIMUM_PAYLOAD_BYTES: u64 = 256 * 1_024 * 1_024;

#[derive(Clone)]
pub(crate) struct AgentSkillStore {
    file: PathBuf,
    payload: Arc<Mutex<OverridePayload>>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct OverridePayload {
    #[serde(default = "schema_version")]
    schema_version: u8,
    #[serde(default)]
    accounts: HashMap<String, AccountOverrides>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct AccountOverrides {
    #[serde(default)]
    professions: HashMap<String, ProfessionOverride>,
    #[serde(default)]
    project_types: HashMap<String, ProjectTypeOverride>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProfessionOverride {
    label: String,
    description: String,
    skill_markdown: String,
    #[serde(rename = "labelEN", default)]
    label_en: Option<String>,
    #[serde(rename = "descriptionEN", default)]
    description_en: Option<String>,
    #[serde(rename = "skillMarkdownEN", default)]
    skill_markdown_en: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProjectTypeOverride {
    label: String,
    description: String,
    rule_markdown: String,
    #[serde(rename = "labelEN", default)]
    label_en: Option<String>,
    #[serde(rename = "descriptionEN", default)]
    description_en: Option<String>,
    #[serde(rename = "ruleMarkdownEN", default)]
    rule_markdown_en: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CatalogPayload {
    professions: Vec<CatalogProfession>,
    project_types: Vec<CatalogProjectType>,
}

#[derive(Clone, Debug, Deserialize)]
struct CatalogProfession {
    key: String,
    label: String,
    label_en: String,
    description: String,
    description_en: String,
    category_key: String,
    category_label: String,
    category_label_en: String,
    skill_markdown: String,
    skill_markdown_en: String,
}

#[derive(Clone, Debug, Deserialize)]
struct CatalogProjectType {
    key: String,
    label: String,
    label_en: String,
    description: String,
    description_en: String,
    category_key: String,
    category_label: String,
    category_label_en: String,
    rule_markdown: String,
    rule_markdown_en: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ExampleCatalog {
    professions: HashMap<String, ExampleProfile>,
    project_types: HashMap<String, ExampleProfile>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ExampleProfile {
    situation_zh: String,
    good_example_zh: String,
    evidence_zh: String,
    counterexample_zh: String,
    correction_zh: String,
    situation_en: String,
    good_example_en: String,
    evidence_en: String,
    counterexample_en: String,
    correction_en: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) enum AgentSkillKind {
    Profession,
    ProjectType,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AgentSkillDefinition {
    kind: AgentSkillKind,
    key: String,
    label: String,
    label_en: String,
    description: String,
    description_en: String,
    category_key: String,
    category_label: String,
    category_label_en: String,
    markdown: String,
    markdown_en: String,
    customized: bool,
    skill_ref: String,
    skill_ref_en: String,
    instructions_sha256: String,
    instructions_sha256_en: String,
    resources: Vec<AgentSkillResource>,
    resources_en: Vec<AgentSkillResource>,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub(crate) struct AgentProfessionRuntimeSnapshot {
    pub(crate) key: String,
    pub(crate) label: String,
    pub(crate) skill_ref: String,
    pub(crate) instructions_sha256: String,
    pub(crate) markdown: String,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct AgentSkillResource {
    relative_path: String,
    title: String,
    summary: String,
    markdown: String,
    size_bytes: usize,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct AgentSkillMutationInput {
    kind: AgentSkillKind,
    key: String,
    label: String,
    label_en: String,
    description: String,
    description_en: String,
    markdown: String,
    markdown_en: String,
}

fn schema_version() -> u8 {
    2
}

impl AgentSkillStore {
    pub(crate) fn new(file: PathBuf) -> Result<Self, String> {
        let payload = load_payload(&file)?;
        Ok(Self {
            file,
            payload: Arc::new(Mutex::new(payload)),
        })
    }

    pub(crate) fn from_runtime() -> Result<Self, String> {
        let file = dirs::data_local_dir()
            .ok_or_else(storage_error)?
            .join("com.chatos.windows")
            .join("connector")
            .join("AgentSkillOverrides.json");
        Self::new(file)
    }

    fn catalog(&self, owner_user_id: &str) -> Result<Vec<AgentSkillDefinition>, String> {
        let catalog = catalog_payload()?;
        let payload = self.payload.lock().map_err(|_| storage_error())?;
        let account = payload.accounts.get(owner_user_id);
        Ok(merged_catalog(&catalog, account))
    }

    pub(crate) fn profession_runtime_snapshot(
        &self,
        owner_user_id: &str,
        profession_key: &str,
    ) -> Result<AgentProfessionRuntimeSnapshot, String> {
        let definition = self
            .catalog(owner_user_id)?
            .into_iter()
            .find(|definition| {
                definition.kind == AgentSkillKind::Profession && definition.key == profession_key
            })
            .ok_or_else(|| "Agent 职业 Skill 不可用，请刷新后重试。".to_owned())?;
        Ok(AgentProfessionRuntimeSnapshot {
            key: definition.key,
            label: definition.label,
            skill_ref: definition.skill_ref,
            instructions_sha256: definition.instructions_sha256,
            markdown: definition.markdown,
        })
    }

    fn save(
        &self,
        owner_user_id: &str,
        input: AgentSkillMutationInput,
    ) -> Result<AgentSkillDefinition, String> {
        let catalog = catalog_payload()?;
        let base = catalog_item(&catalog, input.kind, input.key.trim())
            .ok_or_else(|| "Skill 标识无效，请刷新后重试。".to_owned())?;
        let label = validate_field(&input.label, "名称", 120)?;
        let label_en = validate_field(&input.label_en, "英文名称", 120)?;
        let description = validate_field(&input.description, "说明", 4_000)?;
        let description_en = validate_field(&input.description_en, "英文说明", 4_000)?;
        let markdown = validate_field(&input.markdown, "正文", 500_000)?;
        let markdown_en = validate_field(&input.markdown_en, "英文正文", 500_000)?;
        let mut payload = self.payload.lock().map_err(|_| storage_error())?;
        let mut next = payload.clone();
        next.schema_version = 2;
        let account = next.accounts.entry(owner_user_id.to_owned()).or_default();
        let saved = match input.kind {
            AgentSkillKind::Profession => {
                let value = ProfessionOverride {
                    label,
                    description,
                    skill_markdown: markdown,
                    label_en: Some(label_en),
                    description_en: Some(description_en),
                    skill_markdown_en: Some(markdown_en),
                };
                account.professions.insert(base.key.clone(), value.clone());
                apply_profession_override(base, Some(&value))
            }
            AgentSkillKind::ProjectType => {
                let value = ProjectTypeOverride {
                    label,
                    description,
                    rule_markdown: markdown,
                    label_en: Some(label_en),
                    description_en: Some(description_en),
                    rule_markdown_en: Some(markdown_en),
                };
                account
                    .project_types
                    .insert(base.key.clone(), value.clone());
                apply_project_type_override(base, Some(&value))
            }
        };
        persist_payload(&self.file, &next)?;
        *payload = next;
        Ok(saved)
    }

    fn reset(
        &self,
        owner_user_id: &str,
        kind: AgentSkillKind,
        key: &str,
    ) -> Result<AgentSkillDefinition, String> {
        let catalog = catalog_payload()?;
        let base = catalog_item(&catalog, kind, key.trim())
            .ok_or_else(|| "Skill 标识无效，请刷新后重试。".to_owned())?;
        let mut payload = self.payload.lock().map_err(|_| storage_error())?;
        let mut next = payload.clone();
        if let Some(account) = next.accounts.get_mut(owner_user_id) {
            match kind {
                AgentSkillKind::Profession => {
                    account.professions.remove(base.key.as_str());
                }
                AgentSkillKind::ProjectType => {
                    account.project_types.remove(base.key.as_str());
                }
            }
            if account.professions.is_empty() && account.project_types.is_empty() {
                next.accounts.remove(owner_user_id);
            }
        }
        persist_payload(&self.file, &next)?;
        *payload = next;
        Ok(base)
    }
}

#[tauri::command]
pub(crate) async fn load_agent_skill_catalog(
    owner_user_id: String,
    authentication: State<'_, AuthenticationManager>,
    store: State<'_, AgentSkillStore>,
) -> Result<Vec<AgentSkillDefinition>, String> {
    validate_owner(&owner_user_id)?;
    let generation = authentication
        .session_generation_for_owner(&owner_user_id)
        .await?;
    let query_store = store.inner().clone();
    let owner = owner_user_id.clone();
    let catalog = tauri::async_runtime::spawn_blocking(move || query_store.catalog(&owner))
        .await
        .map_err(|_| "Skill 目录读取任务异常退出。".to_owned())??;
    authentication
        .require_session_generation(&owner_user_id, generation)
        .await?;
    Ok(catalog)
}

#[tauri::command]
pub(crate) async fn save_agent_skill_override(
    owner_user_id: String,
    input: AgentSkillMutationInput,
    authentication: State<'_, AuthenticationManager>,
    store: State<'_, AgentSkillStore>,
) -> Result<AgentSkillDefinition, String> {
    validate_owner(&owner_user_id)?;
    let generation = authentication
        .session_generation_for_owner(&owner_user_id)
        .await?;
    authentication
        .require_session_generation(&owner_user_id, generation)
        .await?;
    let mutation_store = store.inner().clone();
    let owner = owner_user_id.clone();
    tauri::async_runtime::spawn_blocking(move || mutation_store.save(&owner, input))
        .await
        .map_err(|_| "Skill 保存任务异常退出。".to_owned())?
}

#[tauri::command]
pub(crate) async fn reset_agent_skill_override(
    owner_user_id: String,
    kind: AgentSkillKind,
    key: String,
    authentication: State<'_, AuthenticationManager>,
    store: State<'_, AgentSkillStore>,
) -> Result<AgentSkillDefinition, String> {
    validate_owner(&owner_user_id)?;
    let generation = authentication
        .session_generation_for_owner(&owner_user_id)
        .await?;
    authentication
        .require_session_generation(&owner_user_id, generation)
        .await?;
    let mutation_store = store.inner().clone();
    let owner = owner_user_id.clone();
    tauri::async_runtime::spawn_blocking(move || mutation_store.reset(&owner, kind, &key))
        .await
        .map_err(|_| "Skill 恢复任务异常退出。".to_owned())?
}

fn catalog_payload() -> Result<CatalogPayload, String> {
    serde_json::from_str(SKILL_CATALOG).map_err(|_| "Skill 内置目录不可用。".to_owned())
}

fn merged_catalog(
    catalog: &CatalogPayload,
    account: Option<&AccountOverrides>,
) -> Vec<AgentSkillDefinition> {
    catalog
        .professions
        .iter()
        .cloned()
        .map(AgentSkillDefinition::from)
        .map(|base| {
            let value = account.and_then(|account| account.professions.get(&base.key));
            apply_profession_override(base, value)
        })
        .chain(
            catalog
                .project_types
                .iter()
                .cloned()
                .map(AgentSkillDefinition::from)
                .map(|base| {
                    let value = account.and_then(|account| account.project_types.get(&base.key));
                    apply_project_type_override(base, value)
                }),
        )
        .collect()
}

fn catalog_item(
    catalog: &CatalogPayload,
    kind: AgentSkillKind,
    key: &str,
) -> Option<AgentSkillDefinition> {
    match kind {
        AgentSkillKind::Profession => catalog
            .professions
            .iter()
            .find(|value| value.key == key)
            .cloned()
            .map(Into::into),
        AgentSkillKind::ProjectType => catalog
            .project_types
            .iter()
            .find(|value| value.key == key)
            .cloned()
            .map(Into::into),
    }
}

fn apply_profession_override(
    mut base: AgentSkillDefinition,
    value: Option<&ProfessionOverride>,
) -> AgentSkillDefinition {
    if let Some(value) = value {
        base.label.clone_from(&value.label);
        base.description.clone_from(&value.description);
        base.markdown.clone_from(&value.skill_markdown);
        if let Some(value) = &value.label_en {
            base.label_en.clone_from(value);
        }
        if let Some(value) = &value.description_en {
            base.description_en.clone_from(value);
        }
        if let Some(value) = &value.skill_markdown_en {
            base.markdown_en.clone_from(value);
        }
        base.customized = true;
    }
    base.with_progressive_projection()
}

fn apply_project_type_override(
    mut base: AgentSkillDefinition,
    value: Option<&ProjectTypeOverride>,
) -> AgentSkillDefinition {
    if let Some(value) = value {
        base.label.clone_from(&value.label);
        base.description.clone_from(&value.description);
        base.markdown.clone_from(&value.rule_markdown);
        if let Some(value) = &value.label_en {
            base.label_en.clone_from(value);
        }
        if let Some(value) = &value.description_en {
            base.description_en.clone_from(value);
        }
        if let Some(value) = &value.rule_markdown_en {
            base.markdown_en.clone_from(value);
        }
        base.customized = true;
    }
    base.with_progressive_projection()
}

impl AgentSkillDefinition {
    fn with_progressive_projection(mut self) -> Self {
        self.instructions_sha256 = sha256(self.markdown.trim());
        self.instructions_sha256_en = sha256(self.markdown_en.trim());
        let kind = match self.kind {
            AgentSkillKind::Profession => "profession",
            AgentSkillKind::ProjectType => "project_type",
        };
        self.skill_ref = format!("AS-{kind}-{}-{}", self.key, &self.instructions_sha256[..12]);
        self.skill_ref_en = format!(
            "AS-{kind}-{}-{}",
            self.key,
            &self.instructions_sha256_en[..12]
        );
        self.resources = progressive_resources(
            self.kind,
            &self.key,
            &self.label,
            &self.description,
            &self.category_label,
            false,
        );
        self.resources_en = progressive_resources(
            self.kind,
            &self.key,
            &self.label_en,
            &self.description_en,
            &self.category_label_en,
            true,
        );
        self
    }
}

impl From<CatalogProfession> for AgentSkillDefinition {
    fn from(value: CatalogProfession) -> Self {
        Self {
            kind: AgentSkillKind::Profession,
            key: value.key,
            label: value.label,
            label_en: value.label_en,
            description: value.description,
            description_en: value.description_en,
            category_key: value.category_key,
            category_label: value.category_label,
            category_label_en: value.category_label_en,
            markdown: value.skill_markdown,
            markdown_en: value.skill_markdown_en,
            customized: false,
            skill_ref: String::new(),
            skill_ref_en: String::new(),
            instructions_sha256: String::new(),
            instructions_sha256_en: String::new(),
            resources: Vec::new(),
            resources_en: Vec::new(),
        }
        .with_progressive_projection()
    }
}

impl From<CatalogProjectType> for AgentSkillDefinition {
    fn from(value: CatalogProjectType) -> Self {
        Self {
            kind: AgentSkillKind::ProjectType,
            key: value.key,
            label: value.label,
            label_en: value.label_en,
            description: value.description,
            description_en: value.description_en,
            category_key: value.category_key,
            category_label: value.category_label,
            category_label_en: value.category_label_en,
            markdown: value.rule_markdown,
            markdown_en: value.rule_markdown_en,
            customized: false,
            skill_ref: String::new(),
            skill_ref_en: String::new(),
            instructions_sha256: String::new(),
            instructions_sha256_en: String::new(),
            resources: Vec::new(),
            resources_en: Vec::new(),
        }
        .with_progressive_projection()
    }
}

fn progressive_resources(
    kind: AgentSkillKind,
    key: &str,
    label: &str,
    description: &str,
    category: &str,
    english: bool,
) -> Vec<AgentSkillResource> {
    let values = if english {
        english_resources(label, description, category)
    } else {
        chinese_resources(label, description, category)
    };
    let mut resources: Vec<_> = values
        .into_iter()
        .map(|(relative_path, title, summary, markdown)| {
            let markdown = markdown.trim().to_owned();
            AgentSkillResource {
                relative_path: relative_path.to_owned(),
                title: title.to_owned(),
                summary: summary.to_owned(),
                size_bytes: markdown.len(),
                markdown,
            }
        })
        .collect();
    resources.push(worked_example_resource(kind, key, label, english));
    resources
}

fn worked_example_resource(
    kind: AgentSkillKind,
    key: &str,
    label: &str,
    english: bool,
) -> AgentSkillResource {
    static EXAMPLES: OnceLock<Result<ExampleCatalog, String>> = OnceLock::new();
    let profile = EXAMPLES
        .get_or_init(|| serde_json::from_str(SKILL_EXAMPLES).map_err(|_| storage_error()))
        .as_ref()
        .ok()
        .and_then(|catalog| match kind {
            AgentSkillKind::Profession => catalog.professions.get(key),
            AgentSkillKind::ProjectType => catalog.project_types.get(key),
        });
    let (title, summary, markdown) = match (profile, english) {
        (Some(profile), true) => (
            "Worked example and critical counterexample",
            "An individually authored end-to-end example with expected evidence, a critical anti-pattern, and its correction.",
            format!(
                r#"# {label}: worked example and critical counterexample

## Situation

{}

## Good example

{}

## Expected evidence

{}

## Critical counterexample — do not copy

{}

## Why it fails and how to correct it

{}

## Usage boundary

This example demonstrates a method, not project facts or extra authority. Replace every concrete value with current evidence, preserve the Human's scope, and never report the example outcome as work performed in the current project."#,
                profile.situation_en,
                profile.good_example_en,
                profile.evidence_en,
                profile.counterexample_en,
                profile.correction_en,
            ),
        ),
        (Some(profile), false) => (
            "完整示例与关键反例",
            "逐项编写的端到端示例，包含应有证据、关键反例及修正方式。",
            format!(
                r#"# {label}：完整示例与关键反例

## 场景

{}

## 正确示例

{}

## 应有证据

{}

## 关键反例（不要照做）

{}

## 为什么错，以及如何修正

{}

## 使用边界

示例只演示方法，不代表当前项目事实，也不扩大工具或执行权限。执行时必须把具体值替换为当前证据，保持 Human 给定范围，不得把示例结果当成当前项目已经完成的工作。"#,
                profile.situation_zh,
                profile.good_example_zh,
                profile.evidence_zh,
                profile.counterexample_zh,
                profile.correction_zh,
            ),
        ),
        (None, true) => (
            "Missing worked example",
            "Catalog validation must reject this Skill.",
            format!(
                "# {label}: missing worked example\n\nThis catalog entry is incomplete and must not be released."
            ),
        ),
        (None, false) => (
            "缺少专属示例",
            "目录校验必须拒绝此 Skill。",
            format!("# {label}：缺少专属示例\n\n此目录条目不完整，不得发布。"),
        ),
    };
    let markdown = markdown.trim().to_owned();
    AgentSkillResource {
        relative_path: "references/worked-examples-and-counterexamples.md".into(),
        title: title.into(),
        summary: summary.into(),
        size_bytes: markdown.len(),
        markdown,
    }
}

fn chinese_resources(
    label: &str,
    description: &str,
    category: &str,
) -> Vec<(&'static str, &'static str, &'static str, String)> {
    vec![
        (
            "references/workflow.md",
            "工作流与决策路径",
            "从接收目标、澄清事实、拆分工作到验证与交接的详细步骤。",
            format!(
                r#"# {label}：工作流与决策路径

## 适用目标

本 Skill 绑定为“{label}”（分类：{category}）。核心职责是：{description}。使用本页时先确认当前请求确实落在这一职责内；若涉及另一职业的独立判断，只陈述接口、依赖和所需证据，不假装拥有另一身份或权限。

## 开始前

1. 从 Human 消息、当前 Todo、团队目标和已读取资产中提取目标、范围、约束、截止条件与明确排除项。
2. 将事实、假设、未知项分开。事实必须能追溯到消息、代码、数据、工具输出或已确认决策；假设必须标注验证办法。
3. 确认执行权限和工具能力。Skill 只提供方法，不授予写入、发布、创建任务、访问私密数据或代表 Human 决策的权限。
4. 为交付物写出可观察的验收条件，包含正常路径、关键边界、失败与恢复路径。

## 执行循环

- 建立最小可验证计划，每一步说明输入、动作、产物、验证与失败处理。
- 优先读取真实现状，再做设计或修改；避免用模板覆盖已有约定。
- 重要发现区分“已做”“已观察”“待确认”和“下一步”。
- 发现范围扩大、不可逆影响、权限不足、证据冲突或外部依赖时暂停相关动作并升级。
- 完成前回到原始目标逐条核验，不能用“代码已写”或“文档已生成”代替结果已经生效。

## 交接

交接至少包含结论、变更范围、证据位置、验证方法、残余风险、未解决问题和下一责任人。项目类型 Skill 同时存在时，还要读取它的工作流资料；两者冲突时以真实权限、Human 指令和更严格的验收门槛为准。"#
            ),
        ),
        (
            "references/deliverables-and-evidence.md",
            "交付物与证据",
            "定义什么可以交付、如何引用证据以及何时才允许声称完成。",
            format!(
                r#"# {label}：交付物与证据

围绕“{description}”选择最小但完整的交付集合。每项交付物必须写清受众、用途、存放位置、版本或时间点、所有者和验收方式；没有验证上下文的摘要不算交付物。

## 证据等级

1. **直接证据**：测试输出、工具返回、可复现命令、查询结果、截图、运行日志、评审记录或 Human 明确确认。
2. **可追溯证据**：代码或文档路径、版本、输入数据范围、环境与时间，足以让下一位执行者复核。
3. **推断**：基于事实但尚未通过真实环境或负责人确认的分析。必须标为推断，不能混入完成结论。

## 完成声明检查

- 原始目标与验收条件逐条对应到当前证据。
- 关键失败路径、边界条件、权限和数据影响已经验证。
- 明确未测试范围、环境差异、样本偏差和仍需 Human 决策的事项。
- 若部署、发布、审批、迁移或运营接管仍未完成，只能写“已准备”或“待上线”。
- 摘要按“结论 → 已完成范围 → 关键证据 → 风险/限制 → 下一步与负责人”组织。"#
            ),
        ),
        (
            "references/quality-gates-and-risks.md",
            "质量门禁与风险",
            "给出适用于当前 Skill 的验证门禁、常见失败模式和风险处理。",
            format!(
                r#"# {label}：质量门禁与风险

## 必过门禁

- **正确性**：产物满足已确认目标，并有正向、边界和失败场景证据。
- **一致性**：术语、接口、数据、状态和决策与项目现状一致，不制造第二套真相。
- **可复核性**：另一位成员能依据记录重现关键判断与验证结果。
- **安全与权限**：敏感信息、写入范围、外部通信和不可逆动作均在授权边界内。
- **可运营性**：明确监控、回滚或恢复、维护责任和遗留风险。

对“{description}”尤其要防止：只复述需求而未核实现状；为了显得完整而虚构事实；只验证顺利路径；遗漏上下游；将建议写成已执行；以篇幅替代验收；发现证据冲突后仍继续推进。

低风险事项可在当前范围修正并记录；影响多个成员、接口、数据或时间的中风险事项需要同步负责人；可能造成数据丢失、安全合规问题、生产中断、对外承诺或不可逆成本的高风险事项必须停止并请求 Human 决策。"#
            ),
        ),
        (
            "references/collaboration-and-escalation.md",
            "协作、边界与升级",
            "明确与项目经理、其他职业 Agent 和 Human 的协作接口。",
            format!(
                r#"# {label}：协作、边界与升级

当前身份负责：{description}。职业 Skill 不会扩大客户端提供的工具与数据权限，也不会赋予项目经理身份。只有明确绑定的项目经理维护团队任务结构和项目看板；普通成员通过消息与任务进度提供事实、建议、风险和交付证据。

## 协作请求应包含

- 背景、问题、期望输出、验收条件、截止或优先级、输入位置和已知约束。
- 已完成的调查、证据与失败尝试，避免对方重复劳动。
- 需要对方做出的明确决定，以及没有回复时受影响的范围。

目标或验收矛盾、需要越权或不可逆操作、关键输入缺失且假设会改变方案、安全隐私合规或生产稳定性风险、跨团队依赖没有负责人、范围成本显著变化、真实结果与看板不一致时必须升级。日常同步按“结论、证据、影响、下一步”组织；需要 Human 决策时列出选项、取舍、推荐项和最晚决策点。"#
            ),
        ),
    ]
}

fn english_resources(
    label: &str,
    description: &str,
    category: &str,
) -> Vec<(&'static str, &'static str, &'static str, String)> {
    vec![
        (
            "references/workflow.md",
            "Workflow and decision path",
            "Detailed path from intake and fact finding through execution, validation, and handoff.",
            format!(
                r#"# {label}: workflow and decision path

This Skill is bound as **{label}** in **{category}**. Its purpose is: {description}. Confirm that the request belongs to this responsibility. For another profession's independent judgment, describe the interface, dependency, and evidence without claiming that identity or authority.

## Before work

1. Extract objective, scope, constraints, deadline, and exclusions from Human messages, the current Todo, team goals, and assets already read.
2. Separate facts, assumptions, and unknowns. Facts must trace to current evidence and every assumption needs a validation path.
3. Confirm authority and available tools. A Skill supplies method, never permission to write, publish, create tasks, access private data, or decide for a Human.
4. Define observable acceptance for normal, boundary, failure, and recovery paths.

Use the smallest verifiable plan. Read the real state before editing, report material discoveries accurately, and stop on scope growth, irreversible impact, missing authority, conflicting evidence, or unmanaged dependencies. Before completion, map every objective to current evidence.

Handoff includes conclusion, changed scope, evidence locations, validation steps, residual risk, open questions, and next owner. Real authority, Human direction, and the stricter acceptance gate always win."#
            ),
        ),
        (
            "references/deliverables-and-evidence.md",
            "Deliverables and evidence",
            "Defines acceptable outputs, evidence traceability, and completion claims.",
            format!(
                r#"# {label}: deliverables and evidence

Build the smallest complete deliverable set for “{description}.” For every output state its audience, purpose, location, version or time boundary, owner, and acceptance method. An unverified summary is not a deliverable.

## Evidence ladder

1. **Direct evidence:** tests, tool results, reproducible commands, queries, screenshots, logs, reviews, or explicit Human confirmation.
2. **Traceable evidence:** paths, versions, input ranges, environment, and time sufficient for another person to verify.
3. **Inference:** reasoned interpretation not yet validated in the real environment; label it and keep it out of completion claims.

Map every acceptance criterion to evidence; cover boundaries, failure paths, permissions, and data impact; disclose untested scope and environment differences. If deployment, approval, migration, publication, or operational takeover is pending, say “prepared” or “awaiting approval,” not “complete.”"#
            ),
        ),
        (
            "references/quality-gates-and-risks.md",
            "Quality gates and risks",
            "Verification gates, common failure modes, and risk handling for this Skill.",
            format!(
                r#"# {label}: quality gates and risks

Required gates are correctness against confirmed objectives; consistency with project terminology, interfaces, data, states, and decisions; reproducibility by another contributor; authorization for sensitive data, external communication, and irreversible actions; and operability through monitoring, recovery, ownership, and residual risk.

For “{description},” prevent restating the request without inspecting reality, inventing facts, testing only the happy path, missing downstream consumers, describing a recommendation as executed, substituting length for acceptance, or continuing after evidence conflicts.

Treat reversible local issues as low risk. Multi-owner, interface, data, or schedule impact is medium risk and needs coordination. Potential data loss, security or compliance exposure, production interruption, external commitments, or irreversible cost is high risk: stop and request Human direction."#
            ),
        ),
        (
            "references/collaboration-and-escalation.md",
            "Collaboration and escalation",
            "Interfaces with the Project Manager, other professions, and the Human.",
            format!(
                r#"# {label}: collaboration and escalation

This identity owns: {description}. The Skill does not expand tool or data authority and does not confer Project Manager status. Only the explicitly bound Project Manager maintains team task structure and the project dashboard.

A useful collaboration request includes context and problem, expected output and acceptance, priority or deadline, input locations, constraints, prior investigation, failed attempts, the exact decision needed, and the consequence of delay.

Escalate contradictory goals, unauthorized or irreversible actions, assumption-sensitive missing inputs, security/privacy/compliance/production risk, ownerless cross-team dependencies, material scope or cost change, and disagreement between evidence and dashboard state. Keep updates concise: conclusion, evidence, impact, and next step. For a Human decision, present options, trade-offs, a recommendation, and the latest decision point."#
            ),
        ),
    ]
}

fn sha256(value: &str) -> String {
    Sha256::digest(value.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn validate_field(raw: &str, field: &str, maximum_length: usize) -> Result<String, String> {
    let value = raw.trim();
    if value.is_empty() || value.contains('\0') || value.graphemes(true).count() > maximum_length {
        return Err(format!("无效的 Skill 字段：{field}"));
    }
    Ok(value.to_owned())
}

fn load_payload(file: &Path) -> Result<OverridePayload, String> {
    let Some(metadata) = symlink_safe_metadata(file)? else {
        return Ok(OverridePayload {
            schema_version: 2,
            accounts: HashMap::new(),
        });
    };
    if !metadata.is_file() || metadata.len() > MAXIMUM_PAYLOAD_BYTES {
        return Err("Skill 配置不是普通文件或超过本地存储上限。".to_owned());
    }
    let bytes = fs::read(file).map_err(|_| storage_error())?;
    let mut payload: OverridePayload =
        serde_json::from_slice(&bytes).map_err(|_| storage_error())?;
    if !(1..=2).contains(&payload.schema_version) {
        return Err("Skill 配置版本不受支持。".to_owned());
    }
    payload.schema_version = 2;
    Ok(payload)
}

fn persist_payload(file: &Path, payload: &OverridePayload) -> Result<(), String> {
    let parent = file.parent().ok_or_else(storage_error)?;
    fs::create_dir_all(parent).map_err(|_| storage_error())?;
    reject_symlink(parent)?;
    if symlink_safe_metadata(file)?.is_some() {
        reject_symlink(file)?;
    }
    let bytes = serde_json::to_vec_pretty(payload).map_err(|_| storage_error())?;
    if bytes.len() as u64 > MAXIMUM_PAYLOAD_BYTES {
        return Err("Skill 配置超过本地存储上限。".to_owned());
    }
    let mut temporary = tempfile::NamedTempFile::new_in(parent).map_err(|_| storage_error())?;
    temporary.write_all(&bytes).map_err(|_| storage_error())?;
    temporary
        .as_file()
        .sync_all()
        .map_err(|_| storage_error())?;
    replace_file(temporary.into_temp_path().as_ref(), file)?;
    Ok(())
}

#[cfg(not(target_os = "windows"))]
fn replace_file(source: &Path, destination: &Path) -> Result<(), String> {
    fs::rename(source, destination).map_err(|_| storage_error())
}

#[cfg(target_os = "windows")]
fn replace_file(source: &Path, destination: &Path) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
    };
    let source: Vec<u16> = source.as_os_str().encode_wide().chain(Some(0)).collect();
    let destination: Vec<u16> = destination
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect();
    let result = unsafe {
        MoveFileExW(
            source.as_ptr(),
            destination.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if result == 0 {
        Err(storage_error())
    } else {
        Ok(())
    }
}

fn symlink_safe_metadata(path: &Path) -> Result<Option<fs::Metadata>, String> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => Err(storage_error()),
        Ok(metadata) => Ok(Some(metadata)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(_) => Err(storage_error()),
    }
}

fn reject_symlink(path: &Path) -> Result<(), String> {
    if fs::symlink_metadata(path)
        .map_err(|_| storage_error())?
        .file_type()
        .is_symlink()
    {
        Err(storage_error())
    } else {
        Ok(())
    }
}

fn storage_error() -> String {
    "无法保存本地 Skill。".to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overrides_are_account_scoped_and_persisted() {
        let directory = tempfile::tempdir().unwrap();
        let file = directory.path().join("AgentSkillOverrides.json");
        let store = AgentSkillStore::new(file.clone()).unwrap();
        let saved = store.save("alice", mutation("自定义职业")).unwrap();
        assert_eq!(saved.label, "自定义职业");
        assert!(saved.customized);
        assert_eq!(store.catalog("bob").unwrap()[0].label, "项目经理");

        let reopened = AgentSkillStore::new(file).unwrap();
        let reopened_catalog = reopened.catalog("alice").unwrap();
        assert_eq!(reopened_catalog.len(), 60);
        assert!(reopened_catalog.iter().all(|item| {
            item.resources.len() == 5
                && item.resources_en.len() == 5
                && item
                    .resources
                    .last()
                    .is_some_and(|resource| resource.title != "缺少专属示例")
                && item
                    .resources_en
                    .last()
                    .is_some_and(|resource| resource.title != "Missing worked example")
        }));
        let reopened_item = &reopened_catalog[0];
        assert_eq!(reopened_item.label, "自定义职业");
        assert_eq!(reopened_item.resources.len(), 5);
        assert_eq!(reopened_item.resources_en.len(), 5);
        assert_eq!(
            reopened_item.resources.last().unwrap().title,
            "完整示例与关键反例"
        );
        assert!(reopened_item
            .skill_ref
            .ends_with(&reopened_item.instructions_sha256[..12]));
        assert_eq!(reopened.catalog("bob").unwrap()[0].label, "项目经理");

        let persisted = fs::read_to_string(reopened.file.as_path()).unwrap();
        assert!(persisted.contains("\"labelEN\""));
        assert!(persisted.contains("\"skillMarkdownEN\""));
        assert!(!persisted.contains("\"labelEn\""));
    }

    #[test]
    fn reset_restores_the_bundled_definition() {
        let directory = tempfile::tempdir().unwrap();
        let store = AgentSkillStore::new(directory.path().join("overrides.json")).unwrap();
        store.save("alice", mutation("自定义职业")).unwrap();
        let reset = store
            .reset("alice", AgentSkillKind::Profession, "project_manager")
            .unwrap();
        assert_eq!(reset.label, "项目经理");
        assert!(!reset.customized);
    }

    #[test]
    fn rejects_unknown_keys_and_empty_bilingual_fields() {
        let directory = tempfile::tempdir().unwrap();
        let store = AgentSkillStore::new(directory.path().join("overrides.json")).unwrap();
        let mut unknown = mutation("自定义职业");
        unknown.key = "unknown".into();
        assert!(store.save("alice", unknown).is_err());
        let mut empty = mutation("自定义职业");
        empty.label_en = "  ".into();
        assert!(store.save("alice", empty).is_err());
    }

    fn mutation(label: &str) -> AgentSkillMutationInput {
        AgentSkillMutationInput {
            kind: AgentSkillKind::Profession,
            key: "project_manager".into(),
            label: label.into(),
            label_en: "Custom role".into(),
            description: "自定义说明".into(),
            description_en: "Custom description".into(),
            markdown: "# 自定义正文".into(),
            markdown_en: "# Custom instructions".into(),
        }
    }
}
