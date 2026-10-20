import { useEffect, useState, type ReactNode } from "react";
import type { ProjectDirectoryPort } from "../ports/ProjectDirectoryPort";
import type { ProjectGitPort } from "../../project-git/ports/ProjectGitPort";
import { ProjectGitView } from "../../project-git/ui/ProjectGitView";
import type { ProjectRunPort } from "../../project-run/ports/ProjectRunPort";
import { ProjectRunSettingsView } from "../../project-run/ui/ProjectRunSettingsView";
import { ProjectDirectoryView } from "./ProjectDirectoryView";

type ProjectWorkspaceTab = "directory" | "git" | "messages" | "settings";

export function ProjectWorkspaceSurface({
  ownerUserId,
  projectId,
  projectName,
  directoryPort,
  gitPort,
  runPort,
  messages,
  onProjectRemoved,
}: {
  ownerUserId: string;
  projectId: string;
  projectName: string;
  directoryPort: ProjectDirectoryPort;
  gitPort: ProjectGitPort;
  runPort: ProjectRunPort;
  messages: ReactNode;
  onProjectRemoved(projectId: string): void;
}) {
  const [tab, setTab] = useState<ProjectWorkspaceTab>("messages");
  const [openFileRequest, setOpenFileRequest] = useState<{ id: number; path: string } | null>(null);

  useEffect(() => {
    setTab("messages");
    setOpenFileRequest(null);
  }, [projectId]);

  return (
    <div className="project-workspace-surface">
      <nav className="project-workspace-tabs" aria-label="项目工作面">
        <button className={tab === "directory" ? "is-selected" : ""} onClick={() => setTab("directory")} type="button">项目目录</button>
        <button className={tab === "git" ? "is-selected" : ""} onClick={() => setTab("git")} type="button">Git</button>
        <button className={tab === "messages" ? "is-selected" : ""} onClick={() => setTab("messages")} type="button">用户消息</button>
        <button className={tab === "settings" ? "is-selected" : ""} onClick={() => setTab("settings")} type="button">项目设置</button>
      </nav>
      <div className="project-workspace-content">
        {tab === "directory" ? (
          <ProjectDirectoryView
            onOpenFileRequestHandled={() => setOpenFileRequest(null)}
            openFileRequest={openFileRequest}
            ownerUserId={ownerUserId}
            port={directoryPort}
            projectId={projectId}
          />
        ) : tab === "git" ? (
          <ProjectGitView
            onOpenFile={(path) => {
              setOpenFileRequest((current) => ({ id: (current?.id ?? 0) + 1, path }));
              setTab("directory");
            }}
            ownerUserId={ownerUserId}
            port={gitPort}
            projectId={projectId}
          />
        ) : tab === "settings" ? (
          <ProjectRunSettingsView
            onProjectRemoved={onProjectRemoved}
            ownerUserId={ownerUserId}
            port={runPort}
            projectId={projectId}
            projectName={projectName}
          />
        ) : messages}
      </div>
    </div>
  );
}
