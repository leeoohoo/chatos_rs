interface SafeMarkdownPreviewProps {
  markdown: string;
}

export function SafeMarkdownPreview({ markdown }: SafeMarkdownPreviewProps) {
  if (!markdown.trim()) {
    return <div className="notepad-preview-empty"><span aria-hidden="true">¶</span><p>开始输入 Markdown 后，可在这里预览。</p></div>;
  }
  return (
    <article aria-label="Markdown 预览" className="notepad-markdown">
      {markdown.split(/\n{2,}/u).map((block, index) => <MarkdownBlock block={block} key={index} />)}
    </article>
  );
}

function MarkdownBlock({ block }: { block: string }) {
  const value = block.trim();
  if (!value) return null;
  const heading = /^(#{1,6})\s+(.+)$/u.exec(value);
  if (heading) {
    const level = Math.min(heading[1].length, 4);
    if (level === 1) return <h1>{heading[2]}</h1>;
    if (level === 2) return <h2>{heading[2]}</h2>;
    if (level === 3) return <h3>{heading[2]}</h3>;
    return <h4>{heading[2]}</h4>;
  }
  const image = /^!\[(.*)\]\(<(data:image\/(?:png|jpeg|webp|gif);base64,[A-Za-z0-9+/=]+)>\)$/u.exec(value);
  if (image && image[2].length <= 700_000) {
    return <figure><img alt={image[1].replaceAll("\\[", "[").replaceAll("\\]", "]")} src={image[2]} /></figure>;
  }
  if (/^```/u.test(value) && /```$/u.test(value)) {
    const code = value.replace(/^```[^\n]*\n?/u, "").replace(/\n?```$/u, "");
    return <pre><code>{code}</code></pre>;
  }
  const lines = value.split("\n");
  if (lines.every((line) => /^[-*]\s+/u.test(line))) {
    return <ul>{lines.map((line, index) => <li key={index}>{line.replace(/^[-*]\s+/u, "")}</li>)}</ul>;
  }
  if (lines.every((line) => /^\d+\.\s+/u.test(line))) {
    return <ol>{lines.map((line, index) => <li key={index}>{line.replace(/^\d+\.\s+/u, "")}</li>)}</ol>;
  }
  if (lines.every((line) => /^>\s?/u.test(line))) {
    return <blockquote>{lines.map((line) => line.replace(/^>\s?/u, "")).join("\n")}</blockquote>;
  }
  return <p>{lines.map((line, index) => <span key={index}>{line}{index < lines.length - 1 && <br />}</span>)}</p>;
}
