import { useEffect, type ReactNode } from "react";
import type { WorkspaceResourceItem } from "../../workspace-index/domain/workspaceIndex";
import { useProjectConversationStore } from "../application/projectConversationStore";
import type { ProjectConversationPort } from "../ports/ProjectConversationPort";

interface ProjectConversationPlaceholderProps {
  ownerUserId: string;
  project: WorkspaceResourceItem;
  hostReady: boolean;
  defaultContactReady: boolean;
  port: ProjectConversationPort;
}

export function ProjectConversationPlaceholder({
  ownerUserId,
  project,
  hostReady,
  defaultContactReady,
  port,
}: ProjectConversationPlaceholderProps) {
  const phase = useProjectConversationStore((state) => state.phase(project.id));
  const prepare = useProjectConversationStore((state) => state.prepare);

  useEffect(() => {
    void prepare(port, {
      ownerUserId,
      projectId: project.id,
      existingConversationId: project.conversationId,
      hostReady,
      defaultContactReady,
    });
  }, [defaultContactReady, hostReady, ownerUserId, port, prepare, project.conversationId, project.id]);

  if (phase.kind === "preparing") {
    return <ProjectStateCard icon={<span className="spinner" />} title="正在连接“叽咕狸”…" description="正在为项目准备首次会话，完成后即可进入消息页面。" />;
  }
  if (phase.kind === "failed") {
    return (
      <ProjectStateCard icon="!" title="会话准备失败" description={phase.message}>
        <button
          className="primary-button"
          onClick={() => void prepare(port, {
            ownerUserId,
            projectId: project.id,
            existingConversationId: project.conversationId,
            hostReady,
            defaultContactReady,
            force: true,
          })}
          type="button"
        >
          重试
        </button>
      </ProjectStateCard>
    );
  }
  if (phase.kind === "available") {
    return <ProjectStateCard icon="✓" title="项目会话已准备" description="项目绑定已按 macOS 规则完成。Conversation 时间线将在对应切片通过源码准入后接入。"><code className="conversation-identity">{phase.conversationId}</code></ProjectStateCard>;
  }
  const description = !hostReady
    ? "本地任务服务就绪后会自动继续。"
    : "正在等待默认项目联系人“叽咕狸”可用。";
  return <ProjectStateCard icon="…" title="项目还没有可用会话" description={description} />;
}

function ProjectStateCard({
  icon,
  title,
  description,
  children,
}: {
  icon: ReactNode;
  title: string;
  description: string;
  children?: ReactNode;
}) {
  return (
    <div className="project-state">
      <div className="project-state__card">
        <div className="project-state__icon" aria-hidden="true">{icon}</div>
        <h2>{title}</h2>
        <p>{description}</p>
        {children && <div className="project-state__actions">{children}</div>}
      </div>
    </div>
  );
}
