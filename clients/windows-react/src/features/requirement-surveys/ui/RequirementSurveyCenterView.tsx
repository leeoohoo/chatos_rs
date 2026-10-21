import { useEffect, useMemo, useState } from "react";
import { useRequirementSurveyCenterStore } from "../application/requirementSurveyCenterStore";
import { useRequirementSurveyFollowUpStore } from "../application/requirementSurveyFollowUpStore";
import {
  isRequirementSurveySubmissionValid,
  toggleRequirementSurveyChoice,
  type RequirementSurvey,
  type RequirementSurveyAnswers,
} from "../domain/requirementSurvey";
import type { RequirementSurveyPort } from "../ports/RequirementSurveyPort";
import "./requirementSurvey.css";

interface RequirementSurveyCenterViewProps {
  hostReady: boolean;
  ownerUserId: string;
  projects: Array<{ id: string; title: string }>;
  port: RequirementSurveyPort;
}

export function RequirementSurveyCenterView({
  hostReady,
  ownerUserId,
  projects,
  port,
}: RequirementSurveyCenterViewProps) {
  const surveys = useRequirementSurveyCenterStore((state) => state.surveys);
  const isLoading = useRequirementSurveyCenterStore((state) => state.isLoading);
  const submittingSurveyIds = useRequirementSurveyCenterStore((state) => state.submittingSurveyIds);
  const error = useRequirementSurveyCenterStore((state) => state.error);
  const activate = useRequirementSurveyCenterStore((state) => state.activate);
  const refresh = useRequirementSurveyCenterStore((state) => state.refresh);
  const submit = useRequirementSurveyCenterStore((state) => state.submit);
  const reset = useRequirementSurveyCenterStore((state) => state.reset);
  const [selectedProjectId, setSelectedProjectId] = useState<string | null>(null);
  const [selectedSurveyId, setSelectedSurveyId] = useState<string | null>(null);
  const projectIdentity = useMemo(
    () => projects.map((project) => `${project.id}\u0000${project.title}`).join("\u0001"),
    [projects],
  );
  const selectedProject = projects.find((project) => project.id === selectedProjectId) ?? null;
  const selectedSurvey = surveys.find((survey) => survey.surveyId === selectedSurveyId) ?? null;

  useEffect(() => {
    setSelectedProjectId(null);
    setSelectedSurveyId(null);
    if (hostReady) void activate(port, ownerUserId, projects);
    else reset();
    return reset;
  }, [activate, hostReady, ownerUserId, port, projectIdentity, reset]);

  useEffect(() => {
    if (!hostReady || projects.length === 0) return undefined;
    const timer = window.setInterval(() => {
      if (document.visibilityState === "visible") void refresh(port);
    }, 30_000);
    return () => window.clearInterval(timer);
  }, [hostReady, port, projectIdentity, refresh]);

  useEffect(() => {
    if (selectedProjectId && !projects.some((project) => project.id === selectedProjectId)) {
      setSelectedProjectId(null);
      setSelectedSurveyId(null);
    }
  }, [projects, selectedProjectId]);

  useEffect(() => {
    if (selectedSurveyId && !surveys.some((survey) => survey.surveyId === selectedSurveyId)) {
      setSelectedSurveyId(null);
    }
  }, [selectedSurveyId, surveys]);

  if (!hostReady) {
    return <SurveyState icon="⌁" title="正在连接本机服务" detail="连接就绪后会自动读取需求调研。" />;
  }

  if (selectedSurvey && selectedProject) {
    return (
      <RequirementSurveyDetail
        isSubmitting={submittingSurveyIds.includes(selectedSurvey.surveyId)}
        ownerUserId={ownerUserId}
        port={port}
        survey={selectedSurvey}
        onBack={() => setSelectedSurveyId(null)}
        onSubmit={(answers) => submit(port, selectedSurvey, answers)}
      />
    );
  }

  if (selectedProject) {
    return (
      <ProjectSurveyList
        error={error}
        isLoading={isLoading}
        project={selectedProject}
        surveys={surveys.filter((survey) => survey.projectId === selectedProject.id)}
        onBack={() => setSelectedProjectId(null)}
        onOpen={setSelectedSurveyId}
        onRefresh={() => void refresh(port)}
      />
    );
  }

  const openCount = surveys.filter((survey) => survey.status === "open").length;
  const resolvedCount = surveys.filter((survey) => survey.status === "resolved").length;
  return (
    <section aria-label="需求调研中心" className="requirement-survey-center">
      <SurveyHero openCount={openCount} resolvedCount={resolvedCount} />
      {error && <SurveyError message={error} onRetry={() => void refresh(port)} />}
      {projects.length === 0 ? (
        <SurveyState icon="▱" title="暂无项目" detail="创建项目后，可以在这里查看主聊天或任务发起的需求调研。" />
      ) : (
        <>
          <div className="requirement-survey-section-title">
            <div><h2>项目</h2><p>选择项目查看主聊天与任务的调研记录</p></div>
            <button aria-label="刷新需求调研" disabled={isLoading} onClick={() => void refresh(port)} type="button">↻</button>
          </div>
          <div className="requirement-survey-project-grid">
            {projects.map((project) => {
              const projectSurveys = surveys.filter((survey) => survey.projectId === project.id);
              const open = projectSurveys.filter((survey) => survey.status === "open").length;
              const resolved = projectSurveys.filter((survey) => survey.status === "resolved").length;
              return (
                <button key={project.id} onClick={() => setSelectedProjectId(project.id)} type="button">
                  <span className={`requirement-survey-project-icon ${open > 0 ? "is-open" : ""}`}>▰</span>
                  <span className="requirement-survey-project-copy">
                    <strong>{project.title}</strong>
                    <small>{projectSurveys.length === 0 ? "暂无需求调研" : `共 ${projectSurveys.length} 张调研单`}</small>
                    <span><em className={open > 0 ? "is-open" : ""}>{open} 待填写</em><em className={resolved > 0 ? "is-resolved" : ""}>{resolved} 已提交</em></span>
                  </span>
                  <b aria-hidden="true">›</b>
                </button>
              );
            })}
          </div>
          {isLoading && surveys.length === 0 && <div className="requirement-survey-loading"><span className="mini-spinner" />正在读取需求调研…</div>}
        </>
      )}
    </section>
  );
}

