import { useEffect, useRef, useState, type ReactNode } from "react";
import { useAgentTeamTodoStore } from "../application/agentTeamTodoStore";
import type {
  AgentTeamDetail,
  AgentTeamTodo,
  AgentTeamTodoProgress,
} from "../domain/agentManagement";
import type { AgentManagementPort } from "../ports/AgentManagementPort";
import { SafeMarkdownPreview } from "../../notepad/ui/SafeMarkdownPreview";
import { formatTeamMessageTime } from "./AgentManagementUiPrimitives";

export function todoStatusLabel(status: AgentTeamTodo["status"]) {
  return ({ pending: "等待", in_progress: "执行中", blocked: "项目经理协调中", completed: "已完成", cancelled: "已取消" })[status];
}

export function AgentTeamTodosView({
  detail,
  focusTarget,
  onFocusHandled,
  ownerUserId,
  port,
}: {
  detail: AgentTeamDetail;
  focusTarget: { roomId: string; todoId: string; requestId: number } | null;
  onFocusHandled(requestId: number): void;
  ownerUserId: string;
  port: AgentManagementPort;
}) {
  const todos = useAgentTeamTodoStore((state) => state.todos);
  const todoOwnerUserId = useAgentTeamTodoStore((state) => state.ownerUserId);
  const todoTeamId = useAgentTeamTodoStore((state) => state.teamId);
  const isLoading = useAgentTeamTodoStore((state) => state.isLoading);
  const error = useAgentTeamTodoStore((state) => state.error);
  const activate = useAgentTeamTodoStore((state) => state.activate);
  const refresh = useAgentTeamTodoStore((state) => state.refresh);
  const reset = useAgentTeamTodoStore((state) => state.reset);
  const [expandedTodoId, setExpandedTodoId] = useState<string | null>(null);
  const [resultTodo, setResultTodo] = useState<AgentTeamTodo | null>(null);
  const [page, setPage] = useState(0);
  const [focusError, setFocusError] = useState<string | null>(null);
  const [activeFocus, setActiveFocus] = useState<{ todoId: string; requestId: number } | null>(null);
  const handledFocusRequest = useRef<number | null>(null);
  const cycling = useRef(false);
  const pageSize = 10;
  const supportsTodos = Boolean(
    port.listTeamTodos && port.dispatchNextTeamDelivery,
  );

  useEffect(() => {
    setExpandedTodoId(null);
    setResultTodo(null);
    setPage(0);
    if (supportsTodos) void activate(port, ownerUserId, detail.team.id);
    else reset();
    return reset;
  }, [activate, detail.team.id, ownerUserId, port, reset, supportsTodos]);

  useEffect(() => {
    if (!supportsTodos) return undefined;
    let cancelled = false;
    const cycle = async () => {
      if (cycling.current || cancelled) return;
      cycling.current = true;
      try {
        await port.loadTeamRuns?.(ownerUserId, detail.team.id);
        await refresh(port);
        if (!cancelled) {
          await port.dispatchNextTeamDelivery?.(ownerUserId, detail.team.id);
          await refresh(port);
        }
      } catch {
        // The stores retain the last trusted projection and expose their own retry state.
      } finally {
        cycling.current = false;
      }
    };
    const timer = window.setInterval(() => void cycle(), 2_000);
    void cycle();
    return () => { cancelled = true; window.clearInterval(timer); };
  }, [detail.team.id, ownerUserId, port, refresh, supportsTodos]);

  useEffect(() => {
    const lastPage = Math.max(0, Math.ceil(todos.length / pageSize) - 1);
    if (page > lastPage) setPage(lastPage);
  }, [page, todos.length]);

  useEffect(() => {
    if (!focusTarget || handledFocusRequest.current === focusTarget.requestId
      || todoOwnerUserId !== ownerUserId || todoTeamId !== detail.team.id || isLoading) return;
    handledFocusRequest.current = focusTarget.requestId;
    const index = todos.findIndex((todo) => todo.id === focusTarget.todoId);
    if (index < 0) {
      setFocusError("无法定位来源团队任务；该任务可能已被删除或不属于当前账号。");
      onFocusHandled(focusTarget.requestId);
      return;
    }
    setFocusError(null);
    setPage(Math.floor(index / pageSize));
    setExpandedTodoId(focusTarget.todoId);
    setActiveFocus({ todoId: focusTarget.todoId, requestId: focusTarget.requestId });
    onFocusHandled(focusTarget.requestId);
  }, [
    detail.team.id,
    focusTarget,
    isLoading,
    onFocusHandled,
    ownerUserId,
    todoOwnerUserId,
    todoTeamId,
    todos,
  ]);

  if (!supportsTodos) {
    return (
      <div className="agent-team-todos__state is-error" role="alert">
        <strong>当前版本尚未连接团队任务</strong><span>请更新本机客户端后重试。</span>
      </div>
    );
  }

  const visibleTodos = todos.slice(page * pageSize, (page + 1) * pageSize);
  const deliveredCount = todos.filter((todo) => todo.status === "completed"
    && todo.executionPlan.builtinCapabilities.includes("project_write")).length;
  const summaryOnlyCount = todos.filter((todo) => todo.status === "completed"
    && !todo.executionPlan.builtinCapabilities.includes("project_write")).length;
  const reviewCount = todos.filter((todo) => todo.status === "blocked").length;
  const waitingCount = todos.filter((todo) => todo.status === "pending").length;
  const firstVisible = todos.length === 0 ? 0 : page * pageSize + 1;
  const lastVisible = Math.min(todos.length, (page + 1) * pageSize);
  const pageCount = Math.max(1, Math.ceil(todos.length / pageSize));

  return (
    <div className="agent-team-todos">
      {focusError && <div className="agent-team-conversation__error" role="alert">{focusError}</div>}
      {error && (
        <div className="agent-team-conversation__error" role="alert">
          <span>{error}</span><button onClick={() => void refresh(port)} type="button">刷新</button>
        </div>
      )}
      {isLoading && todos.length === 0 ? (
        <div className="agent-team-todos__state"><span className="mini-spinner" />正在读取团队任务…</div>
      ) : todos.length === 0 ? (
        <div className="agent-team-todos__empty">
          <span aria-hidden="true">☷</span>
          <strong>还没有团队任务</strong>
          <p>项目经理创建的任务会显示在这里。</p>
        </div>
      ) : (
        <div className="agent-team-todos__scroll">
          <section aria-label="团队交付概览" className="agent-team-todo-overview">
            <div><strong>{deliveredCount}</strong><span>项目文件交付</span></div>
            <div><strong>{summaryOnlyCount}</strong><span>管理 / 检查完成</span></div>
            <div><strong>{reviewCount}</strong><span>需人工处理</span></div>
            <div><strong>{waitingCount}</strong><span>待执行</span></div>
            <p>“交付”按已完成且通过 Host 项目写入证据校验的任务统计；计划交付、授权工具和执行总结不会被当作实际项目产出。</p>
          </section>
          <div className="agent-team-todos__list">
            {visibleTodos.map((todo) => (
              <AgentTeamTodoCard
                expanded={expandedTodoId === todo.id}
                focusRequestId={activeFocus?.todoId === todo.id ? activeFocus.requestId : null}
                key={todo.id}
                onOpenResult={() => setResultTodo(todo)}
                onToggle={() => setExpandedTodoId((current) => current === todo.id ? null : todo.id)}
                ownerUserId={ownerUserId}
                port={port}
                teamId={detail.team.id}
                todo={todo}
              />
            ))}
          </div>
          <nav aria-label="团队任务分页" className="agent-team-todos__pagination">
            <span>{firstVisible}–{lastVisible} / {todos.length}</span>
            <div>
              <button aria-label="上一页任务" disabled={page === 0} onClick={() => setPage((value) => value - 1)} type="button">‹</button>
              <span>{page + 1} / {pageCount}</span>
              <button aria-label="下一页任务" disabled={page + 1 >= pageCount} onClick={() => setPage((value) => value + 1)} type="button">›</button>
            </div>
          </nav>
        </div>
      )}
      {resultTodo && <AgentTeamTodoResultDialog onClose={() => setResultTodo(null)} todo={resultTodo} />}
    </div>
  );
}

