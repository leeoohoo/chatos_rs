// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { Fragment } from 'react';
import type { CSSProperties, ReactNode } from 'react';
import { Tag, Typography } from 'antd';

type MarkdownPreviewBlock =
  | { type: 'heading'; level: 1 | 2 | 3 | 4; text: string }
  | { type: 'paragraph'; text: string }
  | { type: 'ul' | 'ol'; items: string[] }
  | { type: 'blockquote'; text: string }
  | { type: 'code'; language?: string; text: string };

interface MarkdownPreviewProps {
  value?: string | null;
  emptyText?: string;
}

interface MarkdownPreviewSectionProps extends MarkdownPreviewProps {
  title: string;
}

export function MarkdownPreviewSection({ title, value, emptyText }: MarkdownPreviewSectionProps) {
  return (
    <section style={markdownSectionStyle}>
      <div style={markdownSectionHeaderStyle}>
        <Typography.Title level={4} style={{ margin: 0 }}>
          {title}
        </Typography.Title>
        <Tag color="blue">Markdown</Tag>
      </div>
      <MarkdownPreview value={value} emptyText={emptyText} />
    </section>
  );
}

export function MarkdownPreview({ value, emptyText = '暂无内容' }: MarkdownPreviewProps) {
  const text = value?.trim();
  if (!text) {
    return (
      <div style={markdownEmptyStyle}>
        <Typography.Text type="secondary">{emptyText}</Typography.Text>
      </div>
    );
  }

  const blocks = parseMarkdownBlocks(text);
  return (
    <div style={markdownPreviewStyle}>
      {blocks.map((block, index) => renderMarkdownBlock(block, index))}
    </div>
  );
}

function parseMarkdownBlocks(text: string): MarkdownPreviewBlock[] {
  const blocks: MarkdownPreviewBlock[] = [];
  const lines = text.replace(/\r\n/g, '\n').split('\n');
  let paragraphLines: string[] = [];
  let listType: 'ul' | 'ol' | null = null;
  let listItems: string[] = [];
  let quoteLines: string[] = [];
  let inCode = false;
  let codeLanguage = '';
  let codeLines: string[] = [];

  const flushParagraph = () => {
    if (paragraphLines.length > 0) {
      blocks.push({ type: 'paragraph', text: paragraphLines.join('\n').trim() });
      paragraphLines = [];
    }
  };
  const flushList = () => {
    if (listType && listItems.length > 0) {
      blocks.push({ type: listType, items: listItems });
      listType = null;
      listItems = [];
    }
  };
  const flushQuote = () => {
    if (quoteLines.length > 0) {
      blocks.push({ type: 'blockquote', text: quoteLines.join('\n').trim() });
      quoteLines = [];
    }
  };
  const flushTextBlocks = () => {
    flushParagraph();
    flushList();
    flushQuote();
  };

  for (const line of lines) {
    const fenceMatch = line.match(/^\s*```(.*)$/);
    if (fenceMatch) {
      if (inCode) {
        blocks.push({ type: 'code', language: codeLanguage || undefined, text: codeLines.join('\n') });
        inCode = false;
        codeLanguage = '';
        codeLines = [];
      } else {
        flushTextBlocks();
        inCode = true;
        codeLanguage = fenceMatch[1].trim();
      }
      continue;
    }

    if (inCode) {
      codeLines.push(line);
      continue;
    }

    if (!line.trim()) {
      flushTextBlocks();
      continue;
    }

    const headingMatch = line.match(/^(#{1,4})\s+(.+)$/);
    if (headingMatch) {
      flushTextBlocks();
      blocks.push({
        type: 'heading',
        level: headingMatch[1].length as 1 | 2 | 3 | 4,
        text: headingMatch[2].trim(),
      });
      continue;
    }

    const unorderedMatch = line.match(/^\s*[-*+]\s+(.+)$/);
    if (unorderedMatch) {
      flushParagraph();
      flushQuote();
      if (listType !== 'ul') {
        flushList();
        listType = 'ul';
      }
      listItems.push(unorderedMatch[1].trim());
      continue;
    }

    const orderedMatch = line.match(/^\s*\d+[.)]\s+(.+)$/);
    if (orderedMatch) {
      flushParagraph();
      flushQuote();
      if (listType !== 'ol') {
        flushList();
        listType = 'ol';
      }
      listItems.push(orderedMatch[1].trim());
      continue;
    }

    const quoteMatch = line.match(/^\s*>\s?(.*)$/);
    if (quoteMatch) {
      flushParagraph();
      flushList();
      quoteLines.push(quoteMatch[1]);
      continue;
    }

    flushList();
    flushQuote();
    paragraphLines.push(line);
  }

  if (inCode) {
    blocks.push({ type: 'code', language: codeLanguage || undefined, text: codeLines.join('\n') });
  }
  flushTextBlocks();
  return blocks;
}

function renderMarkdownBlock(block: MarkdownPreviewBlock, index: number) {
  if (block.type === 'heading') {
    const level = Math.min(block.level + 2, 5) as 3 | 4 | 5;
    return (
      <Typography.Title key={index} level={level} style={markdownHeadingStyle}>
        {renderInlineMarkdown(block.text)}
      </Typography.Title>
    );
  }

  if (block.type === 'paragraph') {
    return (
      <Typography.Paragraph key={index} style={markdownParagraphStyle}>
        {renderInlineMarkdown(block.text)}
      </Typography.Paragraph>
    );
  }

  if (block.type === 'ul') {
    return (
      <ul key={index} style={markdownListStyle}>
        {block.items.map((item, itemIndex) => (
          <li key={itemIndex}>{renderInlineMarkdown(item)}</li>
        ))}
      </ul>
    );
  }

  if (block.type === 'ol') {
    return (
      <ol key={index} style={markdownListStyle}>
        {block.items.map((item, itemIndex) => (
          <li key={itemIndex}>{renderInlineMarkdown(item)}</li>
        ))}
      </ol>
    );
  }

  if (block.type === 'blockquote') {
    return (
      <blockquote key={index} style={markdownBlockquoteStyle}>
        {block.text.split('\n').map((line, lineIndex) => (
          <Fragment key={lineIndex}>
            {lineIndex > 0 ? <br /> : null}
            {renderInlineMarkdown(line)}
          </Fragment>
        ))}
      </blockquote>
    );
  }

  if (block.type === 'code') {
    return (
      <pre key={index} style={markdownCodeBlockStyle}>
        {block.language ? <div style={markdownCodeLanguageStyle}>{block.language}</div> : null}
        <code>{block.text}</code>
      </pre>
    );
  }

  return null;
}

function renderInlineMarkdown(text: string): ReactNode[] {
  const nodes: ReactNode[] = [];
  const pattern = /(`[^`]+`|\*\*[^*]+\*\*|\[[^\]]+\]\(https?:\/\/[^)\s]+\))/g;
  let lastIndex = 0;
  let match: RegExpExecArray | null;

  while ((match = pattern.exec(text)) !== null) {
    if (match.index > lastIndex) {
      nodes.push(text.slice(lastIndex, match.index));
    }
    const token = match[0];
    const key = `${match.index}-${token.length}`;
    const linkMatch = token.match(/^\[([^\]]+)\]\((https?:\/\/[^)\s]+)\)$/);
    if (token.startsWith('`')) {
      nodes.push(
        <Typography.Text key={key} code>
          {token.slice(1, -1)}
        </Typography.Text>,
      );
    } else if (token.startsWith('**')) {
      nodes.push(<strong key={key}>{token.slice(2, -2)}</strong>);
    } else if (linkMatch) {
      nodes.push(
        <Typography.Link key={key} href={linkMatch[2]} target="_blank" rel="noreferrer">
          {linkMatch[1]}
        </Typography.Link>,
      );
    }
    lastIndex = pattern.lastIndex;
  }

  if (lastIndex < text.length) {
    nodes.push(text.slice(lastIndex));
  }
  return nodes;
}

