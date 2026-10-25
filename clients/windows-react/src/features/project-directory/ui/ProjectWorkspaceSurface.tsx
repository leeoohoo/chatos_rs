import { useEffect, useState, type ReactNode } from "react";
import type { ProjectDirectoryPort } from "../ports/ProjectDirectoryPort";
import { useProjectGitStore } from "../../project-git/application/projectGitStore";
import type { ProjectGitPort } from "../../project-git/ports/ProjectGitPort";
import { ProjectGitView } from "../../project-git/ui/ProjectGitView";
import type { ProjectRunPort } from "../../project-run/ports/ProjectRunPort";
import { ProjectRunSettingsView } from "../../project-run/ui/ProjectRunSettingsView";
import { ProjectDirectoryView } from "./ProjectDirectoryView";

type ProjectWorkspaceTab = "directory" | "messages" | "settings";

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
  const [directoryTab, setDirectoryTab] = useState<"files" | "git">("files");
  const [openFileRequest, setOpenFileRequest] = useState<{ id: number; path: string } | null>(null);
  const gitChangeCount = useProjectGitStore((state) => state.snapshot.changes.length);

  useEffect(() => {
    setTab("messages");
    setDirectoryTab("files");
    setOpenFileRequest(null);
  }, [projectId]);

  return (
    <div className="project-workspace-surface">
      <nav className="project-workspace-tabs" aria-label="项目工作面">
        <button className={tab === "directory" ? "is-selected" : ""} onClick={() => setTab("directory")} type="button">项目目录</button>
        <button
          aria-label="Git"
          className="project-workspace-tabs__compat-git"
          onClick={() => { setTab("directory"); setDirectoryTab("git"); }}
          type="button"
        >Git</button>
        <button className={tab === "messages" ? "is-selected" : ""} onClick={() => setTab("messages")} type="button">用户消息</button>
        <button className={tab === "settings" ? "is-selected" : ""} onClick={() => setTab("settings")} type="button">项目设置</button>
      </nav>
      <div className="project-workspace-content">
        {tab === "directory" ? (
          <div className="project-directory-workspace">
            <nav className="project-directory-tabs" aria-label="项目目录视图">
              <button className={directoryTab === "files" ? "is-selected" : ""} onClick={() => setDirectoryTab("files")} type="button">文件</button>
              <button aria-label="Git" className={directoryTab === "git" ? "is-selected" : ""} onClick={() => setDirectoryTab("git")} type="button">
                {directoryTab === "git" && gitChangeCount > 0 ? `Git ${gitChangeCount}` : "Git"}
              </button>
            </nav>
            {directoryTab === "files" ? (
              <ProjectDirectoryView
                onOpenFileRequestHandled={() => setOpenFileRequest(null)}
                openFileRequest={openFileRequest}
                ownerUserId={ownerUserId}
                port={directoryPort}
                projectId={projectId}
              />
            ) : (
              <ProjectGitView
                onOpenFile={(path) => {
                  setOpenFileRequest((current) => ({ id: (current?.id ?? 0) + 1, path }));
                  setDirectoryTab("files");
                }}
                ownerUserId={ownerUserId}
                port={gitPort}
                projectId={projectId}
              />
            )}
          </div>
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
