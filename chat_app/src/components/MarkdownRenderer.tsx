import React, { useMemo, useCallback, useEffect, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import './MarkdownRenderer.css';

interface MarkdownRendererProps {
    content: string;
    isStreaming?: boolean;
    className?: string;
    onApplyCode?: (code: string, language: string) => void;
}

interface MermaidNormalizationResult {
    code: string;
    changed: boolean;
    notes: string[];
}

interface MermaidExportNotice {
    type: 'success' | 'error';
    text: string;
}

const normalizeFlowchartMermaid = (sourceCode: string): MermaidNormalizationResult => {
    let normalizedCode = sourceCode;
    const notes: string[] = [];
    const isFlowchart = /^\s*(flowchart|graph)\b/im.test(sourceCode);

    if (!isFlowchart) {
        return { code: sourceCode, changed: false, notes };
    }

    if (normalizedCode.includes('-->>')) {
        normalizedCode = normalizedCode.replace(/-->>/g, '-->');
        notes.push('flowchart edge "-->>" normalized to "-->"');
    }

    // 修复常见的 flowchart 写法：A-->B: 文本 => A -->|文本| B
    const edgeWithColonPattern = /^(\s*)(.+?)\s*-->\s*(.+?)\s*:\s*(.+?)\s*$/;
    const rewrittenLines = normalizedCode.split('\n').map((line) => {
        const trimmed = line.trim();
        if (!trimmed || trimmed.startsWith('%%')) {
            return line;
        }

        const match = line.match(edgeWithColonPattern);
        if (!match) {
            return line;
        }

        const [, indent, from, to, label] = match;
        const safeLabel = label.trim().replace(/\|/g, '\\|');
        return `${indent}${from.trim()} -->|${safeLabel}| ${to.trim()}`;
    });
    const rewrittenCode = rewrittenLines.join('\n');
    if (rewrittenCode !== normalizedCode) {
        notes.push('flowchart edge labels with ":" normalized to "|label|"');
        normalizedCode = rewrittenCode;
    }

    return {
        code: normalizedCode,
        changed: normalizedCode !== sourceCode,
        notes,
    };
};

const normalizeSequenceMermaid = (sourceCode: string): MermaidNormalizationResult => {
    const notes: string[] = [];
    const isSequenceDiagram = /^\s*sequenceDiagram\b/im.test(sourceCode);
    if (!isSequenceDiagram) {
        return { code: sourceCode, changed: false, notes };
    }

    const blockStartPattern = /^\s*(alt|opt|loop|par|critical|break|rect)\b/i;
    const lines = sourceCode.split('\n');
    const normalizedLines: string[] = [];
    let openBlocks = 0;
    let removedEndCount = 0;

    lines.forEach((line) => {
        const trimmed = line.trim();
        if (!trimmed || trimmed.startsWith('%%')) {
            normalizedLines.push(line);
            return;
        }
        if (blockStartPattern.test(trimmed)) {
            openBlocks += 1;
            normalizedLines.push(line);
            return;
        }
        if (/^end$/i.test(trimmed)) {
            if (openBlocks > 0) {
                openBlocks -= 1;
                normalizedLines.push(line);
            } else {
                removedEndCount += 1;
            }
            return;
        }
        normalizedLines.push(line);
    });

    if (removedEndCount <= 0) {
        return { code: sourceCode, changed: false, notes };
    }

    notes.push(`sequenceDiagram removed ${removedEndCount} unmatched "end" lines`);
    return {
        code: normalizedLines.join('\n'),
        changed: true,
        notes,
    };
};

const normalizeMermaidForRetry = (sourceCode: string): MermaidNormalizationResult => {
    let currentCode = sourceCode;
    let changed = false;
    const notes: string[] = [];

    const flowchartNormalized = normalizeFlowchartMermaid(currentCode);
    if (flowchartNormalized.changed) {
        currentCode = flowchartNormalized.code;
        changed = true;
        notes.push(...flowchartNormalized.notes);
    }

    const sequenceNormalized = normalizeSequenceMermaid(currentCode);
    if (sequenceNormalized.changed) {
        currentCode = sequenceNormalized.code;
        changed = true;
        notes.push(...sequenceNormalized.notes);
    }

    return {
        code: currentCode,
        changed,
        notes,
    };
};

const resolveSvgRenderSize = (svgElement: SVGSVGElement): { width: number; height: number } | null => {
    let width = Number.parseFloat(svgElement.getAttribute('width') || '');
    let height = Number.parseFloat(svgElement.getAttribute('height') || '');

    if (!(width > 0) || !(height > 0)) {
        const viewBox = svgElement.getAttribute('viewBox');
        if (viewBox) {
            const values = viewBox
                .split(/[\s,]+/)
                .map((item) => Number(item))
                .filter((item) => Number.isFinite(item));
            if (values.length === 4) {
                width = values[2];
                height = values[3];
            }
        }
    }

    if (!(width > 0) || !(height > 0)) {
        const rect = svgElement.getBoundingClientRect();
        width = rect.width;
        height = rect.height;
    }

    if (!(width > 0) || !(height > 0)) {
        return null;
    }
    return { width, height };
};

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
    const mermaidPreviewContainerRef = useRef<HTMLDivElement | null>(null);
    const mermaidExportNoticeTimerRef = useRef<number | null>(null);
    const [isMermaidPreviewOpen, setIsMermaidPreviewOpen] = useState(false);
    const [mermaidPreviewCode, setMermaidPreviewCode] = useState('');
    const [mermaidPreviewStatus, setMermaidPreviewStatus] = useState<'idle' | 'loading' | 'rendered' | 'error'>('idle');
    const [mermaidPreviewError, setMermaidPreviewError] = useState('');
    const [mermaidExportNotice, setMermaidExportNotice] = useState<MermaidExportNotice | null>(null);

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

    const showMermaidExportNotice = useCallback((type: MermaidExportNotice['type'], text: string) => {
        setMermaidExportNotice({ type, text });
        if (mermaidExportNoticeTimerRef.current !== null) {
            window.clearTimeout(mermaidExportNoticeTimerRef.current);
        }
        mermaidExportNoticeTimerRef.current = window.setTimeout(() => {
            setMermaidExportNotice(null);
            mermaidExportNoticeTimerRef.current = null;
        }, 2200);
    }, []);

    useEffect(() => {
        return () => {
            if (mermaidExportNoticeTimerRef.current !== null) {
                window.clearTimeout(mermaidExportNoticeTimerRef.current);
                mermaidExportNoticeTimerRef.current = null;
            }
        };
    }, []);

    const getMermaidPreviewSvgSnapshot = useCallback(() => {
        const previewContainer = mermaidPreviewContainerRef.current;
        const svgElement = previewContainer?.querySelector('svg');
        if (!svgElement) {
            throw new Error('Mermaid svg not found');
        }

        const size = resolveSvgRenderSize(svgElement as SVGSVGElement);
        if (!size) {
            throw new Error('Cannot resolve Mermaid svg size');
        }

        const exportSvg = (svgElement as SVGSVGElement).cloneNode(true) as SVGSVGElement;
        const width = Math.max(1, Math.ceil(size.width));
        const height = Math.max(1, Math.ceil(size.height));
        exportSvg.setAttribute('xmlns', 'http://www.w3.org/2000/svg');
        exportSvg.setAttribute('xmlns:xlink', 'http://www.w3.org/1999/xlink');
        exportSvg.setAttribute('width', `${width}`);
        exportSvg.setAttribute('height', `${height}`);
        if (!exportSvg.getAttribute('viewBox')) {
            exportSvg.setAttribute('viewBox', `0 0 ${width} ${height}`);
        }

        const serializer = new XMLSerializer();
        const svgText = serializer.serializeToString(exportSvg);
        return { svgText, width, height };
    }, []);

    const buildMermaidPreviewSvgBlob = useCallback((): Blob => {
        const { svgText } = getMermaidPreviewSvgSnapshot();
        return new Blob([svgText], { type: 'image/svg+xml;charset=utf-8' });
    }, [getMermaidPreviewSvgSnapshot]);

    const buildMermaidPreviewPngBlob = useCallback(async (): Promise<Blob> => {
        const { svgText, width, height } = getMermaidPreviewSvgSnapshot();
        const svgBlob = new Blob([svgText], { type: 'image/svg+xml;charset=utf-8' });
        const blobUrl = URL.createObjectURL(svgBlob);
        const dataUrl = `data:image/svg+xml;charset=utf-8,${encodeURIComponent(svgText)}`;

        const loadImage = (src: string) => new Promise<HTMLImageElement>((resolve, reject) => {
            const img = new Image();
            img.onload = () => resolve(img);
            img.onerror = () => reject(new Error(`Failed to load svg image from ${src.startsWith('blob:') ? 'blob' : 'data'} url`));
            img.src = src;
        });

        try {
            const image = await (async () => {
                try {
                    return await loadImage(blobUrl);
                } catch (blobLoadError) {
                    console.warn('Failed to load Mermaid svg via blob url, trying data url fallback:', blobLoadError);
                    return loadImage(dataUrl);
                }
            })();

            const scale = Math.min(2, Math.max(1, window.devicePixelRatio || 1));
            const canvas = document.createElement('canvas');
            canvas.width = Math.max(1, Math.round(width * scale));
            canvas.height = Math.max(1, Math.round(height * scale));

            const context = canvas.getContext('2d');
            if (!context) {
                throw new Error('Canvas 2d context is unavailable');
            }
            context.scale(scale, scale);
            context.fillStyle = '#ffffff';
            context.fillRect(0, 0, width, height);
            context.drawImage(image, 0, 0, width, height);

            const pngBlob = await new Promise<Blob>((resolve, reject) => {
                if (typeof canvas.toBlob === 'function') {
                    canvas.toBlob(async (blob) => {
                        if (blob) {
                            resolve(blob);
                            return;
                        }
                        try {
                            const fallbackDataUrl = canvas.toDataURL('image/png');
                            const fallbackBlob = await fetch(fallbackDataUrl).then((response) => response.blob());
                            resolve(fallbackBlob);
                        } catch (fallbackError) {
                            reject(new Error(`Failed to encode png blob: ${(fallbackError as Error).message}`));
                        }
                    }, 'image/png');
                    return;
                }
                try {
                    const fallbackDataUrl = canvas.toDataURL('image/png');
                    fetch(fallbackDataUrl)
                        .then((response) => response.blob())
                        .then(resolve)
                        .catch((error) => reject(new Error(`Failed to convert canvas data url to blob: ${(error as Error).message}`)));
                } catch (error) {
                    reject(new Error(`Failed to encode png blob: ${(error as Error).message}`));
                }
            });

            return pngBlob;
        } finally {
            URL.revokeObjectURL(blobUrl);
        }
    }, [getMermaidPreviewSvgSnapshot]);

    const downloadBlobToLocal = useCallback((blob: Blob, filename: string) => {
        const downloadUrl = URL.createObjectURL(blob);
        const link = document.createElement('a');
        link.href = downloadUrl;
        link.download = filename;
        document.body.appendChild(link);
        link.click();
        link.remove();
        URL.revokeObjectURL(downloadUrl);
    }, []);

    const copyMermaidPreviewImage = useCallback(async () => {
        if (mermaidPreviewStatus !== 'rendered') {
            showMermaidExportNotice('error', '图表尚未渲染完成，暂时无法复制图片');
            return;
        }

        try {
            const clipboardWriter = navigator.clipboard?.write?.bind(navigator.clipboard);
            const ClipboardItemCtor = (window as any).ClipboardItem;
            if (clipboardWriter && ClipboardItemCtor) {
                try {
                    const pngBlob = await buildMermaidPreviewPngBlob();
                    await clipboardWriter([new ClipboardItemCtor({ 'image/png': pngBlob })]);
                    showMermaidExportNotice('success', 'PNG 图片已复制到剪贴板');
                    return;
                } catch (pngCopyError) {
                    console.warn('Failed to copy Mermaid PNG to clipboard, trying SVG fallback:', pngCopyError);
                }

                try {
                    const svgBlob = buildMermaidPreviewSvgBlob();
                    await clipboardWriter([new ClipboardItemCtor({ 'image/svg+xml': svgBlob })]);
                    showMermaidExportNotice('success', 'SVG 图片已复制到剪贴板');
                    return;
                } catch (svgCopyError) {
                    console.warn('Failed to copy Mermaid SVG to clipboard:', svgCopyError);
                }
            }

            const textWriter = navigator.clipboard?.writeText?.bind(navigator.clipboard);
            if (!textWriter) {
                throw new Error('Clipboard text write api unavailable');
            }
            const { svgText } = getMermaidPreviewSvgSnapshot();
            await textWriter(svgText);
            showMermaidExportNotice('success', '当前环境不支持图片写入，已复制 SVG 源码');
        } catch (error) {
            console.error('Failed to copy Mermaid preview image:', error);
            showMermaidExportNotice('error', '复制失败，请先尝试下载');
        }
    }, [buildMermaidPreviewPngBlob, buildMermaidPreviewSvgBlob, getMermaidPreviewSvgSnapshot, mermaidPreviewStatus, showMermaidExportNotice]);

    const downloadMermaidPreviewImage = useCallback(async () => {
        if (mermaidPreviewStatus !== 'rendered') {
            showMermaidExportNotice('error', '图表尚未渲染完成，暂时无法下载图片');
            return;
        }

        const timestamp = new Date().toISOString().replace(/[:.]/g, '-');
        try {
            try {
                const imageBlob = await buildMermaidPreviewPngBlob();
                downloadBlobToLocal(imageBlob, `mermaid-preview-${timestamp}.png`);
                showMermaidExportNotice('success', 'PNG 图片已下载到本地');
                return;
            } catch (pngDownloadError) {
                console.warn('Failed to download Mermaid PNG, trying SVG fallback:', pngDownloadError);
            }

            const svgBlob = buildMermaidPreviewSvgBlob();
            downloadBlobToLocal(svgBlob, `mermaid-preview-${timestamp}.svg`);
            showMermaidExportNotice('success', 'PNG 导出失败，已下载 SVG');
        } catch (finalError) {
            console.error('Failed to download Mermaid preview image:', finalError);
            showMermaidExportNotice('error', '下载失败，请稍后重试');
        }
    }, [buildMermaidPreviewPngBlob, buildMermaidPreviewSvgBlob, downloadBlobToLocal, mermaidPreviewStatus, showMermaidExportNotice]);

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
                        <div class="code-actions">
                            <button class="code-action-btn mermaid-open-btn" data-code="${encodedCode}" title="预览图表">
                                预览
                            </button>
                            <button class="code-action-btn copy-btn" data-code="${encodedCode}" title="复制图表源码">
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

    const closeMermaidPreview = useCallback(() => {
        setIsMermaidPreviewOpen(false);
        setMermaidPreviewStatus('idle');
        setMermaidPreviewError('');
        setMermaidExportNotice(null);
        if (mermaidExportNoticeTimerRef.current !== null) {
            window.clearTimeout(mermaidExportNoticeTimerRef.current);
            mermaidExportNoticeTimerRef.current = null;
        }
    }, []);

    useEffect(() => {
        if (!isMermaidPreviewOpen) {
            return;
        }
        const handleKeyDown = (event: KeyboardEvent) => {
            if (event.key === 'Escape') {
                closeMermaidPreview();
            }
        };
        document.addEventListener('keydown', handleKeyDown);
        return () => {
            document.removeEventListener('keydown', handleKeyDown);
        };
    }, [closeMermaidPreview, isMermaidPreviewOpen]);

    useEffect(() => {
        if (!isMermaidPreviewOpen) {
            return;
        }
        const previousOverflow = document.body.style.overflow;
        document.body.style.overflow = 'hidden';
        return () => {
            document.body.style.overflow = previousOverflow;
        };
    }, [isMermaidPreviewOpen]);

    // 处理按钮点击事件
    const handleClick = useCallback((event: React.MouseEvent) => {
        const target = event.target as HTMLElement;
        const button = target.closest('button');
        
        if (!button) return;
        
        if (button.classList.contains('copy-btn')) {
            const code = decodeURIComponent(button.getAttribute('data-code') || '');
            copyToClipboard(code);
        } else if (button.classList.contains('mermaid-open-btn')) {
            const code = decodeURIComponent(button.getAttribute('data-code') || '');
            setMermaidPreviewCode(code);
            setMermaidPreviewStatus('idle');
            setMermaidPreviewError('');
            setMermaidExportNotice(null);
            setIsMermaidPreviewOpen(true);
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
        if (!hasMermaidBlock || isStreaming || !isMermaidPreviewOpen) {
            return;
        }
        if (!mermaidPreviewCode.trim()) {
            setMermaidPreviewStatus('error');
            setMermaidPreviewError('Mermaid 内容为空');
            return;
        }

        let cancelled = false;
        const root = markdownContainerRef.current;
        const previewContainer = mermaidPreviewContainerRef.current;
        if (!root || !previewContainer) {
            return;
        }
        previewContainer.innerHTML = '';

        const renderMermaid = async () => {
            setMermaidPreviewStatus('loading');
            setMermaidPreviewError('');
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

                const renderDiagram = async (candidateCode: string) => {
                    const parseResult = await mermaid.parse(candidateCode, { suppressErrors: true });
                    if (parseResult === false) {
                        throw new Error('Mermaid parse returned false');
                    }

                    mermaidRenderSeqRef.current += 1;
                    const renderId = `mermaid-preview-${mermaidRenderSeqRef.current}`;
                    const { svg, bindFunctions } = await mermaid.render(renderId, candidateCode);
                    if (typeof svg !== 'string' || !svg.includes('<svg')) {
                        throw new Error('Mermaid render returned invalid svg content');
                    }

                    const probe = document.createElement('div');
                    probe.innerHTML = svg;
                    const svgElement = probe.querySelector('svg');
                    if (!svgElement) {
                        throw new Error('Rendered SVG element not found');
                    }
                    const viewBox = svgElement.getAttribute('viewBox');
                    if (viewBox) {
                        const numbers = viewBox
                            .split(/\s+/)
                            .map((item) => Number(item))
                            .filter((item) => Number.isFinite(item));
                        if (numbers.length === 4) {
                            const width = numbers[2];
                            const height = numbers[3];
                            if (!(width > 0) || !(height > 0)) {
                                throw new Error(`Rendered SVG has invalid viewBox: ${viewBox}`);
                            }
                        }
                    }

                    return { svg, bindFunctions };
                };

                const diagramCode = mermaidPreviewCode;
                let rendered: { svg: string; bindFunctions?: (element: Element) => void };
                try {
                    rendered = await renderDiagram(diagramCode);
                } catch (error) {
                    const normalization = normalizeMermaidForRetry(diagramCode);
                    if (!normalization.changed) {
                        throw error;
                    }
                    rendered = await renderDiagram(normalization.code);
                    console.warn('Mermaid preview recovered by normalization', {
                        notes: normalization.notes,
                    });
                }

                if (cancelled) {
                    return;
                }
                previewContainer.innerHTML = rendered.svg;
                if (typeof rendered.bindFunctions === 'function') {
                    rendered.bindFunctions(previewContainer);
                }
                setMermaidPreviewStatus('rendered');
            } catch (error) {
                console.error('Mermaid preview failed:', {
                    error,
                    diagramCode: mermaidPreviewCode,
                });
                if (cancelled) {
                    return;
                }
                previewContainer.innerHTML = '';
                setMermaidPreviewStatus('error');
                setMermaidPreviewError('Mermaid 渲染失败，已显示源码');
            }
        };

        void renderMermaid();

        return () => {
            cancelled = true;
        };
    }, [hasMermaidBlock, isStreaming, isMermaidPreviewOpen, mermaidPreviewCode, renderedHtml]);

    const mermaidPreviewModal = isMermaidPreviewOpen && typeof document !== 'undefined'
        ? createPortal(
            <div
                className="mermaid-preview-overlay"
                onClick={(event) => {
                    if (event.target === event.currentTarget) {
                        closeMermaidPreview();
                    }
                }}
            >
                <div className="mermaid-preview-dialog" role="dialog" aria-modal="true" aria-label="Mermaid 图表预览">
                    <div className="mermaid-preview-header">
                        <span className="mermaid-preview-title">Mermaid 图表预览</span>
                        <div className="code-actions">
                            <button
                                className="code-action-btn mermaid-image-copy-btn"
                                onClick={copyMermaidPreviewImage}
                                title="复制图表为图片到剪贴板"
                                disabled={mermaidPreviewStatus !== 'rendered'}
                            >
                                复制图片
                            </button>
                            <button
                                className="code-action-btn mermaid-image-download-btn"
                                onClick={downloadMermaidPreviewImage}
                                title="下载图表为图片"
                                disabled={mermaidPreviewStatus !== 'rendered'}
                            >
                                下载图片
                            </button>
                            <button className="code-action-btn mermaid-close-btn" onClick={closeMermaidPreview} title="关闭图表弹窗">
                                关闭
                            </button>
                        </div>
                    </div>
                    <div className="mermaid-preview-body">
                        {mermaidExportNotice && (
                            <div className={`mermaid-preview-notice ${mermaidExportNotice.type}`}>
                                {mermaidExportNotice.text}
                            </div>
                        )}
                        {mermaidPreviewStatus === 'loading' && (
                            <div className="mermaid-preview-loading">● 正在渲染图表...</div>
                        )}
                        <div
                            ref={mermaidPreviewContainerRef}
                            className={`mermaid-preview-diagram ${mermaidPreviewStatus === 'error' ? 'hidden' : ''}`}
                        />
                        {mermaidPreviewStatus === 'error' && (
                            <>
                                <div className="mermaid-preview-error">{mermaidPreviewError}</div>
                                <pre className="mermaid-preview-fallback"><code>{mermaidPreviewCode}</code></pre>
                            </>
                        )}
                    </div>
                </div>
            </div>,
            document.body
        )
        : null;

    return (
        <div ref={markdownContainerRef} className={`markdown-renderer ${className}`} onClick={handleClick}>
            <div
                dangerouslySetInnerHTML={{
                    __html: renderedHtml
                }}
            />
            {mermaidPreviewModal}
            {isStreaming && (
                <span className="streaming-cursor" />
            )}
        </div>
    );
};

export default MarkdownRenderer;