function SurveyHero({ openCount, resolvedCount }: { openCount: number; resolvedCount: number }) {
  return (
    <header className="requirement-survey-hero">
      <span className="requirement-survey-hero__icon" aria-hidden="true">☷</span>
      <div><h2>需求调研</h2><p>了解你的需求，整理目标与后续方案</p></div>
      <dl>
        <div><dt>待填写</dt><dd className="is-open">{openCount}</dd></div>
        <div><dt>已提交</dt><dd className="is-resolved">{resolvedCount}</dd></div>
      </dl>
    </header>
  );
}

function ProjectSurveyList({
  project,
  surveys,
  isLoading,
  error,
  onBack,
  onOpen,
  onRefresh,
}: {
  project: { id: string; title: string };
  surveys: RequirementSurvey[];
  isLoading: boolean;
  error: string | null;
  onBack(): void;
  onOpen(id: string): void;
  onRefresh(): void;
}) {
  const open = surveys.filter((survey) => survey.status === "open");
  const resolved = surveys.filter((survey) => survey.status === "resolved");
  return (
    <section aria-label={`${project.title}的需求调研`} className="requirement-survey-center">
      <header className="requirement-survey-project-header">
        <button onClick={onBack} type="button">‹ 返回项目列表</button>
        <span aria-hidden="true">☑</span>
        <div><h2>{project.title}</h2><p>填写调研，帮助 AI 理解你的目标、偏好和限制。提交后，原 Run 会继续处理。</p></div>
        <dl><div><dt>全部</dt><dd>{surveys.length}</dd></div><div><dt>待填写</dt><dd>{open.length}</dd></div><div><dt>已提交</dt><dd>{resolved.length}</dd></div></dl>
        <button aria-label="刷新当前项目需求调研" disabled={isLoading} onClick={onRefresh} type="button">↻</button>
      </header>
      {error && <SurveyError message={error} onRetry={onRefresh} />}
      {isLoading && surveys.length === 0 ? (
        <SurveyState icon="…" title="正在读取需求调研" detail="正在从本机 Host 获取当前项目的调研记录。" loading />
      ) : surveys.length === 0 ? (
        <SurveyState icon="☷" title="暂无需求调研" detail="本项目的主聊天或任务还没有发起调研。" />
      ) : (
        <div className="requirement-survey-list-sections">
          {open.length > 0 && <SurveySection title="待填写" surveys={open} onOpen={onOpen} />}
          {resolved.length > 0 && <SurveySection title="已提交" surveys={resolved} onOpen={onOpen} />}
        </div>
      )}
    </section>
  );
}

