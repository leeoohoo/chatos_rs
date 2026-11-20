import type { AgentProfileSummary } from "../domain/agentManagement";
import { SafeMarkdownPreview } from "../../notepad/ui/SafeMarkdownPreview";

export function TeamSystemMessage({ content }: { content: string }) {
  const isRich = content.includes("\n\n") || /^#{1,6}\s/u.test(content);
  return (
    <div className={`agent-team-message agent-team-message--system${isRich ? " is-rich" : ""}`}>
      <SafeMarkdownPreview markdown={content} />
    </div>
  );
}

export function TeamMessageAvatar({
  compact = false,
  human = false,
  name,
  profile,
}: {
  compact?: boolean;
  human?: boolean;
  name: string;
  profile?: AgentProfileSummary | null;
}) {
  const className = `agent-team-message-avatar${compact ? " is-compact" : ""}${human ? " is-human" : ""}`;
  if (profile?.avatarPreviewUrl) return <img alt="" className={className} src={profile.avatarPreviewUrl} />;
  return <span aria-hidden="true" className={className}>{human ? "你" : name.trim().slice(0, 1) || "A"}</span>;
}

export function teamDeliveryStatusLabel(status: "pending" | "running" | "completed" | "failed" | "cancelled") {
  return {
    pending: "等待执行",
    running: "正在执行",
    completed: "已完成",
    failed: "执行失败",
    cancelled: "已取消",
  }[status];
}