function AgentTeamTodoCard({
  expanded,
  focusRequestId,
  onOpenResult,
  onToggle,
  ownerUserId,
  port,
  teamId,
  todo,
}: {
  expanded: boolean;
  focusRequestId: number | null;
  onOpenResult: () => void;
  onToggle: () => void;
  ownerUserId: string;
  port: AgentManagementPort;
  teamId: string;
  todo: AgentTeamTodo;
}) {
  const [progress, setProgress] = useState<AgentTeamTodoProgress[]>([]);
  const [progressError, setProgressError] = useState<string | null>(null);
  const cardRef = useRef<HTMLElement | null>(null);

  useEffect(() => {
    if (focusRequestId == null) return undefined;
    const frame = window.requestAnimationFrame(() => {
      cardRef.current?.scrollIntoView?.({ block: "center" });
    });
    return () => window.cancelAnimationFrame(frame);
  }, [focusRequestId]);

  useEffect(() => {
    if (!port.listTeamTodoProgress || todo.status === "pending") {
      setProgress([]);
      setProgressError(null);
      return undefined;
    }
    let cancelled = false;
    const load = async () => {
      try {
        const next = await port.listTeamTodoProgress!(ownerUserId, teamId, todo.id, 100);
        if (!cancelled) {
          setProgress(next);
          setProgressError(null);
        }
      } catch (failure) {
        if (!cancelled) setProgressError(failure instanceof Error ? failure.message : "执行记录暂时不可用。");
      }
    };
    void load();
    const timer = todo.status === "in_progress"
      ? window.setInterval(() => void load(), 2_000)
      : null;
    return () => {
      cancelled = true;
      if (timer != null) window.clearInterval(timer);
    };
  }, [ownerUserId, port, teamId, todo.id, todo.status, todo.updatedAtUnixMs]);

  const latest = progress.at(-1) ?? null;
  const toolCount = todo.executionPlan.builtinCapabilities.reduce(
    (count, capability) => count + teamTodoCapability(capability).tools.length,
    0,
  );
  const hasProjectFiles = todo.status === "completed"
    && todo.executionPlan.builtinCapabilities.includes("project_write");
  const statusColor = todo.status === "blocked" ? "warning" : todo.status;
  return (
    <article className={`agent-team-todo-card is-${statusColor}`} ref={cardRef}>
      <div className="agent-team-todo-card__identity">
        <span aria-hidden="true" className="agent-team-todo-card__avatar">{todo.agentName.trim().slice(0, 1) || "A"}</span>
        <div>
          <div className="agent-team-todo-card__title-row">
            <strong>{todo.title}</strong>
            <span className={`agent-team-todo-card__status is-${statusColor}`}>{todoStatusIcon(todo.status)} {todoStatusLabel(todo.status)}</span>
          </div>
          <small>●&nbsp; 负责人 · {todo.agentName}</small>
          <p>{collapsedTeamTodoText(todo.executionContract.objective, expanded ? Number.POSITIVE_INFINITY : 220)}</p>
        </div>
      </div>
      {todo.status === "blocked" && (
        <div className="agent-team-todo-card__notice">
          <span>⚠ {collapsedTeamTodoText(todo.blockedReason || "任务已阻塞，但未记录原因。", expanded ? Number.POSITIVE_INFINITY : 180)}</span>
          <strong>项目经理协调中</strong>
        </div>
      )}
      {latest && !expanded && (
        <section className={`agent-team-todo-card__latest is-${latest.kind}`} aria-label="最新执行记录">
          <div>
            <strong>{todoProgressStageLabel(latest)}</strong>
            <p>{latest.detail}</p>
          </div>
          <time>{formatTeamMessageTime(latest.createdAtUnixMs)}</time>
          {progress.length > 1 && <button onClick={onToggle} type="button">查看全部 {progress.length} 条</button>}
        </section>
      )}
      <div className="agent-team-todo-card__metadata">
        {todo.executionContract.expectedOutputs.length > 0 && <span>◇ 计划 {todo.executionContract.expectedOutputs.length} 项交付</span>}
        {todo.executionContract.acceptanceCriteria.length > 0 && <span>◉ {todo.executionContract.acceptanceCriteria.length} 项验收标准</span>}
        {toolCount > 0 && <span>⌘ 已授权 {toolCount} 个工具</span>}
        {todo.executionPlan.plugins.length > 0 && <span>◇ {todo.executionPlan.plugins.length} 个插件</span>}
        {progress.length > 0 && <span>⊙ 实际记录 {progress.length} 次</span>}
        {hasProjectFiles && <span className="is-delivered">▣ 已记录项目文件交付</span>}
        {!hasProjectFiles && todo.result && <span className="is-summary">▤ {todo.executionPlan.builtinCapabilities.includes("project_write") ? "有总结，未记录文件提交" : "管理 / 检查结果"}</span>}
        <button aria-expanded={expanded} onClick={onToggle} type="button">⌄ {expanded ? "收起" : "查看详情"}</button>
      </div>
      {expanded && (
        <div className="agent-team-todo-card__details">
          {todo.executionContract.scope && <TeamTodoDetailBlock icon="⌖" title="范围"><p>{todo.executionContract.scope}</p></TeamTodoDetailBlock>}
          <TeamTodoStringList icon="◇" title="交付物" values={todo.executionContract.expectedOutputs} />
          <TeamTodoStringList icon="✓" title="验收条件" values={todo.executionContract.acceptanceCriteria} />
          <TeamTodoStringList icon="●" title="约束" values={todo.executionContract.constraints} />
          <TeamTodoAuthorization plan={todo.executionPlan} />
          {latest && (
            <TeamTodoDetailBlock icon="↻" title="执行记录">
              <ol className="agent-team-todo-progress-list">
                {[...progress].reverse().map((item) => (
                  <li key={item.sequence}>
                    <div><strong>{todoProgressStageLabel(item)}</strong><time>{formatTeamMessageTime(item.createdAtUnixMs)}</time></div>
                    <p>{item.detail}</p>
                  </li>
                ))}
              </ol>
            </TeamTodoDetailBlock>
          )}
          {progressError && <div className="agent-team-todo-progress__error">{progressError}</div>}
          {todo.result && (
            <section className={`agent-team-todo-card__result ${hasProjectFiles ? "is-delivered" : ""}`}>
              <div><strong>{hasProjectFiles ? "项目交付结果" : "管理 / 检查结果"}</strong><p>{todo.result}</p></div>
              <button onClick={onOpenResult} type="button">查看完整结果</button>
            </section>
          )}
        </div>
      )}
    </article>
  );
}