function SurveySection({ title, surveys, onOpen }: { title: string; surveys: RequirementSurvey[]; onOpen(id: string): void }) {
  return (
    <section className="requirement-survey-list-section">
      <h3><span aria-hidden="true">{title === "待填写" ? "✎" : "✓"}</span>{title}<em>{surveys.length}</em></h3>
      <div className="requirement-survey-card-grid">
        {surveys.map((survey) => (
          <button key={survey.surveyId} onClick={() => onOpen(survey.surveyId)} type="button">
            <span className={`requirement-survey-card-icon is-${survey.status}`} aria-hidden="true">{survey.status === "open" ? "✎" : "✓"}</span>
            <span><strong>{survey.title}</strong>{survey.description && <p>{survey.description}</p>}<small>{survey.questions.length} 个问题 · {survey.sourceTaskId ? "任务" : "主聊天"} · {formatSurveyTime(survey.createdAtUnixMs)}</small></span>
            <em className={`is-${survey.status}`}>{survey.status === "open" ? "待填写" : "已提交"}</em>
            <b aria-hidden="true">›</b>
          </button>
        ))}
      </div>
    </section>
  );
}

function RequirementSurveyDetail({
  survey,
  isSubmitting,
  ownerUserId,
  port,
  onBack,
  onSubmit,
}: {
  survey: RequirementSurvey;
  isSubmitting: boolean;
  ownerUserId: string;
  port: RequirementSurveyPort;
  onBack(): void;
  onSubmit(answers: RequirementSurveyAnswers): Promise<boolean>;
}) {
  const [answers, setAnswers] = useState<RequirementSurveyAnswers>(survey.answers ?? {});
  const canSubmit = isRequirementSurveySubmissionValid(survey, answers);
  const setTextAnswer = (questionId: string, required: boolean, value: string) => {
    setAnswers((current) => {
      if (!required && value.length === 0) {
        const next = { ...current };
        delete next[questionId];
        return next;
      }
      return { ...current, [questionId]: value };
    });
  };
  return (
    <section aria-label="需求调研详情" className="requirement-survey-detail">
      <div className="requirement-survey-detail__back"><button onClick={onBack} type="button">‹ 返回调研列表</button></div>
      <div className="requirement-survey-detail__content">
        <header><span>{survey.sourceTaskId ? "任务需求调研" : "主聊天需求调研"}</span><em className={`is-${survey.status}`}>{survey.status === "open" ? "等待你的输入" : "已提交"}</em><h2>{survey.title}</h2>{survey.description && <p>{survey.description}</p>}</header>
        {survey.questions.map((question, index) => (
          <fieldset disabled={survey.status === "resolved" || isSubmitting} key={question.questionId}>
            <legend>{index + 1}. {question.prompt}{question.required && <em> *</em>}</legend>
            {question.responseKind === "text" ? (
              <textarea
                aria-label={question.prompt}
                maxLength={16_000}
                onChange={(event) => setTextAnswer(question.questionId, question.required, event.target.value)}
                value={typeof answers[question.questionId] === "string" ? answers[question.questionId] as string : ""}
              />
            ) : question.responseKind === "boolean" ? (
              <div className="requirement-survey-detail__choices">
                {[true, false].map((value) => <label className={answers[question.questionId] === value ? "is-selected" : ""} key={String(value)}><input checked={answers[question.questionId] === value} name={question.questionId} onChange={() => setAnswers((current) => ({ ...current, [question.questionId]: value }))} type="radio" />{value ? "是" : "否"}</label>)}
              </div>
            ) : (
              <div className="requirement-survey-detail__choices is-stacked">
                {question.options.map((option, optionIndex) => {
                  const multiple = question.responseKind === "multiple_choice";
                  const selected = multiple
                    ? Array.isArray(answers[question.questionId]) && (answers[question.questionId] as string[]).includes(option)
                    : answers[question.questionId] === option;
                  return <label className={selected ? "is-selected" : ""} key={option}><input aria-label={option} checked={selected} name={question.questionId} onChange={() => setAnswers((current) => multiple ? toggleRequirementSurveyChoice(current, question.questionId, option) : { ...current, [question.questionId]: option })} type={multiple ? "checkbox" : "radio"} /><span>{option}</span><small>{optionLetter(optionIndex)}</small></label>;
                })}
              </div>
            )}
          </fieldset>
        ))}
        {survey.status === "open" ? (
          <footer><span>{!canSubmit && "请完成所有必填题"}</span><button className="primary-button" disabled={!canSubmit || isSubmitting} onClick={() => void onSubmit(answers)} type="button">{isSubmitting ? "提交中…" : "提交并继续"}</button></footer>
        ) : (
          <>
            <div className="requirement-survey-detail__resolved">✓ 你的回答已提交，原 Run 已恢复。</div>
            <RequirementSurveyFollowUpPanel ownerUserId={ownerUserId} port={port} survey={survey} />
          </>
        )}
      </div>
    </section>
  );
}

