import { useEffect, useRef, useState } from "react";
import { useAskUserStore } from "../application/askUserStore";
import {
  askUserSubmission,
  clearAskUserSecrets,
  createAskUserForm,
  isAskUserFormValid,
  toggleAskUserSelection,
  type AskUserPrompt,
} from "../domain/askUser";
import type { AskUserPort } from "../ports/AskUserPort";

export function AskUserPromptCard({
  prompt,
  port,
  onResolved,
}: {
  prompt: AskUserPrompt;
  port: AskUserPort;
  onResolved(): Promise<void>;
}) {
  const [form, setForm] = useState(() => createAskUserForm(prompt));
  const formRef = useRef(form);
  const secretKeysRef = useRef<string[]>([]);
  formRef.current = form;
  secretKeysRef.current = prompt.fields.filter((field) => field.isSecret).map((field) => field.key);
  const actionId = useAskUserStore((state) => state.actionIds[prompt.id]);
  const error = useAskUserStore((state) => state.promptErrors[prompt.id]);
  const submit = useAskUserStore((state) => state.submit);
  const cancel = useAskUserStore((state) => state.cancel);
  const clearError = useAskUserStore((state) => state.clearPromptError);
  const isSubmitting = actionId != null;

  useEffect(() => {
    setForm(createAskUserForm(prompt));
  }, [prompt.id]);

  useEffect(() => {
    if (prompt.status !== "pending") setForm((current) => clearAskUserSecrets(current, prompt));
  }, [prompt, prompt.status]);

  useEffect(() => () => {
    for (const key of secretKeysRef.current) formRef.current.values[key] = "";
  }, [prompt.id]);

  if (prompt.status !== "pending") return <ResolvedPromptCard prompt={prompt} />;
  const valid = isAskUserFormValid(prompt, form);

  const resolve = async (operation: Promise<boolean>) => {
    const accepted = await operation;
    if (accepted) {
      setForm((current) => clearAskUserSecrets(current, prompt));
      await onResolved();
    }
  };

  return (
    <article className="ask-user-card" aria-label={prompt.title || "需要你的回复"}>
      <header className="ask-user-card__header">
        <span className="ask-user-card__icon" aria-hidden="true">?</span>
        <div>
          <strong>{prompt.title.trim() || "需要你的回复"}</strong>
          <small>任务正在等待你的回复</small>
        </div>
        <span className="ask-user-card__status">待处理</span>
      </header>
      <p className="ask-user-card__message">{prompt.message}</p>
      {prompt.fields.length > 0 && (
        <div className="ask-user-fields">
          {prompt.fields.map((field) => (
            <label className="ask-user-field" key={field.key}>
              <span>{field.label}{field.isRequired && <b aria-label="必填"> *</b>}</span>
              {field.description && <small>{field.description}</small>}
              {field.isMultiline ? (
                <textarea
                  disabled={isSubmitting}
                  onChange={(event) => setForm((current) => ({
                    ...current,
                    values: { ...current.values, [field.key]: event.target.value },
                  }))}
                  placeholder={field.placeholder ?? ""}
                  rows={3}
                  value={form.values[field.key] ?? ""}
                />
              ) : (
                <input
                  autoComplete="off"
                  disabled={isSubmitting}
                  onChange={(event) => setForm((current) => ({
                    ...current,
                    values: { ...current.values, [field.key]: event.target.value },
                  }))}
                  placeholder={field.placeholder ?? ""}
                  type={field.isSecret ? "password" : "text"}
                  value={form.values[field.key] ?? ""}
                />
              )}
              {field.isSecret && <small className="ask-user-secret-note">安全字段不能通过本机 IPC 提交</small>}
            </label>
          ))}
        </div>
      )}
      {prompt.choice && (
        <fieldset className="ask-user-choice">
          <legend>
            {prompt.choice.allowsMultiple ? "请选择（可多选）" : "请选择"}
            {prompt.choice.minimumSelectionCount > 0 && (
              <small>{selectionHint(prompt.choice.minimumSelectionCount, prompt.choice.maximumSelectionCount)}</small>
            )}
          </legend>
          <div className="ask-user-options">
            {prompt.choice.options.map((option) => {
              const selected = form.selection.has(option.value);
              const maximumReached = prompt.choice!.allowsMultiple
                && !selected
                && form.selection.size >= prompt.choice!.maximumSelectionCount;
              return (
                <button
                  aria-pressed={selected}
                  className={`ask-user-option${selected ? " ask-user-option--selected" : ""}`}
                  disabled={isSubmitting || maximumReached}
                  key={option.value}
                  onClick={() => setForm((current) => toggleAskUserSelection(current, prompt.choice!, option.value))}
                  type="button"
                >
                  <span aria-hidden="true">{prompt.choice!.allowsMultiple ? (selected ? "✓" : "□") : (selected ? "●" : "○")}</span>
                  <span><strong>{option.label}</strong>{option.description && <small>{option.description}</small>}</span>
                </button>
              );
            })}
          </div>
        </fieldset>
      )}
      {error && (
        <div className="ask-user-error" role="alert">
          <span>{error}</span>
          <button onClick={() => clearError(prompt.id)} type="button" aria-label="关闭错误">×</button>
        </div>
      )}
      <footer className="ask-user-actions">
        {prompt.allowsCancel && (
          <button
            className="ask-user-cancel"
            disabled={isSubmitting}
            onClick={() => void resolve(cancel(port, prompt))}
            type="button"
          >取消请求</button>
        )}
        <span />
        <button
          className="primary-button ask-user-submit"
          disabled={!valid || isSubmitting}
          onClick={() => void resolve(submit(port, prompt, askUserSubmission(prompt, form)))}
          type="button"
        >
          {isSubmitting ? <span className="button-spinner" aria-label="正在提交" /> : "确认提交"}
        </button>
      </footer>
    </article>
  );
}
function ResolvedPromptCard({ prompt }: { prompt: AskUserPrompt }) {
  const presentation = {
    ok: ["✓", "已提交，任务将继续执行"],
    canceled: ["×", "已取消"],
    timeout: ["◷", "已超时"],
    failed: ["!", "处理失败"],
    pending: ["?", "待处理"],
  }[prompt.status];
  return (
    <article className={`ask-user-resolved ask-user-resolved--${prompt.status}`}>
      <span aria-hidden="true">{presentation[0]}</span>
      <div><strong>{prompt.title.trim() || "交互请求"}</strong><small>{presentation[1]}</small></div>
    </article>
  );
}

function selectionHint(minimum: number, maximum: number) {
  return minimum === maximum ? `选择 ${minimum} 项` : `选择 ${minimum}–${maximum} 项`;
}
