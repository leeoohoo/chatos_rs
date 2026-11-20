import { useEffect, useState } from "react";
import {
  agentProfessionLabels,
  type AgentConversationActivitySnapshot,
  type AgentProfileActivitySnapshot,
  type AgentProfileSummary,
  type AgentStaffingProposal,
  type AgentTeamSummary,
  type PreparedAgentConversation,
} from "../domain/agentManagement";
import type { AgentManagementPort } from "../ports/AgentManagementPort";

export function AgentConversationActivity({
  conversation,
  ownerUserId,
  port,
  refreshSequence,
}: {
  conversation: PreparedAgentConversation;
  ownerUserId: string;
  port: AgentManagementPort;
  refreshSequence: number;
}) {
  const [snapshot, setSnapshot] = useState<AgentConversationActivitySnapshot | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [isLoading, setIsLoading] = useState(true);
  const [isExpanded, setIsExpanded] = useState(false);
  const [retrySequence, setRetrySequence] = useState(0);

  useEffect(() => {
    const loadActivity = port.loadConversationActivity;
    if (!loadActivity) return;
    let cancelled = false;
    let timer: number | undefined;
    const load = async () => {
      try {
        const next = await loadActivity(ownerUserId, conversation.roomId);
        if (cancelled) return;
        if (next.roomId !== conversation.roomId
          || next.agentId !== conversation.agentId
          || next.conversationId !== conversation.conversationId) {
          throw new Error("Agent 运行动态身份不一致，请重新打开会话。");
        }
        setSnapshot((current) => (
          !current || next.conversationVersion >= current.conversationVersion ? next : current
        ));
        setError(null);
        setIsLoading(false);
        if (next.hasActiveDelivery) timer = window.setTimeout(() => void load(), 1_000);
      } catch (failure) {
        if (cancelled) return;
        setError(failure instanceof Error ? failure.message : "Agent 运行动态暂时无法读取。");
        setIsLoading(false);
      }
    };
    void load();
    return () => {
      cancelled = true;
      if (timer != null) window.clearTimeout(timer);
    };
  }, [conversation, ownerUserId, port, refreshSequence, retrySequence]);

  const items = snapshot?.items ?? [];
  const active = snapshot?.hasActiveDelivery ?? false;
  return (
    <section aria-label="Agent 运行动态" className="agent-activity" aria-live="polite">
      <div className="agent-activity__summary">
        <span className={active ? "agent-activity__pulse is-active" : "agent-activity__pulse"} />
        <div>
          <strong>{active ? "Agent 正在运行" : items.length ? `最近 ${items.length} 次运行` : "还没有运行记录"}</strong>
          <small>{active ? "Delivery 与 Run 状态由本机原生层持续对账" : "仅展示当前 Agent 私聊的安全运行投影"}</small>
        </div>
        {isLoading && <span className="mini-spinner" aria-label="正在读取 Agent 运行动态" />}
        {items.length > 0 && (
          <button onClick={() => setIsExpanded((value) => !value)} type="button">
            {isExpanded ? "收起" : "查看动态"}
          </button>
        )}
      </div>
      {error && (
        <div className="agent-activity__error" role="alert">
          <span>{error}</span>
          <button onClick={() => { setIsLoading(true); setRetrySequence((value) => value + 1); }} type="button">重试</button>
        </div>
      )}
      {isExpanded && items.length > 0 && (
        <div className="agent-activity__list" aria-label="Agent 最近运行">
          {items.slice(0, 20).map((item) => (
            <article key={item.deliveryId}>
              <span className={`agent-activity__status agent-activity__status--${item.deliveryStatus}`}>
                {agentDeliveryStatusLabel(item.deliveryStatus)}
              </span>
              <div>
                <strong>{agentRunStatusLabel(item.runStatus)}</strong>
                <small>{formatAgentActivityTime(item.updatedAtUnixMs)} · Run {shortIdentity(item.runId)}</small>
              </div>
              <code title={item.deliveryId}>Delivery {shortIdentity(item.deliveryId)}</code>
            </article>
          ))}
          {items.length > 20 && <small>仅显示最新 20 条，共 {items.length} 条。</small>}
        </div>
      )}
    </section>
  );
}

