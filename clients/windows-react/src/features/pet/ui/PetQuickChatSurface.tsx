import type { WorkspaceIndexSnapshot, WorkspaceResourceItem } from "../../workspace-index/domain/workspaceIndex";
import "./petQuickChat.css";

type ResourceKind = "contact" | "project";

export function PetQuickChatSurface({
  resources,
  onClose,
  onOpenTranslation,
  onOpenNotepad,
  onSelectResource,
}: {
  resources: WorkspaceIndexSnapshot;
  onClose(): void;
  onOpenTranslation(): void;
  onOpenNotepad(): void;
  onSelectResource(kind: ResourceKind, id: string): void;
}) {
  const hasResources = resources.contacts.length > 0 || resources.projects.length > 0;
  return (
    <div
      className="pet-quick-chat-backdrop"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget) onClose();
      }}
      role="presentation"
    >
      <section aria-labelledby="pet-quick-chat-title" aria-modal="true" className="pet-quick-chat" role="dialog">
        <header className="pet-quick-chat__header">
          <div className="pet-quick-chat__heading">
            <span aria-hidden="true" className="pet-quick-chat__icon">✦</span>
            <div>
              <h2 id="pet-quick-chat-title">快捷聊天</h2>
              <p>选择常用联系人或项目，快速回到会话。</p>
            </div>
          </div>
          <button aria-label="关闭快捷聊天" className="pet-quick-chat__close" onClick={onClose} type="button">×</button>
        </header>
        <div className="pet-quick-chat__body">
          <div className="pet-quick-chat__actions" aria-label="快捷功能">
            <button className="pet-quick-chat__action pet-quick-chat__action--translate" onClick={() => { onClose(); onOpenTranslation(); }} type="button">
              <span aria-hidden="true">文</span>
              <strong>快速翻译</strong>
              <small>粘贴文字或截图后翻译</small>
              <b aria-hidden="true">›</b>
            </button>
            <button className="pet-quick-chat__action pet-quick-chat__action--notepad" onClick={() => { onClose(); onOpenNotepad(); }} type="button">
              <span aria-hidden="true">▤</span>
              <strong>快速记事本</strong>
              <small>记录内容并与记事本同步</small>
              <b aria-hidden="true">›</b>
            </button>
          </div>
          <ResourceGroup
            empty={!hasResources}
            items={resources.contacts}
            kind="contact"
            onSelect={onSelectResource}
            title="联系人"
          />
          <ResourceGroup
            empty={!hasResources}
            items={resources.projects}
            kind="project"
            onSelect={onSelectResource}
            title="常用项目"
          />
          {!hasResources && <p className="pet-quick-chat__empty">还没有可用的联系人或项目。你仍可以使用上方的快速功能。</p>}
        </div>
        <footer className="pet-quick-chat__footer">快捷聊天使用当前登录账号的会话和项目权限。</footer>
      </section>
    </div>
  );
}

function ResourceGroup({
  empty,
  items,
  kind,
  onSelect,
  title,
}: {
  empty: boolean;
  items: WorkspaceResourceItem[];
  kind: ResourceKind;
  onSelect(kind: ResourceKind, id: string): void;
  title: string;
}) {
  if (empty || items.length === 0) return null;
  return (
    <section className="pet-quick-chat__group" aria-labelledby={`pet-quick-chat-${kind}-title`}>
      <h3 id={`pet-quick-chat-${kind}-title`}>{title}</h3>
      <div className="pet-quick-chat__resources">
        {items.map((item) => (
          <button className="pet-quick-chat__resource" key={item.id} onClick={() => onSelect(kind, item.id)} type="button">
            <span aria-hidden="true" className="pet-quick-chat__resource-icon">{kind === "contact" ? "◉" : "▰"}</span>
            <span className="pet-quick-chat__resource-copy">
              <strong>{item.title}</strong>
              <small>{item.subtitle ?? "最近会话"}</small>
            </span>
            <span aria-hidden="true" className="pet-quick-chat__chevron">›</span>
          </button>
        ))}
      </div>
    </section>
  );
}
