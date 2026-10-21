import { useEffect, useMemo, useState } from "react";
import { useAgentManagementStore } from "../application/agentManagementStore";
import {
  agentProfessionLabels,
  heartbeatIntervalLabel,
  type AgentProfileSummary,
} from "../domain/agentManagement";
import type { AgentManagementPort } from "../ports/AgentManagementPort";
import "./agentManagement.css";

export function AgentManagementView({
  ownerUserId,
  port,
}: {
  ownerUserId: string;
  port: AgentManagementPort;
}) {
  const profiles = useAgentManagementStore((state) => state.profiles);
  const isLoading = useAgentManagementStore((state) => state.isLoading);
  const error = useAgentManagementStore((state) => state.error);
  const activate = useAgentManagementStore((state) => state.activate);
  const refresh = useAgentManagementStore((state) => state.refresh);
  const reset = useAgentManagementStore((state) => state.reset);
  const [page, setPage] = useState(0);
  const [pageSize, setPageSize] = useState(20);
  const pageCount = Math.max(1, Math.ceil(profiles.length / pageSize));
  const visibleProfiles = useMemo(
    () => profiles.slice(page * pageSize, page * pageSize + pageSize),
    [page, pageSize, profiles],
  );

  useEffect(() => {
    void activate(port, ownerUserId);
    return reset;
  }, [activate, ownerUserId, port, reset]);

  useEffect(() => {
    setPage((current) => Math.min(current, pageCount - 1));
  }, [pageCount]);

  return (
    <div className="agent-management" aria-label="Agent 管理">
      <section className="agent-management__panel">
        <header className="agent-management__toolbar">
          <div>
            <h2>Agent 管理</h2>
            <p>管理 Agent、模型和权限。</p>
          </div>
          <div className="agent-management__actions">
            <span>资料只读</span>
            <button disabled title="Skill 管理将在对应原生能力完成后开放" type="button">Skill 管理</button>
            <button className="primary-button" disabled title="创建 Agent 将在写入与模型目录门禁完成后开放" type="button">＋ 创建 Agent</button>
            <button
              aria-label="刷新 Agent"
              disabled={isLoading}
              onClick={() => void refresh(port)}
              type="button"
            >↻</button>
          </div>
        </header>

        {error && (
          <div className="agent-management__error" role="alert">
            <span>{error}</span>
            <button onClick={() => void refresh(port)} type="button">重试</button>
          </div>
        )}

        {isLoading && profiles.length === 0 ? (
          <div className="agent-management__state"><span className="mini-spinner" />正在读取 Agent…</div>
        ) : profiles.length === 0 ? (
          <div className="agent-management__empty">
            <span aria-hidden="true">◎</span>
            <h2>还没有 Agent</h2>
            <p>Windows 已建立与 macOS 相同的账号隔离 Profile Store；创建能力将在下一阶段开放。</p>
          </div>
        ) : (
          <>
            <div className="agent-management__grid">
              {visibleProfiles.map((profile) => <AgentCard key={profile.id} profile={profile} />)}
            </div>
            <Pagination
              page={page}
              pageCount={pageCount}
              pageSize={pageSize}
              total={profiles.length}
              onPageChange={setPage}
              onPageSizeChange={(size) => { setPageSize(size); setPage(0); }}
            />
          </>
        )}
      </section>
    </div>
  );
}

function AgentCard({ profile }: { profile: AgentProfileSummary }) {
  return (
    <article className="agent-card">
      <header>
        <span className="agent-card__avatar" aria-hidden="true">
          {profile.hasAvatar ? "◉" : profile.name.slice(0, 1).toUpperCase()}
        </span>
        <div>
          <h3>{profile.name}</h3>
          <p>{profile.canManageStaff ? "可招募和解雇成员" : "无人员管理权限"}</p>
        </div>
      </header>
      {profile.description && <p className="agent-card__description">{profile.description}</p>}
      <div className="agent-card__actions">
        <button disabled title="私聊将在 Agent Conversation 准入后开放" type="button">私聊</button>
        <button disabled title="编辑将在 Profile mutation 准入后开放" type="button">编辑</button>
      </div>
      <dl>
        <Row label="模型" value={profile.modelConfigId} />
        <Row label="思考等级" value={profile.thinkingLevel ?? "跟随模型默认"} />
        <Row label="职业" value={agentProfessionLabels[profile.professionKey]} />
        <Row label="主动巡检" value={profile.heartbeatEnabled ? heartbeatIntervalLabel(profile.heartbeatIntervalSeconds) : "关闭"} />
      </dl>
      {(profile.canManageStaff || profile.canAccessLocalProjects) && (
        <footer>
          {profile.canManageStaff && <span>人员管理</span>}
          {profile.canAccessLocalProjects && <span>项目与团队</span>}
        </footer>
      )}
    </article>
  );
}

function Row({ label, value }: { label: string; value: string }) {
  return <div><dt>{label}</dt><dd title={value}>{value}</dd></div>;
}

function Pagination({
  page,
  pageCount,
  pageSize,
  total,
  onPageChange,
  onPageSizeChange,
}: {
  page: number;
  pageCount: number;
  pageSize: number;
  total: number;
  onPageChange(page: number): void;
  onPageSizeChange(size: number): void;
}) {
  const start = total === 0 ? 0 : page * pageSize + 1;
  const end = Math.min(total, start + pageSize - 1);
  return (
    <footer className="agent-pagination">
      <span>{start}–{end} / {total}</span>
      <div>
        <label>
          <span className="sr-only">每页</span>
          <select aria-label="每页" onChange={(event) => onPageSizeChange(Number(event.target.value))} value={pageSize}>
            {[10, 20, 50].map((size) => <option key={size} value={size}>{size} / 页</option>)}
          </select>
        </label>
        <button aria-label="上一页" disabled={page === 0} onClick={() => onPageChange(page - 1)} type="button">‹</button>
        <span>{page + 1} / {pageCount}</span>
        <button aria-label="下一页" disabled={page + 1 >= pageCount} onClick={() => onPageChange(page + 1)} type="button">›</button>
      </div>
    </footer>
  );
}