function RequirementSurveyFollowUpPanel({
  ownerUserId,
  port,
  survey,
}: {
  ownerUserId: string;
  port: RequirementSurveyPort;
  survey: RequirementSurvey;
}) {
  const snapshot = useRequirementSurveyFollowUpStore((state) => state.snapshot);
  const isLoading = useRequirementSurveyFollowUpStore((state) => state.isLoading);
  const action = useRequirementSurveyFollowUpStore((state) => state.action);
  const error = useRequirementSurveyFollowUpStore((state) => state.error);
  const activate = useRequirementSurveyFollowUpStore((state) => state.activate);
  const refresh = useRequirementSurveyFollowUpStore((state) => state.refresh);
  const confirm = useRequirementSurveyFollowUpStore((state) => state.confirm);
  const adopt = useRequirementSurveyFollowUpStore((state) => state.adopt);
  const reset = useRequirementSurveyFollowUpStore((state) => state.reset);

  useEffect(() => {
    void activate(port, ownerUserId, survey);
    return reset;
  }, [activate, ownerUserId, port, reset, survey.projectId, survey.surveyId, survey.version]);

  useEffect(() => {
    if (!snapshot || (snapshot.followUp.status !== "processing"
      && snapshot.planFollowUp?.status !== "processing")) return undefined;
    const timer = window.setInterval(() => {
      if (document.visibilityState === "visible") void refresh(port);
    }, 5_000);
    return () => window.clearInterval(timer);
  }, [port, refresh, snapshot]);

  const planReady = snapshot?.planFollowUp?.status === "completed"
    || snapshot?.planFollowUp?.status === "completed_without_reply";
  return (
    <section aria-label="方案与后续回复" className="requirement-survey-follow-up">
      <header><div><span aria-hidden="true">▤</span><strong>方案与后续回复</strong></div><button aria-label="刷新方案与后续回复" disabled={isLoading || action != null} onClick={() => void refresh(port)} type="button">↻</button></header>
      {error && <SurveyError message={error} onRetry={() => void refresh(port)} />}
      {isLoading && !snapshot ? (
        <div className="requirement-survey-follow-up__status"><span className="mini-spinner" />正在读取后续结果…</div>
      ) : snapshot?.plan ? (
        <div className="requirement-survey-plan">
          <div className="requirement-survey-plan__meta"><span>方案版本 r{snapshot.plan.revision}</span>{snapshot.plan.confirmedAtUnixMs != null ? <em className="is-confirmed">✓ 方案已确认</em> : <em>等待确认</em>}</div>
          <h3>{snapshot.plan.title}</h3>
          <SafeMarkdownDocument markdown={snapshot.plan.markdown} />
          {snapshot.plan.confirmedAtUnixMs == null ? (
            <div className="requirement-survey-plan__action">
              <p>确认只表示认可当前方案版本，不会自动开始任务；后续任务会固定引用这个版本。</p>
              {!planReady && <FollowUpStatus followUp={snapshot.planFollowUp} />}
              <button className="primary-button" disabled={!planReady || action != null} onClick={() => void confirm(port)} type="button">{action === "confirm" ? "确认中…" : "确认当前方案"}</button>
            </div>
          ) : (
            <div className="requirement-survey-plan__confirmed"><strong>方案已确认，本次不会自动执行任何任务</strong><p>需要评审、生成文档或开始实施时，请回到原对话说明下一步。</p></div>
          )}
        </div>
      ) : (
        <div className="requirement-survey-follow-up__body">
          <FollowUpStatus followUp={snapshot?.followUp ?? null} />
          {snapshot?.followUp.status === "completed" && (
            <><p>这份历史回复尚未保存为方案版本。保存并确认不会自动开始任务。</p><button className="primary-button" disabled={action != null} onClick={() => void adopt(port)} type="button">{action === "adopt" ? "保存并确认中…" : "保存并确认此方案"}</button></>
          )}
        </div>
      )}
    </section>
  );
}