function TeamTodoDetailBlock({
  children,
  icon,
  title,
}: {
  children: ReactNode;
  icon: string;
  title: string;
}) {
  return (
    <section className="agent-team-todo-detail-block">
      <strong><span aria-hidden="true">{icon}</span>{title}</strong>
      {children}
    </section>
  );
}

function TeamTodoStringList({ icon, title, values }: { icon: string; title: string; values: string[] }) {
  if (values.length === 0) return null;
  return (
    <TeamTodoDetailBlock icon={icon} title={title}>
      <ul>{values.map((value, index) => <li key={`${index}-${value}`}><span aria-hidden="true" />{value}</li>)}</ul>
    </TeamTodoDetailBlock>
  );
}

function TeamTodoAuthorization({ plan }: { plan: AgentTeamTodo["executionPlan"] }) {
  const capabilities = plan.builtinCapabilities.map(teamTodoCapability);
  return (
    <TeamTodoDetailBlock icon="◆" title="任务授权">
      <div className="agent-team-todo-authorization__mode">
        <strong>执行模式</strong><span>{plan.requiresExecution ? "独立执行环境" : "仅协调，无独立执行"}</span>
      </div>
      {capabilities.length === 0 ? (
        <p className="agent-team-todo-authorization__empty">未授权内置执行工具。</p>
      ) : (
        <div className="agent-team-todo-authorization__list">
          <small>内置工具</small>
          {capabilities.map((capability) => (
            <section key={capability.key}>
              <header><strong>{capability.name}</strong><code>{capability.key}</code></header>
              <p>{capability.detail}</p>
              <code>{capability.tools.join("  ·  ")}</code>
            </section>
          ))}
        </div>
      )}
      <div className="agent-team-todo-authorization__plugins">
        <small>插件</small>
        {plan.plugins.length === 0 ? <p>未授权额外插件。</p> : plan.plugins.map((plugin) => (
          <section key={plugin.pluginId}>
            <strong>{plugin.displayName}</strong><code>{plugin.pluginId}</code>
            {plugin.reason && <span>授权理由：{plugin.reason}</span>}
          </section>
        ))}
      </div>
      <footer>
        <code>授权快照：{plan.selectionRevision}</code>
        {plan.selectedAtUnixMs > 0 && <time> · {formatTeamMessageTime(plan.selectedAtUnixMs)}</time>}
      </footer>
      <p className="agent-team-todo-authorization__note">这里展示任务创建时锁定的执行授权；Agent 职业与项目规则不属于本任务单独授权。</p>
    </TeamTodoDetailBlock>
  );
}

