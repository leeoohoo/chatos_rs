import {
  useEffect,
  useState,
  type Dispatch,
  type KeyboardEvent,
  type ReactNode,
  type SetStateAction,
} from "react";
import type { ProjectDirectoryPort } from "../ports/ProjectDirectoryPort";
import { useProjectGitStore } from "../../project-git/application/projectGitStore";
import type { ProjectGitPort } from "../../project-git/ports/ProjectGitPort";
import { ProjectGitView } from "../../project-git/ui/ProjectGitView";
import type { ProjectRunPort } from "../../project-run/ports/ProjectRunPort";
import { ProjectRunSettingsView } from "../../project-run/ui/ProjectRunSettingsView";
import { ProjectDirectoryView } from "./ProjectDirectoryView";

type ProjectWorkspaceTab = "directory" | "messages" | "settings";
type ProjectDirectoryTab = "files" | "git";

const PROJECT_WORKSPACE_TABS = ["directory", "messages", "settings"] as const;
const PROJECT_DIRECTORY_TABS = ["files", "git"] as const;

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
  const [directoryTab, setDirectoryTab] = useState<ProjectDirectoryTab>("files");
  const [openFileRequest, setOpenFileRequest] = useState<{ id: number; path: string } | null>(null);
  const gitChangeCount = useProjectGitStore((state) => state.snapshot.changes.length);

  useEffect(() => {
    setTab("messages");
    setDirectoryTab("files");
    setOpenFileRequest(null);
  }, [projectId]);

  return (
    <div className="project-workspace-surface">
      <div aria-label="项目工作面" className="project-workspace-tabs" role="tablist">
        <ProjectTab
          activeTab={tab}
          id="project-workspace-tab-directory"
          label="项目目录"
          onSelect={setTab}
          panelId="project-workspace-panel-directory"
          tab="directory"
          tabs={PROJECT_WORKSPACE_TABS}
        />
        <ProjectTab
          activeTab={tab}
          id="project-workspace-tab-messages"
          label="用户消息"
          onSelect={setTab}
          panelId="project-workspace-panel-messages"
          tab="messages"
          tabs={PROJECT_WORKSPACE_TABS}
        />
        <ProjectTab
          activeTab={tab}
          id="project-workspace-tab-settings"
          label="项目设置"
          onSelect={setTab}
          panelId="project-workspace-panel-settings"
          tab="settings"
          tabs={PROJECT_WORKSPACE_TABS}
        />
      </div>
      <div className="project-workspace-content">
        {tab === "directory" ? (
          <div
            aria-labelledby="project-workspace-tab-directory"
            className="project-directory-workspace"
            id="project-workspace-panel-directory"
            role="tabpanel"
          >
            <div aria-label="项目目录视图" className="project-directory-tabs" role="tablist">
              <ProjectTab
                activeTab={directoryTab}
                id="project-directory-tab-files"
                label="文件"
                onSelect={setDirectoryTab}
                panelId="project-directory-panel-files"
                tab="files"
                tabs={PROJECT_DIRECTORY_TABS}
              />
              <ProjectTab
                accessibleLabel="Git"
                activeTab={directoryTab}
                id="project-directory-tab-git"
                label={directoryTab === "git" && gitChangeCount > 0 ? `Git ${gitChangeCount}` : "Git"}
                onSelect={setDirectoryTab}
                panelId="project-directory-panel-git"
                tab="git"
                tabs={PROJECT_DIRECTORY_TABS}
              />
            </div>
            {directoryTab === "files" ? (
              <div aria-labelledby="project-directory-tab-files" id="project-directory-panel-files" role="tabpanel">
                <ProjectDirectoryView
                  onOpenFileRequestHandled={() => setOpenFileRequest(null)}
                  openFileRequest={openFileRequest}
                  ownerUserId={ownerUserId}
                  port={directoryPort}
                  projectId={projectId}
                />
              </div>
            ) : (
              <div aria-labelledby="project-directory-tab-git" id="project-directory-panel-git" role="tabpanel">
                <ProjectGitView
                  onOpenFile={(path) => {
                    setOpenFileRequest((current) => ({ id: (current?.id ?? 0) + 1, path }));
                    setDirectoryTab("files");
                  }}
                  ownerUserId={ownerUserId}
                  port={gitPort}
                  projectId={projectId}
                />
              </div>
            )}
          </div>
        ) : tab === "settings" ? (
          <div aria-labelledby="project-workspace-tab-settings" id="project-workspace-panel-settings" role="tabpanel">
            <ProjectRunSettingsView
              onProjectRemoved={onProjectRemoved}
              ownerUserId={ownerUserId}
              port={runPort}
              projectId={projectId}
              projectName={projectName}
            />
          </div>
        ) : (
          <div aria-labelledby="project-workspace-tab-messages" id="project-workspace-panel-messages" role="tabpanel">
            {messages}
          </div>
        )}
      </div>
    </div>
  );
}

function ProjectTab<T extends string>({
  accessibleLabel,
  activeTab,
  id,
  label,
  onSelect,
  panelId,
  tab,
  tabs,
}: {
  accessibleLabel?: string;
  activeTab: T;
  id: string;
  label: string;
  onSelect: Dispatch<SetStateAction<T>>;
  panelId: string;
  tab: T;
  tabs: readonly T[];
}) {
  const selected = activeTab === tab;
  return (
    <button
      aria-controls={panelId}
      aria-label={accessibleLabel}
      aria-selected={selected}
      className={selected ? "is-selected" : ""}
      id={id}
      onClick={() => onSelect(tab)}
      onKeyDown={(event) => moveTabSelection(event, tab, tabs, onSelect)}
      role="tab"
      tabIndex={selected ? 0 : -1}
      type="button"
    >
      {label}
    </button>
  );
}

function moveTabSelection<T extends string>(
  event: KeyboardEvent<HTMLButtonElement>,
  currentTab: T,
  tabs: readonly T[],
  onSelect: Dispatch<SetStateAction<T>>,
) {
  const currentIndex = tabs.indexOf(currentTab);
  let nextIndex: number | null = null;
  if (event.key === "ArrowLeft") nextIndex = (currentIndex - 1 + tabs.length) % tabs.length;
  if (event.key === "ArrowRight") nextIndex = (currentIndex + 1) % tabs.length;
  if (event.key === "Home") nextIndex = 0;
  if (event.key === "End") nextIndex = tabs.length - 1;
  if (nextIndex === null) return;

  event.preventDefault();
  const tabElements = Array.from(event.currentTarget.parentElement?.children ?? [])
    .filter((element): element is HTMLElement => element instanceof HTMLElement);
  onSelect(tabs[nextIndex]);
  window.requestAnimationFrame(() => tabElements[nextIndex]?.focus({ preventScroll: true }));
}
