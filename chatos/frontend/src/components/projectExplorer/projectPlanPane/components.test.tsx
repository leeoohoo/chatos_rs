// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

// @vitest-environment jsdom

import '@testing-library/jest-dom/vitest';
import { render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import type { ProjectRequirementDocumentResponse } from '../../../lib/api/client/types';
import { TechnicalDocumentsSection } from './components';

vi.mock('../../LazyMarkdownRenderer', () => ({
  LazyMarkdownRenderer: ({ content }: { content: string }) => (
    <div data-testid="markdown-preview">{content}</div>
  ),
}));

describe('TechnicalDocumentsSection', () => {
  it('renders svg documents as an image preview instead of markdown', () => {
    const documents: ProjectRequirementDocumentResponse[] = [
      {
        id: 'doc-svg',
        title: '多页面高保真设计总览 SVG',
        doc_type: 'ui_svg_preview',
        format: 'svg',
        content: '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 40 20"><rect width="40" height="20" fill="#f97316" /></svg>',
        version: 1,
        created_at: '2026-08-11T02:00:00Z',
        updated_at: '2026-08-11T02:30:00Z',
        requirement_id: 'req-1',
      },
    ];

    render(
      <TechnicalDocumentsSection
        documents={documents}
        loading={false}
      />,
    );

    const image = screen.getByRole('img', { name: '多页面高保真设计总览 SVG' });
    expect(image).toHaveAttribute('src');
    expect(image.getAttribute('src')).toContain('data:image/svg+xml;charset=utf-8,');
    expect(screen.queryByTestId('markdown-preview')).not.toBeInTheDocument();
  });
});
