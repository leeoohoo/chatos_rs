import type { TranslateFn } from '../../i18n/I18nProvider';

const highlightCode = (code: string, _language: string): string => (
  code
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
);

export const hasMermaidFence = (content: string): boolean => /```mermaid(?:\s|$)/i.test(content);

export const buildMarkdownHtml = (content: string, isStreaming: boolean, t: TranslateFn): string => {
  const text = typeof content === 'string' ? content : '';
  if (!text) {
    return '';
  }

  let processedText = text;

  const mathBlockRegex = /\$\$([\s\S]*?)\$\$/g;
  processedText = processedText.replace(mathBlockRegex, (_match, formula) => (
    `<div class="math-block">${formula.trim()}</div>`
  ));

  const mathInlineRegex = /\$([^$]+)\$/g;
  processedText = processedText.replace(mathInlineRegex, (_match, formula) => (
    `<span class="math-inline">${formula}</span>`
  ));

  const tableRegex = /\|(.+)\|\n\|[-\s|:]+\|\n((?:\|.+\|\n?)*)/g;
  processedText = processedText.replace(tableRegex, (_match, headerRow, bodyRows) => {
    const headers = headerRow
      .split('|')
      .map((header: string) => header.trim())
      .filter((header: string) => header);
    const rows = bodyRows.trim().split('\n').map((row: string) => (
      row.split('|').map((cell: string) => cell.trim()).filter((cell: string) => cell)
    ));

    let tableHtml = '<table>';
    if (headers.length > 0) {
      tableHtml += '<thead><tr>';
      headers.forEach((header: string) => {
        tableHtml += `<th>${header}</th>`;
      });
      tableHtml += '</tr></thead>';
    }
    if (rows.length > 0) {
      tableHtml += '<tbody>';
      rows.forEach((row: string[]) => {
        if (row.length > 0) {
          tableHtml += '<tr>';
          row.forEach((cell: string) => {
            tableHtml += `<td>${cell}</td>`;
          });
          tableHtml += '</tr>';
        }
      });
      tableHtml += '</tbody>';
    }
    tableHtml += '</table>';

    if (isStreaming) {
      return `<div class="streaming-table-wrapper">
                    <div class="streaming-table-indicator">● ${t('markdown.streamingTable')}</div>
                    ${tableHtml}
                </div>`;
    }

    return tableHtml;
  });

  if (isStreaming) {
    const incompleteTableRegex = /\|(.+)\|\n\|[-\s|:]+\|(?:\n(?:\|.+\|)*)?$/;
    const incompleteMatch = processedText.match(incompleteTableRegex);
    if (incompleteMatch) {
      const [fullMatch, headerRow] = incompleteMatch;
      const headers = headerRow
        .split('|')
        .map((header: string) => header.trim())
        .filter((header: string) => header);
      const lines = fullMatch.split('\n');
      const bodyLines = lines.slice(2);
      const rows = bodyLines
        .map((row: string) => (
          row.split('|').map((cell: string) => cell.trim()).filter((cell: string) => cell)
        ))
        .filter((row: string[]) => row.length > 0);

      let streamingTableHtml = '<div class="streaming-table-wrapper">';
      streamingTableHtml += `<div class="streaming-table-indicator">● ${t('markdown.streamingTable')}</div>`;
      streamingTableHtml += '<table class="streaming-table">';
      if (headers.length > 0) {
        streamingTableHtml += '<thead><tr>';
        headers.forEach((header: string) => {
          streamingTableHtml += `<th>${header}</th>`;
        });
        streamingTableHtml += '</tr></thead>';
      }
      if (rows.length > 0) {
        streamingTableHtml += '<tbody>';
        rows.forEach((row: string[]) => {
          streamingTableHtml += '<tr>';
          row.forEach((cell: string) => {
            streamingTableHtml += `<td>${cell}</td>`;
          });
          streamingTableHtml += '</tr>';
        });
        streamingTableHtml += '</tbody>';
      }
      streamingTableHtml += '</table></div>';

      processedText = processedText.replace(incompleteTableRegex, streamingTableHtml);
    }
  }

  const codeBlockRegex = /```([\w-]+)?\n?([\s\S]*?)```/g;
  let codeBlockIndex = 0;
  processedText = processedText.replace(codeBlockRegex, (_match, language, code) => {
    const lang = language || 'text';
    const trimmedCode = code.trim();
    const normalizedLang = String(lang).toLowerCase();

    if (normalizedLang === 'mermaid') {
      const mermaidBlockId = `mermaid-block-${codeBlockIndex++}`;
      const encodedCode = encodeURIComponent(trimmedCode);

      return `<div class="mermaid-block" data-mermaid-block-id="${mermaidBlockId}">
                    <div class="mermaid-header">
                        <span class="mermaid-language">mermaid</span>
                        <div class="code-actions">
                            <button class="code-action-btn mermaid-open-btn" data-code="${encodedCode}" title="${t('markdown.mermaid.previewTitle')}">
                                ${t('markdown.mermaid.preview')}
                            </button>
                            <button class="code-action-btn copy-btn" data-code="${encodedCode}" title="${t('markdown.mermaid.copySourceTitle')}">
                                <svg class="icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
                                    <rect x="9" y="9" width="13" height="13" rx="2"></rect>
                                    <path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1"></path>
                                </svg>
                            </button>
                        </div>
                    </div>
                    <div class="mermaid-content mermaid-manual-content">
                        <pre class="mermaid-source-preview"><code>${highlightCode(trimmedCode, lang)}</code></pre>
                    </div>
                </div>`;
    }

    const highlightedCode = highlightCode(trimmedCode, lang);
    const blockId = `code-block-${codeBlockIndex++}`;

    return `<div class="code-block" data-block-id="${blockId}">
                <div class="code-header">
                    <span class="code-language">${lang}</span>
                    <div class="code-actions">
                        <button class="code-action-btn copy-btn" data-code="${encodeURIComponent(trimmedCode)}" title="${t('markdown.code.copyTitle')}">
                            <svg class="icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
                                <rect x="9" y="9" width="13" height="13" rx="2"></rect>
                                <path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1"></path>
                            </svg>
                        </button>
                        <button class="code-action-btn expand-btn" data-block-id="${blockId}" title="${t('markdown.code.expand')}">
                            <svg class="icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
                                <polyline points="6 9 12 15 18 9"></polyline>
                            </svg>
                        </button>
                    </div>
                </div>
                <div class="code-content">
                    <pre><code>${highlightedCode}</code></pre>
                </div>
            </div>`;
  });

  processedText = processedText.replace(/`([^`\n]+)`/g, '<code>$1</code>');

  if (isStreaming) {
    const unclosedCodeBlockRegex = /```([\w-]+)?\n?([\s\S]*?)$/;
    const unclosedMatch = processedText.match(unclosedCodeBlockRegex);

    if (unclosedMatch && !unclosedMatch[0].includes('```', 3)) {
      const [, language, code] = unclosedMatch;
      const lang = language || 'text';
      const highlightedCode = highlightCode(code, lang);

      processedText = processedText.replace(
        unclosedCodeBlockRegex,
        `<div class="code-block streaming-code">
                        <div class="code-header">${lang} <span class="streaming-indicator">● ${t('markdown.code.streamingInput')}</span></div>
                        <pre><code>${highlightedCode}</code></pre>
                    </div>`,
      );
    }
  }

  processedText = processedText.replace(/^### (.*$)/gm, '<h3>$1</h3>');
  processedText = processedText.replace(/^## (.*$)/gm, '<h2>$1</h2>');
  processedText = processedText.replace(/^# (.*$)/gm, '<h1>$1</h1>');

  processedText = processedText.replace(/^\* (.*$)/gm, '<li>• $1</li>');
  processedText = processedText.replace(/^\d+\. (.*$)/gm, '<li>$1</li>');

  processedText = processedText.replace(/\*\*(.*?)\*\*/g, '<strong>$1</strong>');
  processedText = processedText.replace(/\*(.*?)\*/g, '<em>$1</em>');

  processedText = processedText.replace(
    /\[([^\]]+)\]\(([^)]+)\)/g,
    '<a href="$2" target="_blank" rel="noopener noreferrer">$1</a>',
  );

  processedText = processedText.replace(/^> (.*$)/gm, '<blockquote>$1</blockquote>');
  processedText = processedText.replace(/^---$/gm, '<hr>');

  const protectedHtmlBlocks: string[] = [];
  let protectedHtmlIndex = 0;

  processedText = processedText.replace(/<[^>]+>[\s\S]*?<\/[^>]+>/g, (match) => {
    const placeholder = `__PROTECTED_HTML_${protectedHtmlIndex++}__`;
    protectedHtmlBlocks.push(match);
    return placeholder;
  });

  processedText = processedText.replace(/<[^>]+>/g, (match) => {
    const placeholder = `__PROTECTED_HTML_${protectedHtmlIndex++}__`;
    protectedHtmlBlocks.push(match);
    return placeholder;
  });

  processedText = processedText.replace(/\n\n/g, '</p><p>');
  processedText = processedText.replace(/\n/g, '<br>');

  protectedHtmlBlocks.forEach((block, index) => {
    processedText = processedText.replace(`__PROTECTED_HTML_${index}__`, block);
  });

  if (processedText && !processedText.startsWith('<')) {
    processedText = `<p>${processedText}</p>`;
  }

  return processedText;
};