const markdownSectionStyle: CSSProperties = {
  background: '#fff',
  border: '1px solid #eceff3',
  borderRadius: 8,
  overflow: 'hidden',
};

const markdownSectionHeaderStyle: CSSProperties = {
  display: 'flex',
  alignItems: 'center',
  justifyContent: 'space-between',
  gap: 12,
  padding: '14px 18px',
  borderBottom: '1px solid #eef0f3',
};

const markdownPreviewStyle: CSSProperties = {
  padding: '18px 22px',
  color: '#1f2328',
  fontSize: 14,
  lineHeight: 1.75,
  overflowX: 'auto',
};

const markdownEmptyStyle: CSSProperties = {
  padding: '24px 22px',
};

const markdownHeadingStyle: CSSProperties = {
  marginTop: 18,
  marginBottom: 8,
  lineHeight: 1.35,
  letterSpacing: 0,
};

const markdownParagraphStyle: CSSProperties = {
  marginBottom: 12,
  whiteSpace: 'pre-wrap',
};

const markdownListStyle: CSSProperties = {
  marginTop: 0,
  marginBottom: 12,
  paddingLeft: 24,
};

const markdownBlockquoteStyle: CSSProperties = {
  margin: '0 0 12px',
  padding: '10px 14px',
  borderLeft: '4px solid #d6e4ff',
  background: '#f5f8ff',
  color: '#475467',
};

const markdownCodeBlockStyle: CSSProperties = {
  margin: '0 0 12px',
  padding: '14px 16px',
  borderRadius: 6,
  background: '#111827',
  color: '#f9fafb',
  overflowX: 'auto',
  fontSize: 13,
  lineHeight: 1.65,
};

const markdownCodeLanguageStyle: CSSProperties = {
  marginBottom: 8,
  color: '#9ca3af',
  fontSize: 12,
};
