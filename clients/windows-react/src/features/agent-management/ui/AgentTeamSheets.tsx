import { useEffect, useMemo, useState } from "react";
import type { AgentTeamMemberMutation } from "../application/agentTeamStore";
import {
  agentProfessionLabels,
  type AgentProfileSummary,
  type AgentTeamDetail,
  type AgentTeamSummary,
  type CreateAgentTeamInput,
} from "../domain/agentManagement";

export function AgentTeamMemberSheet({
  detail,
  error,
  isMutating,
  member,
  profile,
  onCancel,
  onMutate,
}: {
  detail: AgentTeamDetail;
  error: string | null;
  isMutating: boolean;
  member: AgentTeamDetail["members"][number];
  profile: AgentProfileSummary | null;
  onCancel(): void;
  onMutate(mutation: AgentTeamMemberMutation): Promise<void>;
}) {
  const [role, setRole] = useState(member.role);
  const [responsibility, setResponsibility] = useState(member.responsibility);
  const [confirmingRemoval, setConfirmingRemoval] = useState(false);
  const isDefault = detail.team.defaultAgentId === member.agentId;
  const isProjectManager = detail.team.projectManagerAgentId === member.agentId;
  const canManageProject = profile?.professionKey === "project_manager";
  const normalizedRole = role.trim();
  const normalizedResponsibility = responsibility.trim();
  const canSave = normalizedRole.length > 0 && normalizedRole.length <= 160
    && normalizedResponsibility.length <= 8_000;
  return (
    <div className="agent-editor-backdrop" role="presentation">
      <section aria-label="管理团队成员" aria-modal="true" className="agent-editor agent-team-member-editor" role="dialog">
        <header>
          <div><h2>管理团队成员</h2><p>{profile?.name ?? member.agentId}</p></div>
          <button aria-label="关闭团队成员管理" disabled={isMutating} onClick={onCancel} type="button">×</button>
        </header>
        <div className="agent-editor__body">
          {error && <div className="agent-editor__error" role="alert">{error}</div>}
          <label>当前项目角色<input maxLength={160} onChange={(event) => setRole(event.target.value)} value={role} /></label>
          <label>当前项目职责<textarea maxLength={8_000} onChange={(event) => setResponsibility(event.target.value)} rows={4} value={responsibility} /></label>
          <div className="agent-team-member-editor__assignments">
            <div>
              <strong>默认 Agent</strong>
              <small>Human 未点名发送团队消息时，由默认 Agent 接收。</small>
              <button
                disabled={isMutating || isDefault}
                onClick={() => void onMutate({ kind: "setDefault" })}
                type="button"
              >{isDefault ? "当前默认 Agent" : "设为默认 Agent"}</button>
            </div>
            <div>
              <strong>项目经理</strong>
              <small>{canManageProject
                ? "负责团队任务板的创建、分配、优先级和依赖管理。"
                : "只有职业为“项目经理”的 Agent 才能担任此角色。"}</small>
              <button
                disabled={isMutating || isProjectManager || !canManageProject}
                onClick={() => void onMutate({ kind: "setProjectManager" })}
                type="button"
              >{isProjectManager ? "当前项目经理" : "设为项目经理"}</button>
            </div>
          </div>
          <div className="agent-team-member-editor__danger">
            <div><strong>移出团队</strong><small>{isProjectManager
              ? "当前项目经理必须先完成交接。"
              : "移出后，该 Agent 不再接收此团队的任务和消息。"}</small></div>
            {confirmingRemoval ? (
              <div>
                <button disabled={isMutating} onClick={() => setConfirmingRemoval(false)} type="button">取消</button>
                <button
                  className="danger-button"
                  disabled={isMutating || isProjectManager}
                  onClick={() => void onMutate({ kind: "remove" })}
                  type="button"
                >确认移出</button>
              </div>
            ) : (
              <button disabled={isMutating || isProjectManager} onClick={() => setConfirmingRemoval(true)} type="button">移出团队</button>
            )}
          </div>
        </div>
        <footer>
          <button disabled={isMutating} onClick={onCancel} type="button">取消</button>
          <button
            className="primary-button"
            disabled={isMutating || !canSave}
            onClick={() => void onMutate({
              kind: "update",
              input: { role: normalizedRole, responsibility: normalizedResponsibility },
            })}
            type="button"
          >{isMutating ? "正在保存…" : "保存"}</button>
        </footer>
      </section>
    </div>
  );
}

