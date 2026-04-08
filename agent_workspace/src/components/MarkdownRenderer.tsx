import React, { useMemo, useCallback, useEffect, useRef } from 'react';
import './MarkdownRenderer.css';

interface MarkdownRendererProps {
    content: string;
    isStreaming?: boolean;
    className?: string;
    onApplyCode?: (code: string, language: string) => void;
}

export const MarkdownRenderer: React.FC<MarkdownRendererProps> = ({
    content,
    isStreaming = false,
    className = '',
    onApplyCode: _onApplyCode,
}) => {
    void _onApplyCode;
    const markdownContainerRef = useRef<HTMLDivElement | null>(null);
    const mermaidRenderSeqRef = useRef(0);
    const mermaidApiRef = useRef<any>(null);
    const mermaidThemeRef = useRef<'default' | 'dark' | ''>('');

    // 复制代码到剪贴板
    const copyToClipboard = useCallback(async (code: string) => {
        try {
            await navigator.clipboard.writeText(code);
            // 可以添加成功提示
        } catch (err) {
            console.error('Failed to copy code:', err);
        }
    }, []);

    // 简单的代码高亮函数
    const highlightCode = (code: string, _language: string): string => {
        // 保持代码的原始格式，使用统一的颜色
        // 可以在这里添加更复杂的语法高亮逻辑
        return code
            .replace(/&/g, '&amp;')
            .replace(/</g, '&lt;')
            .replace(/>/g, '&gt;');
    };

    // 增强的Markdown渲染，支持表格、代码高亮和数学公式
    const renderedHtml = useMemo(() => {
        const text = typeof content === 'string' ? content : '';
        if (!text) return '';

        let processedText = text;

        // 处理数学公式块 ($$...$$)
        const mathBlockRegex = /\$\$([\s\S]*?)\$\$/g;
        processedText = processedText.replace(mathBlockRegex, (_match, formula) => {
            return `<div class="math-block">${formula.trim()}</div>`;
        });

        // 处理内联数学公式 ($...$)
        const mathInlineRegex = /\$([^$]+)\$/g;
        processedText = processedText.replace(mathInlineRegex, (_match, formula) => {
            return `<span class="math-inline">${formula}</span>`;
        });

        // 🔥 处理表格 - 参考 chat.css 的设计理念
        const tableRegex = /\|(.+)\|\n\|[-\s|:]+\|\n((?:\|.+\|\n?)*)/g;
        processedText = processedText.replace(tableRegex, (_match, headerRow, bodyRows) => {
            const headers = headerRow.split('|').map((h: string) => h.trim()).filter((h: string) => h);
            const rows = bodyRows.trim().split('\n').map((row: string) => {
                return row.split('|').map((cell: string) => cell.trim()).filter((cell: string) => cell);
            });

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
                    <div class="streaming-table-indicator">● 正在生成表格...</div>
                    ${tableHtml}
                </div>`;
            }

            return tableHtml;
        });

        // 处理流式渲染时的未完成表格
        if (isStreaming) {
            const incompleteTableRegex = /\|(.+)\|\n\|[-\s|:]+\|(?:\n(?:\|.+\|)*)?$/;
            const incompleteMatch = processedText.match(incompleteTableRegex);
            if (incompleteMatch) {
                const [fullMatch, headerRow] = incompleteMatch;
                const headers = headerRow.split('|').map((h: string) => h.trim()).filter((h: string) => h);
                const lines = fullMatch.split('\n');
                const bodyLines = lines.slice(2);
                const rows = bodyLines.map((row: string) => {
                    return row.split('|').map((cell: string) => cell.trim()).filter((cell: string) => cell);
                }).filter((row: string[]) => row.length > 0);

                let streamingTableHtml = '<div class="streaming-table-wrapper">';
                streamingTableHtml += '<div class="streaming-table-indicator">● 正在生成表格...</div>';
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

        // 智能处理代码块，支持流式渲染时的未闭合代码块
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
                        <button class="code-action-btn copy-btn" data-code="${encodedCode}" title="复制图表源码">
                            <svg class="icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
                                <rect x="9" y="9" width="13" height="13" rx="2"></rect>
                                <path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1"></path>
                            </svg>
                        </button>
                    </div>
                    <div class="mermaid-content">
                        <div class="mermaid-loading">● 正在渲染图表...</div>
                        <div class="mermaid-diagram" data-mermaid-id="${mermaidBlockId}" data-mermaid-code="${encodedCode}"></div>
                        <pre class="mermaid-fallback"><code>${highlightCode(trimmedCode, lang)}</code></pre>
                    </div>
                </div>`;
            }

            const highlightedCode = highlightCode(trimmedCode, lang);
            const blockId = `code-block-${codeBlockIndex++}`;

            return `<div class="code-block" data-block-id="${blockId}">
                <div class="code-header">
                    <span class="code-language">${lang}</span>
                    <div class="code-actions">
                        <button class="code-action-btn copy-btn" data-code="${encodeURIComponent(trimmedCode)}" title="复制代码">
                            <svg class="icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
                                <rect x="9" y="9" width="13" height="13" rx="2"></rect>
                                <path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1"></path>
                            </svg>
                        </button>
                        <button class="code-action-btn expand-btn" data-block-id="${blockId}" title="展开">
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

        // 处理内联代码
        processedText = processedText.replace(/`([^`\n]+)`/g, '<code>$1</code>');

        // 处理流式渲染时的未闭合代码块（仅在流式模式下）
        if (isStreaming) {
            const unclosedCodeBlockRegex = /```([\w-]+)?\n?([\s\S]*?)$/;
            const unclosedMatch = processedText.match(unclosedCodeBlockRegex);

            if (unclosedMatch && !unclosedMatch[0].includes('```', 3)) {
                const [, language, code] = unclosedMatch;
                const lang = language || 'text';
                const highlightedCode = highlightCode(code, lang);

                processedText = processedText.replace(unclosedCodeBlockRegex,
                    `<div class="code-block streaming-code">
                        <div class="code-header">${lang} <span class="streaming-indicator">● 正在输入...</span></div>
                        <pre><code>${highlightedCode}</code></pre>
                    </div>`
                );
            }
        }

        // 处理标题
        processedText = processedText.replace(/^### (.*$)/gm, '<h3>$1</h3>');
        processedText = processedText.replace(/^## (.*$)/gm, '<h2>$1</h2>');
        processedText = processedText.replace(/^# (.*$)/gm, '<h1>$1</h1>');

        // 处理列表
        processedText = processedText.replace(/^\* (.*$)/gm, '<li>• $1</li>');
        processedText = processedText.replace(/^\d+\. (.*$)/gm, '<li>$1</li>');

        // 处理粗体和斜体
        processedText = processedText.replace(/\*\*(.*?)\*\*/g, '<strong>$1</strong>');
        processedText = processedText.replace(/\*(.*?)\*/g, '<em>$1</em>');

        // 处理链接
        processedText = processedText.replace(/\[([^\]]+)\]\(([^)]+)\)/g,
            '<a href="$2" target="_blank" rel="noopener noreferrer">$1</a>');

        // 处理引用
        processedText = processedText.replace(/^> (.*$)/gm, '<blockquote>$1</blockquote>');

        // 处理分隔线
        processedText = processedText.replace(/^---$/gm, '<hr>');

        // 智能处理换行 - 避免在代码块内部添加br标签
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
    }, [content, isStreaming]);

    const hasMermaidBlock = useMemo(() => /```mermaid(?:\s|$)/i.test(content), [content]);

    // 处理按钮点击事件
    const handleClick = useCallback((event: React.MouseEvent) => {
        const target = event.target as HTMLElement;
        const button = target.closest('button');
        
        if (!button) return;
        
        if (button.classList.contains('copy-btn')) {
            const code = decodeURIComponent(button.getAttribute('data-code') || '');
            copyToClipboard(code);
        } else if (button.classList.contains('expand-btn')) {
            const codeBlock = button.closest('.code-block');
            if (!codeBlock) {
                return;
            }
            const expanded = codeBlock.classList.toggle('expanded');
            button.setAttribute('title', expanded ? '收起' : '展开');
            const icon = button.querySelector('.icon');
            if (icon) {
                icon.classList.toggle('rotated', expanded);
            }
        }
    }, [copyToClipboard]);

    useEffect(() => {
        if (!hasMermaidBlock || isStreaming) {
            return;
        }

        let cancelled = false;
        const root = markdownContainerRef.current;
        if (!root) {
            return;
        }
        const mermaidNodes = Array.from(
            root.querySelectorAll<HTMLElement>('.mermaid-diagram[data-mermaid-code]')
        );
        if (mermaidNodes.length === 0) {
            return;
        }

        const renderMermaid = async () => {
            try {
                if (!mermaidApiRef.current) {
                    const mermaidModule = await import('mermaid');
                    if (cancelled) {
                        return;
                    }
                    mermaidApiRef.current = mermaidModule.default;
                }
                const mermaid = mermaidApiRef.current;
                const isDarkTheme = (
                    Boolean(root.closest('.dark'))
                    || document.documentElement.classList.contains('dark')
                    || document.body.classList.contains('dark')
                );
                const theme: 'default' | 'dark' = isDarkTheme ? 'dark' : 'default';

                if (mermaidThemeRef.current !== theme) {
                    mermaid.initialize({
                        startOnLoad: false,
                        securityLevel: 'strict',
                        theme,
                        suppressErrorRendering: true,
                    });
                    mermaidThemeRef.current = theme;
                }

                for (const node of mermaidNodes) {
                    if (cancelled) {
                        return;
                    }

                    const block = node.closest('.mermaid-block');
                    const loading = block?.querySelector<HTMLElement>('.mermaid-loading');
                    block?.classList.remove('mermaid-rendered');
                    block?.classList.remove('mermaid-error');
                    if (loading) {
                        loading.textContent = '● 正在渲染图表...';
                    }

                    const rawCode = node.getAttribute('data-mermaid-code') || '';
                    let diagramCode = '';
                    try {
                        diagramCode = decodeURIComponent(rawCode);
                    } catch {
                        diagramCode = rawCode;
                    }

                    if (!diagramCode.trim()) {
                        block?.classList.add('mermaid-error');
                        if (loading) {
                            loading.textContent = 'Mermaid 内容为空';
                        }
                        continue;
                    }

                    try {
                        mermaidRenderSeqRef.current += 1;
                        const renderId = `mermaid-diagram-${mermaidRenderSeqRef.current}`;
                        const { svg, bindFunctions } = await mermaid.render(renderId, diagramCode);
                        if (cancelled) {
                            return;
                        }
                        node.innerHTML = svg;
                        block?.classList.add('mermaid-rendered');
                        if (typeof bindFunctions === 'function') {
                            bindFunctions(node);
                        }
                    } catch (error) {
                        console.error('Mermaid render failed:', error);
                        node.innerHTML = '';
                        block?.classList.add('mermaid-error');
                        if (loading) {
                            loading.textContent = 'Mermaid 渲染失败，已显示源码';
                        }
                    }
                }
            } catch (error) {
                console.error('Failed to load mermaid:', error);
                mermaidNodes.forEach((node) => {
                    const block = node.closest('.mermaid-block');
                    const loading = block?.querySelector<HTMLElement>('.mermaid-loading');
                    block?.classList.add('mermaid-error');
                    if (loading) {
                        loading.textContent = 'Mermaid 组件加载失败，已显示源码';
                    }
                });
            }
        };

        void renderMermaid();

        return () => {
            cancelled = true;
        };
    }, [hasMermaidBlock, isStreaming, renderedHtml]);

    return (
        <div ref={markdownContainerRef} className={`markdown-renderer ${className}`} onClick={handleClick}>
            <div
                dangerouslySetInnerHTML={{
                    __html: renderedHtml
                }}
            />
            {isStreaming && (
                <span className="streaming-cursor" />
            )}
        </div>
    );
};

export default MarkdownRenderer;
