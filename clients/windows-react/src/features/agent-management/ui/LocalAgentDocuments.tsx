import { useCallback, useEffect, useRef, useState } from "react";
import type { AgentArtifact } from "../domain/agentArtifact";
import type { AgentManagementPort } from "../ports/AgentManagementPort";
import { SafeMarkdownPreview } from "../../notepad/ui/SafeMarkdownPreview";

export function LocalAgentDocuments({
  ownerUserId,
  port,
  refreshSequence,
}: {
  ownerUserId: string;
  port: AgentManagementPort;
  refreshSequence: number;
}) {
  const [artifacts, setArtifacts] = useState<AgentArtifact[]>([]);
  const [nextCursor, setNextCursor] = useState<string | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const [loadingArtifactIds, setLoadingArtifactIds] = useState<Set<string>>(() => new Set());
  const [preview, setPreview] = useState<{ artifact: AgentArtifact; markdown: string } | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const loadingRef = useRef(false);

  const load = useCallback(async (cursor: string | null, reset: boolean) => {
    if (loadingRef.current) return;
    if (!port.listArtifacts) {
      setError("当前客户端未提供 Agent 文档读取能力。");
      return;
    }
    loadingRef.current = true;
    setIsLoading(true);
    setError(null);
    try {
      const page = await port.listArtifacts(ownerUserId, 50, cursor);
      setArtifacts((current) => {
        if (reset) return page.artifacts;
        const known = new Set(current.map((artifact) => artifact.artifactId));
        return [...current, ...page.artifacts.filter((artifact) => !known.has(artifact.artifactId))];
      });
      setNextCursor(page.nextCursor);
    } catch (failure) {
      setError(failure instanceof Error ? failure.message : "本地 Agent 文档读取失败。");
    } finally {
      loadingRef.current = false;
      setIsLoading(false);
    }
  }, [ownerUserId, port]);

  useEffect(() => {
    setNotice(null);
    void load(null, true);
  }, [load, refreshSequence]);

  const withArtifactLoading = async (artifact: AgentArtifact, operation: () => Promise<void>) => {
    if (loadingArtifactIds.has(artifact.artifactId)) return;
    setLoadingArtifactIds((current) => new Set(current).add(artifact.artifactId));
    setError(null);
    setNotice(null);
    try {
      await operation();
    } catch (failure) {
      setError(failure instanceof Error ? failure.message : "本地文档操作失败。");
    } finally {
      setLoadingArtifactIds((current) => {
        const next = new Set(current);
        next.delete(artifact.artifactId);
        return next;
      });
    }
  };

  const openPreview = (artifact: AgentArtifact) => void withArtifactLoading(artifact, async () => {
    if (!port.previewArtifact) throw new Error("当前客户端未提供 Agent 文档预览能力。");
    const result = await port.previewArtifact(
      ownerUserId, artifact.artifactId, artifact.size, artifact.sha256,
    );
    if (result.artifactId !== artifact.artifactId) throw new Error("Agent 文档预览身份不一致。");
    setPreview({ artifact, markdown: result.markdown });
  });

  const saveArtifact = (artifact: AgentArtifact) => void withArtifactLoading(artifact, async () => {
    if (!port.saveArtifact) throw new Error("当前客户端未提供 Agent 文档另存能力。");
    const saved = await port.saveArtifact(
      ownerUserId, artifact.artifactId, artifact.name, artifact.size, artifact.sha256,
    );
    if (saved) setNotice(`“${artifact.name}”已保存。`);
  });

  return (
    <div className="agent-local-documents" aria-label="本地 Agent 文档">
      {error && <div className="agent-local-documents__error" role="alert"><span>{error}</span><button onClick={() => void load(null, true)} type="button">重试</button></div>}
      {notice && <div className="agent-local-documents__notice" role="status">{notice}</div>}
      {isLoading && artifacts.length === 0 ? (
        <div className="agent-local-documents__loading"><span className="mini-spinner" />正在读取本地文档…</div>
      ) : artifacts.length === 0 ? (
        <div className="agent-local-documents__empty">
          <span aria-hidden="true">▤⌕</span>
          <h2>还没有本地 Agent 文档</h2>
          <p>Agent 创建并由 Local Agent Host 保存的 Markdown 会出现在这里。</p>
        </div>
      ) : (
        <div className="agent-local-documents__list" role="list">
          {artifacts.map((artifact) => {
            const loading = loadingArtifactIds.has(artifact.artifactId);
            return (
              <article className="agent-local-document" key={artifact.artifactId} role="listitem">
                <span className="agent-local-document__icon" aria-hidden="true">▤</span>
                <div className="agent-local-document__copy">
                  <strong>{artifact.name}</strong>
                  <small>{formatArtifactSize(artifact.size)} · {formatArtifactDate(artifact.createdAtUnixMs)}</small>
                  <code>SHA-256 {artifact.sha256.slice(0, 12)}…</code>
                </div>
                <div className="agent-local-document__actions">
                  {loading && <span className="mini-spinner" aria-label="正在读取文档" />}
                  <button disabled={loading} onClick={() => openPreview(artifact)} type="button">◎ 预览</button>
                  <button aria-label={`另存为 ${artifact.name}`} disabled={loading} onClick={() => saveArtifact(artifact)} title="另存为…" type="button">⇩</button>
                </div>
              </article>
            );
          })}
          {nextCursor && <button className="agent-local-documents__more" disabled={isLoading} onClick={() => void load(nextCursor, false)} type="button">{isLoading ? "正在加载…" : "加载更多"}</button>}
        </div>
      )}
      <span className="agent-local-documents__mascot" aria-hidden="true">🦊</span>
      {preview && (
        <div className="agent-artifact-preview-backdrop" role="presentation">
          <section aria-label={`${preview.artifact.name} 文档预览`} aria-modal="true" className="agent-artifact-preview" role="dialog">
            <header>
              <div><h2>{preview.artifact.name}</h2><p>{formatArtifactSize(preview.artifact.size)} · Markdown 预览</p></div>
              <button aria-label="关闭文档预览" onClick={() => setPreview(null)} type="button">×</button>
            </header>
            <SafeMarkdownPreview markdown={preview.markdown} />
          </section>
        </div>
      )}
    </div>
  );
}

function formatArtifactSize(bytes: number) {
  if (bytes < 1_024) return `${bytes} B`;
  if (bytes < 1_024 * 1_024) return `${(bytes / 1_024).toFixed(bytes < 10 * 1_024 ? 1 : 0)} KB`;
  return `${(bytes / (1_024 * 1_024)).toFixed(1)} MB`;
}

function formatArtifactDate(unixMilliseconds: number) {
  return new Intl.DateTimeFormat("zh-CN", {
    year: "numeric", month: "short", day: "numeric", hour: "2-digit", minute: "2-digit",
  }).format(new Date(unixMilliseconds));
}
