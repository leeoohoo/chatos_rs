import { useEffect, useMemo, useState } from "react";
import { useAgentSkillManagementStore } from "../application/agentSkillManagementStore";
import type {
  AgentSkillDefinition,
  AgentSkillKind,
  AgentSkillMutationInput,
} from "../domain/agentSkillManagement";
import type { AgentManagementPort } from "../ports/AgentManagementPort";

type Language = "zh" | "en";
type Disclosure = "overview" | "instructions";
type ContentMode = "preview" | "edit" | "split";

export function AgentSkillManagementSheet({
  ownerUserId,
  port,
  onClose,
}: {
  ownerUserId: string;
  port: AgentManagementPort;
  onClose(): void;
}) {
  const items = useAgentSkillManagementStore((state) => state.items);
  const isLoading = useAgentSkillManagementStore((state) => state.isLoading);
  const isSaving = useAgentSkillManagementStore((state) => state.isSaving);
  const error = useAgentSkillManagementStore((state) => state.error);
  const activate = useAgentSkillManagementStore((state) => state.activate);
  const save = useAgentSkillManagementStore((state) => state.save);
  const resetOverride = useAgentSkillManagementStore((state) => state.resetOverride);
  const clearError = useAgentSkillManagementStore((state) => state.clearError);
  const reset = useAgentSkillManagementStore((state) => state.reset);
  const [kind, setKind] = useState<AgentSkillKind>("profession");
  const [language, setLanguage] = useState<Language>("zh");
  const [query, setQuery] = useState("");
  const [selectedKey, setSelectedKey] = useState<string | null>(null);
  const [disclosure, setDisclosure] = useState<Disclosure>("overview");
  const [contentMode, setContentMode] = useState<ContentMode>("preview");
  const [draft, setDraft] = useState<AgentSkillMutationInput | null>(null);

  useEffect(() => {
    void activate(port, ownerUserId);
    return reset;
  }, [activate, ownerUserId, port, reset]);

  const visibleItems = useMemo(() => {
    const normalized = query.trim().toLocaleLowerCase();
    return items.filter((item) => item.kind === kind && (!normalized || [
      item.key,
      item.label,
      item.labelEn,
      item.categoryLabel,
      item.categoryLabelEn,
      item.description,
      item.descriptionEn,
    ].some((value) => value.toLocaleLowerCase().includes(normalized))));
  }, [items, kind, query]);

  useEffect(() => {
    const selected = visibleItems.find((item) => item.key === selectedKey) ?? visibleItems[0] ?? null;
    if (!selected) {
      if (selectedKey != null) setSelectedKey(null);
      if (draft != null) setDraft(null);
      return;
    }
    if (selectedKey !== selected.key || draft?.kind !== selected.kind || draft.key !== selected.key) {
      setSelectedKey(selected.key);
      setDraft(draftFrom(selected));
    }
  }, [draft, selectedKey, visibleItems]);

  const selected = items.find((item) => item.kind === kind && item.key === selectedKey) ?? null;
  const valid = draft != null && [
    draft.label,
    draft.labelEn,
    draft.description,
    draft.descriptionEn,
    draft.markdown,
    draft.markdownEn,
  ].every((value) => value.trim().length > 0);

  const selectItem = (item: AgentSkillDefinition) => {
    clearError();
    setSelectedKey(item.key);
    setDraft(draftFrom(item));
  };

  const saveDraft = async () => {
    if (!draft || !valid) return;
    const saved = await save(port, trimmedDraft(draft));
    if (saved) setDraft(draftFrom(saved));
  };

  const restoreBundled = async () => {
    if (!selected) return;
    const restored = await resetOverride(port, selected.kind, selected.key);
    if (restored) setDraft(draftFrom(restored));
  };

  return (
    <div
      className="agent-editor-backdrop agent-skill-backdrop"
      onMouseDown={(event) => { if (event.target === event.currentTarget && !isSaving) onClose(); }}
      role="presentation"
    >
      <section aria-label="Skill 管理" aria-modal="true" className="agent-skill-sheet" role="dialog">
        <header className="agent-skill-sheet__header">
          <div>
            <h2>Skill 管理</h2>
            <p>管理 Agent 职业与项目类型的完整运行规则。</p>
            <small>新建 Agent Run 会使用当前语言版本；稳定 Key、分类和真实工具权限不会被修改。</small>
          </div>
          <Segmented<AgentSkillKind>
            label="类型"
            options={[["profession", "职业 Skill"], ["projectType", "项目类型 Skill"]]}
            value={kind}
            onChange={(value) => { setKind(value); setSelectedKey(null); clearError(); }}
          />
          <Segmented<Language>
            label="Skill 语言"
            options={[["zh", "中文"], ["en", "English"]]}
            value={language}
            onChange={setLanguage}
          />
          <button aria-label="关闭 Skill 管理" disabled={isSaving} onClick={onClose} type="button">×</button>
        </header>

        <div className="agent-skill-sheet__workspace">
          <aside className="agent-skill-catalog">
            <div><input aria-label="搜索 Skill" onChange={(event) => setQuery(event.target.value)} placeholder="搜索 Skill" value={query} /></div>
            {isLoading ? (
              <p className="agent-skill-catalog__state"><span className="mini-spinner" />正在读取 Skill…</p>
            ) : (
              <div className="agent-skill-catalog__list" role="listbox">
                {visibleItems.map((item) => (
                  <button
                    aria-selected={selectedKey === item.key}
                    className={selectedKey === item.key ? "is-selected" : ""}
                    key={item.key}
                    onClick={() => selectItem(item)}
                    role="option"
                    type="button"
                  >
                    <span>{language === "en" ? item.labelEn : item.label}{item.customized && <em>已修改</em>}</span>
                    <small>{language === "en" ? item.categoryLabelEn : item.categoryLabel}</small>
                    <code>{item.key}</code>
                  </button>
                ))}
              </div>
            )}
          </aside>

          <main className="agent-skill-editor">
            {error && <div className="agent-editor__error" role="alert">{error}</div>}
            {!draft || !selected ? (
              <div className="agent-skill-editor__empty">选择一个 Skill</div>
            ) : (
              <>
                <div className="agent-skill-editor__identity">
                  <ReadOnly label="Key" value={selected.key} />
                  <ReadOnly label="分类" value={language === "en" ? selected.categoryLabelEn : selected.categoryLabel} />
                </div>
                <label>名称<input maxLength={120} onChange={(event) => updateLocalizedDraft(draft, setDraft, language, "label", event.target.value)} value={language === "en" ? draft.labelEn : draft.label} /></label>
                <label>说明<textarea maxLength={4_000} onChange={(event) => updateLocalizedDraft(draft, setDraft, language, "description", event.target.value)} rows={3} value={language === "en" ? draft.descriptionEn : draft.description} /></label>
                <Segmented<Disclosure>
                  label="渐进披露层级"
                  options={[["overview", "概要"], ["instructions", "完整说明"]]}
                  value={disclosure}
                  onChange={setDisclosure}
                />
                {disclosure === "overview" ? (
                  <SkillOverview item={selected} language={language} />
                ) : (
                  <section className="agent-skill-content">
                    <header>
                      <span>{kind === "profession" ? "Skill 正文" : "项目规则正文"}</span>
                      <Segmented<ContentMode>
                        label="Markdown 显示"
                        options={[["preview", "预览"], ["edit", "编辑"], ["split", "分栏"]]}
                        value={contentMode}
                        onChange={setContentMode}
                      />
                    </header>
                    <div className={`agent-skill-content__${contentMode}`}>
                      {contentMode !== "preview" && (
                        <textarea
                          aria-label={kind === "profession" ? "Skill 正文" : "项目规则正文"}
                          maxLength={500_000}
                          onChange={(event) => updateLocalizedDraft(draft, setDraft, language, "markdown", event.target.value)}
                          value={language === "en" ? draft.markdownEn : draft.markdown}
                        />
                      )}
                      {contentMode !== "edit" && <MarkdownPreview markdown={language === "en" ? draft.markdownEn : draft.markdown} />}
                    </div>
                  </section>
                )}
              </>
            )}
          </main>
        </div>

        <footer>
          <button className="destructive-button" disabled={!selected?.customized || isSaving} onClick={() => void restoreBundled()} type="button">恢复内置版本</button>
          <span />
          <button disabled={isSaving} onClick={onClose} type="button">完成</button>
          <button className="primary-button" disabled={!valid || isSaving} onClick={() => void saveDraft()} type="button">{isSaving ? "正在保存…" : "保存"}</button>
        </footer>
      </section>
    </div>
  );
}