function FollowUpStatus({ followUp }: { followUp: import("../domain/requirementSurvey").RequirementSurveyFollowUp | null }) {
  if (!followUp) return <div className="requirement-survey-follow-up__status"><span className="mini-spinner" />正在读取后续结果…</div>;
  if (followUp.status === "completed" && followUp.markdown) return <SafeMarkdownDocument markdown={followUp.markdown} />;
  const messages: Record<typeof followUp.status, string> = {
    awaiting_answers: "请先填写并提交调研。",
    processing: "正在根据你的回答整理需求与后续方案…",
    completed: "后续回复已完成。",
    completed_without_reply: "后续处理已结束，暂无方案回复。请在原对话中查看详情。",
    waiting_for_input: "还需要补充信息，请查看待填写调研或原对话。",
    needs_review: "后续处理已暂停，请在原对话中查看需要处理的事项。",
    failed: "后续处理未完成，请在原对话中查看原因并重试。",
    cancelled: "后续处理已取消，你提交的回答仍已保留。",
  };
  return <div className={`requirement-survey-follow-up__status is-${followUp.status}`}>{followUp.status === "processing" && <span className="mini-spinner" />}{messages[followUp.status]}</div>;
}

function SafeMarkdownDocument({ markdown }: { markdown: string }) {
  return (
    <article aria-label="方案正文" className="requirement-survey-markdown">
      {markdown.split(/\n{2,}/u).map((block, index) => {
        const value = block.trim();
        if (!value) return null;
        const heading = /^(#{1,4})\s+(.+)$/u.exec(value);
        if (heading) return <h4 key={index}>{heading[2]}</h4>;
        const lines = value.split("\n");
        if (lines.every((line) => /^[-*]\s+/u.test(line))) {
          return <ul key={index}>{lines.map((line, lineIndex) => <li key={lineIndex}>{line.replace(/^[-*]\s+/u, "")}</li>)}</ul>;
        }
        if (lines.every((line) => /^\d+\.\s+/u.test(line))) {
          return <ol key={index}>{lines.map((line, lineIndex) => <li key={lineIndex}>{line.replace(/^\d+\.\s+/u, "")}</li>)}</ol>;
        }
        return <p key={index}>{lines.map((line, lineIndex) => <span key={lineIndex}>{line}{lineIndex < lines.length - 1 && <br />}</span>)}</p>;
      })}
    </article>
  );
}

function SurveyError({ message, onRetry }: { message: string; onRetry(): void }) {
  return <div className="requirement-survey-error" role="alert"><span>!</span><p>{message}</p><button onClick={onRetry} type="button">重试</button></div>;
}

function SurveyState({ icon, title, detail, loading = false }: { icon: string; title: string; detail: string; loading?: boolean }) {
  return <div className="requirement-survey-state">{loading ? <span className="mini-spinner" /> : <span aria-hidden="true">{icon}</span>}<strong>{title}</strong><p>{detail}</p></div>;
}

function formatSurveyTime(timestamp: number) {
  return new Intl.DateTimeFormat("zh-CN", { dateStyle: "medium", timeStyle: "short" }).format(new Date(timestamp));
}

function optionLetter(index: number) {
  return index >= 0 && index < 26 ? String.fromCharCode(65 + index) : String(index + 1);
}