export function InviteAgentToTeamSheet({
  detail,
  error,
  isInviting,
  profiles,
  onCancel,
  onInvite,
}: {
  detail: AgentTeamDetail;
  error: string | null;
  isInviting: boolean;
  profiles: AgentProfileSummary[];
  onCancel(): void;
  onInvite(agentId: string): Promise<void>;
}) {
  const availableProfiles = useMemo(() => {
    const memberIds = new Set(detail.members.map((member) => member.agentId));
    return profiles.filter((profile) => !memberIds.has(profile.id));
  }, [detail.members, profiles]);
  const [agentId, setAgentId] = useState(availableProfiles[0]?.id ?? "");

  useEffect(() => {
    if (!availableProfiles.some((profile) => profile.id === agentId)) {
      setAgentId(availableProfiles[0]?.id ?? "");
    }
  }, [agentId, availableProfiles]);

  return (
    <div className="agent-editor-backdrop" role="presentation">
      <section aria-label="邀请 Agent" aria-modal="true" className="agent-editor agent-team-invite" role="dialog">
        <header>
          <h2>邀请 Agent</h2>
          <button aria-label="关闭邀请 Agent" disabled={isInviting} onClick={onCancel} type="button">×</button>
        </header>
        <div className="agent-editor__body">
          <p className="agent-team-creator__intro">选择一个已有 Agent 加入“{detail.team.name}”。</p>
          {error && <div className="agent-editor__error" role="alert">{error}</div>}
          {availableProfiles.length === 0 ? (
            <div className="agent-team-creator__empty">所有已有 Agent 都已加入当前团队，或尚未创建 Agent。</div>
          ) : (
            <div aria-label="可邀请 Agent" className="agent-team-invite__list" role="listbox">
              {availableProfiles.map((profile) => (
                <button
                  aria-selected={profile.id === agentId}
                  key={profile.id}
                  onClick={() => setAgentId(profile.id)}
                  role="option"
                  type="button"
                >
                  <span aria-hidden="true">{profile.name.slice(0, 1).toUpperCase()}</span>
                  <div><strong>{profile.name}</strong><small>{agentProfessionLabels[profile.professionKey]}</small></div>
                  {profile.id === agentId && <em aria-hidden="true">✓</em>}
                </button>
              ))}
            </div>
          )}
        </div>
        <footer>
          <button disabled={isInviting} onClick={onCancel} type="button">取消</button>
          <button
            className="primary-button"
            disabled={isInviting || !agentId}
            onClick={() => void onInvite(agentId)}
            type="button"
          >{isInviting ? "正在邀请…" : "邀请"}</button>
        </footer>
      </section>
    </div>
  );
}

export function CreateAgentTeamSheet({
  error,
  existingTeams,
  isCreating,
  projects,
  onCancel,
  onCreate,
}: {
  error: string | null;
  existingTeams: AgentTeamSummary[];
  isCreating: boolean;
  projects: Array<{ id: string; title: string }>;
  onCancel(): void;
  onCreate(input: CreateAgentTeamInput): Promise<void>;
}) {
  const availableProjects = useMemo(() => {
    const boundProjects = new Set(existingTeams.map((team) => team.projectId));
    return projects.filter((project) => !boundProjects.has(project.id));
  }, [existingTeams, projects]);
  const [projectId, setProjectId] = useState(availableProjects[0]?.id ?? "");
  const [name, setName] = useState("");
  const [goal, setGoal] = useState("");

  useEffect(() => {
    if (!availableProjects.some((project) => project.id === projectId)) {
      setProjectId(availableProjects[0]?.id ?? "");
    }
  }, [availableProjects, projectId]);

  const projectName = availableProjects.find((project) => project.id === projectId)?.title ?? "项目";
  const resolvedName = name.trim() || `${projectName} Agent 团队`;
  return (
    <div className="agent-editor-backdrop" role="presentation">
      <section aria-label="创建 Agent 团队" aria-modal="true" className="agent-editor agent-team-creator" role="dialog">
        <header>
          <h2>创建 Agent 团队</h2>
          <button aria-label="关闭创建 Agent 团队" disabled={isCreating} onClick={onCancel} type="button">×</button>
        </header>
        <div className="agent-editor__body">
          <p className="agent-team-creator__intro">选择一个项目。只有主动创建团队的项目才会进入 Agent 团队空间。</p>
          {error && <div className="agent-editor__error" role="alert">{error}</div>}
          {availableProjects.length === 0 ? (
            <div className="agent-team-creator__empty">所有现有项目都已经建立团队，或当前还没有项目。</div>
          ) : <>
            <label>绑定项目
              <select aria-label="绑定项目" onChange={(event) => setProjectId(event.target.value)} value={projectId}>
                {availableProjects.map((project) => <option key={project.id} value={project.id}>{project.title}</option>)}
              </select>
            </label>
            <label>团队名称
              <input aria-label="团队名称" maxLength={160} onChange={(event) => setName(event.target.value)} placeholder={resolvedName} value={name} />
            </label>
            <label>团队目标（可选）
              <textarea aria-label="团队目标（可选）" maxLength={8_000} onChange={(event) => setGoal(event.target.value)} rows={4} value={goal} />
            </label>
            <small>团队创建后，再按 macOS 规则邀请 Agent、指定项目经理和配置团队职责。</small>
          </>}
        </div>
        <footer>
          <button disabled={isCreating} onClick={onCancel} type="button">取消</button>
          <button
            className="primary-button"
            disabled={isCreating || !projectId}
            onClick={() => void onCreate({ projectId, name: resolvedName, goal: goal.trim() })}
            type="button"
          >{isCreating ? "正在创建…" : "创建团队"}</button>
        </footer>
      </section>
    </div>
  );
}