function SkillOverview({ item, language }: { item: AgentSkillDefinition; language: Language }) {
  const label = language === "en" ? item.labelEn : item.label;
  const description = language === "en" ? item.descriptionEn : item.description;
  return (
    <section className="agent-skill-overview">
      <h3>渐进披露结构</h3>
      <div>
        <DisclosureCard detail="Run 启动时只注入名称、用途与绑定引用，避免完整正文长期占用上下文。" step="1" title="Router 概要" />
        <DisclosureCard detail="Agent 只能按需激活当前身份绑定的 Skill，不能切换职业或项目类型。" step="2" title="按需激活" />
        <DisclosureCard detail="完整正文会固定到新 Run 的快照中，但不会扩大真实工具权限。" step="3" title="运行快照" />
      </div>
      <article>
        <small>运行时 Router 预览</small>
        <p><code>{item.kind}:{item.key}</code> · <strong>{label}</strong>：{description}</p>
      </article>
      <p>自定义内容仅影响之后创建的 Run；已经启动的 Run 保持原有快照，权限仍由任务和原生能力边界决定。</p>
    </section>
  );
}

function DisclosureCard({ step, title, detail }: { step: string; title: string; detail: string }) {
  return <article><em>{step}</em><strong>{title}</strong><p>{detail}</p></article>;
}