function AgentTeamTodoResultDialog({ onClose, todo }: { onClose: () => void; todo: AgentTeamTodo }) {
  return (
    <div className="agent-editor-backdrop" onMouseDown={onClose} role="presentation">
      <section aria-label="团队任务完整结果" aria-modal="true" className="agent-team-todo-result-dialog" onMouseDown={(event) => event.stopPropagation()} role="dialog">
        <header>
          <span aria-hidden="true" className="agent-team-todo-card__avatar">{todo.agentName.trim().slice(0, 1) || "A"}</span>
          <div><h2>{todo.title}</h2><p>●&nbsp; 负责人 · {todo.agentName}</p></div>
          <button onClick={onClose} type="button">完成</button>
        </header>
        <div>
          <h3>▤ 执行结果</h3>
          <SafeMarkdownPreview markdown={todo.result} />
        </div>
      </section>
    </div>
  );
}

function collapsedTeamTodoText(value: string, maximumCharacters: number) {
  const normalized = value.trim().split(/\s+/u).join(" ");
  if (!Number.isFinite(maximumCharacters) || normalized.length <= maximumCharacters) return normalized;
  return `${normalized.slice(0, maximumCharacters).trimEnd()}…`;
}

function todoStatusIcon(status: AgentTeamTodo["status"]) {
  return ({ pending: "◷", in_progress: "↻", blocked: "⚠", completed: "●", cancelled: "⊗" })[status];
}

