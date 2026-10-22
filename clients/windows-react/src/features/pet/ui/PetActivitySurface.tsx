import { usePetPresentation } from "../application/usePetPresentation";
import type { PetActivityKind, PetPresentation } from "../domain/petPresentation";
import type { PetPresentationPort } from "../ports/PetPresentationPort";
import "./pet.css";

export function PetActivitySurface({ port }: { port: PetPresentationPort }) {
  const presentation = usePetPresentation(port);
  const activity = presentation.primaryActivity;
  if (!activity) return <main className="pet-activity-overlay" />;
  const english = presentation.language === "en-US";
  const copy = activityCopy(presentation, english);
  return (
    <main aria-label={copy.title} className={`pet-activity-overlay pet-activity-overlay--${activity.kind}`}>
      <span aria-hidden="true">{activityIcon(activity.kind)}</span>
      <div><strong>{copy.title}</strong><p>{copy.detail}</p></div>
      {presentation.activeWorkCount > 1 && <em>{english ? `${presentation.activeWorkCount} active` : `${presentation.activeWorkCount} 项进行中`}</em>}
    </main>
  );
}

function activityCopy(presentation: PetPresentation, english: boolean) {
  const activity = presentation.primaryActivity!;
  const source = activity.source === "chat"
    ? (english ? "Conversation" : "对话")
    : (english ? "Task" : "任务");
  const detailByCode = {
    waiting_user: english ? "Waiting for your reply" : "等待你的回复",
    waiting_tool: english ? "Running a local tool" : "正在执行本地工具",
    retry_scheduled: english ? "Waiting for a local retry" : "等待本地重试",
    needs_review: english ? "Review the execution result" : "需要检查执行结果",
  } as const;
  const fallback = {
    working: english ? "AI is still working. Open ChatOS for the full progress." : "AI 仍在执行，可打开 ChatOS 查看完整过程。",
    reviewing: english ? "The run is paused and needs review." : "任务已暂停，需要检查后继续。",
    waiting_for_user: english ? "Open ChatOS to provide the requested input." : "打开 ChatOS 完成输入后即可继续。",
    succeeded: english ? "The run completed successfully." : "任务已经完成，可以查看结果。",
    failed: english ? "The run failed. Open ChatOS for details." : "任务执行失败，请打开 ChatOS 查看详情。",
    cancelled: english ? "The run was cancelled." : "任务已经取消。",
  } satisfies Record<PetActivityKind, string>;
  return {
    title: `${source} · ${titleForKind(activity.kind, english)}`,
    detail: activity.detailCode ? detailByCode[activity.detailCode] : fallback[activity.kind],
  };
}

function titleForKind(kind: PetActivityKind, english: boolean) {
  const values = {
    working: english ? "AI is working" : "AI 正在处理",
    reviewing: english ? "Needs review" : "需要检查",
    waiting_for_user: english ? "Waiting for you" : "等待你的输入",
    succeeded: english ? "Completed" : "已完成",
    failed: english ? "Failed" : "执行失败",
    cancelled: english ? "Cancelled" : "已取消",
  } satisfies Record<PetActivityKind, string>;
  return values[kind];
}

function activityIcon(kind: PetActivityKind) {
  if (kind === "succeeded") return "✓";
  if (kind === "failed") return "!";
  if (kind === "cancelled") return "×";
  if (kind === "waiting_for_user") return "…";
  if (kind === "reviewing") return "◉";
  return "✦";
}
