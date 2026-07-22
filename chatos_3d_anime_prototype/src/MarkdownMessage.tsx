import { Check, Copy } from 'lucide-react';
import { Fragment, useState } from 'react';

const inlineParts = (text: string) => {
  const tokens = text.split(/(`[^`]+`|\*\*[^*]+\*\*|\[[^\]]+\]\([^)]+\))/g).filter(Boolean);
  return tokens.map((token, index) => {
    if (token.startsWith('`') && token.endsWith('`')) return <code key={index}>{token.slice(1, -1)}</code>;
    if (token.startsWith('**') && token.endsWith('**')) return <strong key={index}>{token.slice(2, -2)}</strong>;
    const link = token.match(/^\[([^\]]+)\]\(([^)]+)\)$/);
    if (link) return <a key={index} href={link[2]} target="_blank" rel="noreferrer">{link[1]}</a>;
    return <Fragment key={index}>{token}</Fragment>;
  });
};

function CodeBlock({ language, code }: { language: string; code: string }) {
  const [copied, setCopied] = useState(false);
  const copy = async () => {
    await navigator.clipboard.writeText(code);
    setCopied(true);
    window.setTimeout(() => setCopied(false), 1200);
  };
  return (
    <div className="message-code-block">
      <header><span>{language || 'code'}</span><button type="button" onClick={() => void copy()}>{copied ? <Check size={14} /> : <Copy size={14} />}{copied ? '已复制' : '复制'}</button></header>
      <pre><code>{code}</code></pre>
    </div>
  );
}

function TextBlock({ text }: { text: string }) {
  const lines = text.split('\n');
  return (
    <>
      {lines.map((line, index) => {
        const trimmed = line.trim();
        if (!trimmed) return <br key={index} />;
        if (trimmed.startsWith('### ')) return <h4 key={index}>{inlineParts(trimmed.slice(4))}</h4>;
        if (trimmed.startsWith('## ')) return <h3 key={index}>{inlineParts(trimmed.slice(3))}</h3>;
        if (trimmed.startsWith('# ')) return <h2 key={index}>{inlineParts(trimmed.slice(2))}</h2>;
        if (/^[-*]\s+/.test(trimmed)) return <div className="markdown-list-row" key={index}><i /> <span>{inlineParts(trimmed.replace(/^[-*]\s+/, ''))}</span></div>;
        if (/^\d+\.\s+/.test(trimmed)) return <div className="markdown-list-row is-ordered" key={index}><b>{trimmed.match(/^\d+/)?.[0]}.</b><span>{inlineParts(trimmed.replace(/^\d+\.\s+/, ''))}</span></div>;
        if (trimmed.startsWith('> ')) return <blockquote key={index}>{inlineParts(trimmed.slice(2))}</blockquote>;
        return <p key={index}>{inlineParts(line)}</p>;
      })}
    </>
  );
}

export function MarkdownMessage({ content }: { content: string }) {
  const blocks: Array<{ type: 'text' | 'code'; content: string; language?: string }> = [];
  const pattern = /```([\w-]*)\n?([\s\S]*?)```/g;
  let cursor = 0;
  let match: RegExpExecArray | null;
  while ((match = pattern.exec(content)) !== null) {
    if (match.index > cursor) blocks.push({ type: 'text', content: content.slice(cursor, match.index) });
    blocks.push({ type: 'code', language: match[1], content: match[2].replace(/\n$/, '') });
    cursor = pattern.lastIndex;
  }
  if (cursor < content.length) blocks.push({ type: 'text', content: content.slice(cursor) });
  if (blocks.length === 0) blocks.push({ type: 'text', content });

  return <div className="markdown-message">{blocks.map((block, index) => block.type === 'code'
    ? <CodeBlock key={index} language={block.language || ''} code={block.content} />
    : <TextBlock key={index} text={block.content} />)}</div>;
}
