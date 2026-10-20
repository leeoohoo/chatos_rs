import { useEffect, useState, type ReactNode } from "react";
import type { ProjectDirectoryPort } from "../ports/ProjectDirectoryPort";
import { ProjectDirectoryView } from "./ProjectDirectoryView";

type ProjectWorkspaceTab = "directory" | "messages";

export function ProjectWorkspaceSurface({
  ownerUserId,
  projectId,
  directoryPort,
  messages,
}: {
  ownerUserId: string;
  projectId: string;
  directoryPort: ProjectDirectoryPort;
  messages: ReactNode;
}) {
  const [tab, setTab] = useState<ProjectWorkspaceTab>("messages");

  useEffect(() => setTab("messages"), [projectId]);

  return (
    <div className="project-workspace-surface">
      <nav className="project-workspace-tabs" aria-label="项目工作面">
        <button className={tab === "directory" ? "is-selected" : ""} onClick={() => setTab("directory")} type="button">项目目录</button>
        <button className={tab === "messages" ? "is-selected" : ""} onClick={() => setTab("messages")} type="button">用户消息</button>
        <button aria-disabled="true" disabled title="项目设置尚未通过 React 准入" type="button">项目设置</button>
      </nav>
      <div className="project-workspace-content">
        {tab === "directory" ? (
          <ProjectDirectoryView ownerUserId={ownerUserId} port={directoryPort} projectId={projectId} />
        ) : messages}
      </div>
    </div>
  );
}
