import { useLayoutEffect, useMemo, useRef } from "react";

export function ProjectRunLogView({
  content,
  followsTail,
  label,
  status,
  truncated,
}: {
  content: string;
  followsTail: boolean;
  label: string;
  status: string;
  truncated: boolean;
}) {
  const viewportRef = useRef<HTMLPreElement>(null);
  const gutterRef = useRef<HTMLPreElement>(null);
  const displayContent = content || "进程已经启动，暂时还没有日志输出。";
  const lineNumbers = useMemo(() => numberedLines(displayContent), [displayContent]);

  useLayoutEffect(() => {
    const viewport = viewportRef.current;
    if (!viewport || !followsTail) return;
    viewport.scrollTop = Math.max(0, viewport.scrollHeight - viewport.clientHeight);
    if (gutterRef.current) gutterRef.current.scrollTop = viewport.scrollTop;
  }, [displayContent, followsTail]);

  return (
    <div aria-label={label} className="project-run-log" role="region">
      <header><strong>实时运行日志</strong><span>{status}</span></header>
      <div className="project-run-log__frame">
        <pre aria-hidden="true" className="project-run-log__gutter" ref={gutterRef}>{lineNumbers}</pre>
        <pre
          aria-label={`${label}内容`}
          className="project-run-log__content"
          onScroll={(event) => {
            if (gutterRef.current) gutterRef.current.scrollTop = event.currentTarget.scrollTop;
          }}
          ref={viewportRef}
          tabIndex={0}
        >{displayContent}</pre>
      </div>
      {truncated && <p>日志已达到 512 KiB 上限，只保留最近输出。</p>}
    </div>
  );
}

function numberedLines(content: string) {
  const count = Math.max(1, content.split("\n").length);
  return Array.from({ length: count }, (_, index) => String(index + 1)).join("\n");
}