function teamTodoCapability(capability: AgentTeamTodo["executionPlan"]["builtinCapabilities"][number]) {
  return {
    project_read: {
      key: capability,
      name: "项目读取",
      detail: "仅在当前任务绑定的项目目录内读取、列目录和搜索。",
      tools: ["project_read", "project_list", "project_search"],
    },
    project_write: {
      key: capability,
      name: "项目写入",
      detail: "通过暂存与提交会话修改当前项目；提交仍受运行时审批约束。",
      tools: ["project_write", "project_apply_patch", "project_commit"],
    },
    terminal: {
      key: capability,
      name: "项目终端",
      detail: "在当前任务绑定的项目目录内执行和管理命令进程。",
      tools: ["terminal_execute", "terminal_poll", "terminal_write", "terminal_kill"],
    },
    requirement_survey_read: {
      key: capability,
      name: "需求调研读取",
      detail: "读取当前团队 Todo 绑定的调研答案与方案上下文。",
      tools: [],
    },
    requirement_survey_write: {
      key: capability,
      name: "需求调研",
      detail: "在正式需求调研中心创建问卷，并在提交后保存版本化方案。",
      tools: ["requirement_survey_create", "requirement_plan_save"],
    },
  }[capability];
}

function todoProgressStageLabel(progress: AgentTeamTodoProgress) {
  if (progress.stage.trim()) return progress.stage;
  return {
    started: "开始执行",
    progress: "执行进展",
    blocked: "执行阻塞",
    completed: "执行完成",
    cancelled: "已取消",
  }[progress.kind];
}