function MarkdownPreview({ markdown }: { markdown: string }) {
  return (
    <div aria-label="Markdown 预览" className="agent-skill-markdown">
      {markdown.split(/\n{2,}/u).map((block, index) => {
        const value = block.trim();
        if (!value) return null;
        const heading = /^(#{1,4})\s+(.+)$/u.exec(value);
        if (heading) return <h3 key={index}>{heading[2]}</h3>;
        const lines = value.split("\n");
        if (lines.every((line) => /^[-*]\s+/u.test(line))) {
          return <ul key={index}>{lines.map((line, lineIndex) => <li key={lineIndex}>{line.replace(/^[-*]\s+/u, "")}</li>)}</ul>;
        }
        return <p key={index}>{lines.map((line, lineIndex) => <span key={lineIndex}>{line}{lineIndex < lines.length - 1 && <br />}</span>)}</p>;
      })}
    </div>
  );
}

function Segmented<T extends string>({
  label,
  options,
  value,
  onChange,
}: {
  label: string;
  options: ReadonlyArray<readonly [T, string]>;
  value: T;
  onChange(value: T): void;
}) {
  return (
    <div aria-label={label} className="agent-skill-segmented" role="group">
      {options.map(([option, title]) => (
        <button aria-pressed={value === option} key={option} onClick={() => onChange(option)} type="button">{title}</button>
      ))}
    </div>
  );
}

function ReadOnly({ label, value }: { label: string; value: string }) {
  return <div><small>{label}</small><code>{value}</code></div>;
}

function draftFrom(item: AgentSkillDefinition): AgentSkillMutationInput {
  return {
    kind: item.kind,
    key: item.key,
    label: item.label,
    labelEn: item.labelEn,
    description: item.description,
    descriptionEn: item.descriptionEn,
    markdown: item.markdown,
    markdownEn: item.markdownEn,
  };
}

function trimmedDraft(draft: AgentSkillMutationInput): AgentSkillMutationInput {
  return Object.fromEntries(Object.entries(draft).map(([key, value]) => [key, typeof value === "string" ? value.trim() : value])) as unknown as AgentSkillMutationInput;
}

function updateLocalizedDraft(
  draft: AgentSkillMutationInput,
  setDraft: (draft: AgentSkillMutationInput) => void,
  language: Language,
  field: "label" | "description" | "markdown",
  value: string,
) {
  const key = language === "en" ? `${field}En` as const : field;
  setDraft({ ...draft, [key]: value });
}