export function AgentStaffingProposalPanel({
  ownerUserId,
  sourceRoomId,
  port,
  profiles,
  teams,
  onResolved,
}: {
  ownerUserId: string;
  sourceRoomId: string;
  port: AgentManagementPort;
  profiles: AgentProfileSummary[];
  teams: AgentTeamSummary[];
  onResolved(): Promise<void>;
}) {
  const [proposals, setProposals] = useState<AgentStaffingProposal[]>([]);
  const [isLoading, setIsLoading] = useState(true);
  const [resolvingId, setResolvingId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    const load = port.listStaffingProposals;
    if (!load) {
      setIsLoading(false);
      return;
    }
    let cancelled = false;
    let timer: number | undefined;
    const refreshProposals = async () => {
      try {
        const next = await load(ownerUserId, sourceRoomId);
        if (cancelled) return;
        if (next.some((proposal) => proposal.sourceRoomId !== sourceRoomId)) {
          throw new Error("Agent 人员提案来源会话不一致。");
        }
        setProposals(next);
        setError(null);
      } catch (failure) {
        if (!cancelled) {
          setError(failure instanceof Error ? failure.message : "Agent 人员提案暂时无法读取。");
        }
      } finally {
        if (!cancelled) {
          setIsLoading(false);
          timer = window.setTimeout(() => void refreshProposals(), 2_000);
        }
      }
    };
    setIsLoading(true);
    setProposals([]);
    setError(null);
    void refreshProposals();
    return () => {
      cancelled = true;
      if (timer != null) window.clearTimeout(timer);
    };
  }, [ownerUserId, port, sourceRoomId]);

  if (!port.listStaffingProposals || !port.resolveStaffingProposal) return null;
  if (!isLoading && proposals.length === 0 && !error) return null;
  const profileNames = new Map(profiles.map((profile) => [profile.id, profile.name]));
  const teamNames = new Map(teams.map((team) => [team.id, team.name]));
  const resolve = async (proposal: AgentStaffingProposal, approved: boolean) => {
    if (resolvingId) return;
    setResolvingId(proposal.id);
    setError(null);
    try {
      const result = await port.resolveStaffingProposal!(
        ownerUserId,
        sourceRoomId,
        proposal.id,
        proposal.kind,
        approved,
      );
      if (result.proposalId !== proposal.id
        || result.sourceRoomId !== sourceRoomId
        || result.status !== (approved ? "approved" : "rejected")) {
        throw new Error("Agent 人员提案审批结果不一致，请刷新后重试。");
      }
      setProposals((current) => current.filter((candidate) => candidate.id !== proposal.id));
      await onResolved();
    } catch (failure) {
      setError(failure instanceof Error ? failure.message : "Agent 人员提案暂时无法处理。");
    } finally {
      setResolvingId(null);
    }
  };

  return (
    <section aria-label="待审批人员提案" className="agent-staffing-proposals" aria-live="polite">
      <header>
        <div>
          <strong>待审批人员提案</strong>
          <small>Agent 只能提交建议；批准后原生事务才会真正变更成员，并自动回执给发起者。</small>
        </div>
        {isLoading && <span aria-label="正在读取人员提案" className="mini-spinner" />}
      </header>
      {error && <div className="agent-staffing-proposals__error" role="alert">{error}</div>}
      {proposals.map((proposal) => {
        const proposer = profileNames.get(proposal.proposerAgentId) ?? "Agent";
        const targetAgent = proposal.targetAgentId
          ? profileNames.get(proposal.targetAgentId) ?? "现有 Agent"
          : null;
        const targetTeam = proposal.targetTeamId
          ? teamNames.get(proposal.targetTeamId) ?? "项目团队"
          : null;
        const title = proposal.kind === "agentCreation"
          ? `创建 Agent：${proposal.name}`
          : proposal.kind === "existingAgentMembership"
            ? `邀请 ${targetAgent} 加入 ${targetTeam}`
            : `将 ${targetAgent} 移出当前团队`;
        return (
          <article key={proposal.id}>
            <div className="agent-staffing-proposals__content">
              <div className="agent-staffing-proposals__title">
                <span>{proposal.kind === "agentRemoval" ? "移出" : "人员"}</span>
                <div>
                  <strong>{title}</strong>
                  <small>{proposer} 提交 · {formatAgentActivityTime(proposal.createdAtUnixMs)}</small>
                </div>
              </div>
              {proposal.role && <p><b>团队角色</b>{proposal.role}</p>}
              {proposal.responsibility && <p><b>职责</b>{proposal.responsibility}</p>}
              {proposal.rationale && <p><b>理由</b>{proposal.rationale}</p>}
              {proposal.reason && <p><b>移出理由</b>{proposal.reason}</p>}
              {proposal.handoffPlan && <p><b>交接计划</b>{proposal.handoffPlan}</p>}
              {proposal.professionKey && (
                <p><b>职业</b>{agentProfessionLabels[proposal.professionKey]}</p>
              )}
              {proposal.thinkingLevel && <p><b>思考等级</b>{proposal.thinkingLevel}</p>}
              {proposal.rolePrompt && (
                <details>
                  <summary>查看完整角色提示词</summary>
                  <pre>{proposal.rolePrompt}</pre>
                </details>
              )}
            </div>
            <footer>
              <button
                disabled={resolvingId != null}
                onClick={() => void resolve(proposal, false)}
                type="button"
              >拒绝</button>
              <button
                className="primary-button"
                disabled={resolvingId != null}
                onClick={() => void resolve(proposal, true)}
                type="button"
              >{resolvingId === proposal.id ? "正在处理…" : "批准"}</button>
            </footer>
          </article>
        );
      })}
    </section>
  );
}

export function AgentProfileActivity({
  ownerUserId,
  port,
  profile,
}: {
  ownerUserId: string;
  port: AgentManagementPort;
  profile: AgentProfileSummary;
}) {
  const [snapshot, setSnapshot] = useState<AgentProfileActivitySnapshot | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [isLoading, setIsLoading] = useState(true);
  const [retrySequence, setRetrySequence] = useState(0);

  useEffect(() => {
    const loadActivity = port.loadProfileActivity;
    if (!loadActivity) return;
    let cancelled = false;
    let timer: number | undefined;
    const load = async () => {
      try {
        const next = await loadActivity(ownerUserId, profile.id);
        if (cancelled) return;
        if (next.agentId !== profile.id) {
          throw new Error("Agent 运行动态身份不一致，请重新选择 Agent。");
        }
        setSnapshot(next);
        setError(null);
        setIsLoading(false);
        if (next.hasActiveDelivery) timer = window.setTimeout(() => void load(), 1_000);
      } catch (failure) {
        if (cancelled) return;
        setError(failure instanceof Error ? failure.message : "Agent 运行动态暂时无法读取。");
        setIsLoading(false);
      }
    };
    setIsLoading(true);
    setError(null);
    void load();
    return () => {
      cancelled = true;
      if (timer != null) window.clearTimeout(timer);
    };
  }, [ownerUserId, port, profile.id, retrySequence]);

  const current = snapshot?.agentId === profile.id ? snapshot : null;
  const items = current?.items ?? [];
  const active = current?.hasActiveDelivery ?? false;
  return (
    <section aria-label={`${profile.name} 运行动态`} className="agent-profile-activity" aria-live="polite">
      <header>
        <div>
          <h3>{profile.name} · 运行动态</h3>
          <p>最近的私聊消息处理</p>
        </div>
        <div>
          {active && <span className="agent-profile-activity__active">正在运行</span>}
          {isLoading && <span className="mini-spinner" aria-label="正在读取 Agent 运行动态" />}
          <button
            disabled={isLoading}
            onClick={() => { setIsLoading(true); setRetrySequence((value) => value + 1); }}
            type="button"
          >刷新</button>
        </div>
      </header>
      {error ? (
        <div className="agent-profile-activity__state is-error" role="alert">
          <span>{error}</span>
          <button onClick={() => setRetrySequence((value) => value + 1)} type="button">重试</button>
        </div>
      ) : !isLoading && items.length === 0 ? (
        <div className="agent-profile-activity__state">
          <strong>还没有私聊运行记录</strong>
          <span>与这个 Agent 开始私聊后，最近的 Delivery 与 Run 会显示在这里。</span>
        </div>
      ) : (
        <div className="agent-activity__list" aria-label="Agent 最近私聊运行">
          {items.slice(0, 20).map((item) => (
            <article key={item.deliveryId}>
              <span className={`agent-activity__status agent-activity__status--${item.deliveryStatus}`}>
                {agentDeliveryStatusLabel(item.deliveryStatus)}
              </span>
              <div>
                <strong>{agentRunStatusLabel(item.runStatus)}</strong>
                <small>{formatAgentActivityTime(item.updatedAtUnixMs)} · Run {shortIdentity(item.runId)}</small>
              </div>
              <code title={item.deliveryId}>Delivery {shortIdentity(item.deliveryId)}</code>
            </article>
          ))}
          {items.length > 20 && <small>仅显示最新 20 条，共 {items.length} 条。</small>}
        </div>
      )}
    </section>
  );
}

function agentDeliveryStatusLabel(status: AgentConversationActivitySnapshot["items"][number]["deliveryStatus"]) {
  return { pending: "等待", running: "执行中", completed: "完成", failed: "失败", cancelled: "已取消" }[status];
}

function agentRunStatusLabel(status: AgentConversationActivitySnapshot["items"][number]["runStatus"]) {
  return { ready: "Run 已准备", running: "Run 正在执行", succeeded: "Run 已完成", failed: "Run 执行失败", cancelled: "Run 已取消" }[status];
}

function formatAgentActivityTime(unixMilliseconds: number) {
  return new Intl.DateTimeFormat("zh-CN", {
    month: "2-digit", day: "2-digit", hour: "2-digit", minute: "2-digit", second: "2-digit",
  }).format(new Date(unixMilliseconds));
}

function shortIdentity(value: string) {
  return value.length <= 12 ? value : `${value.slice(0, 8)}…${value.slice(-4)}`;
}
