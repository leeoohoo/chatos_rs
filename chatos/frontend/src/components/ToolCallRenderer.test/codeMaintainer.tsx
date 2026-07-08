// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { fireEvent, screen, within } from '@testing-library/react';
import { describe, expect, it } from 'vitest';

import {
  buildToolCall,
  renderWithEnglishI18n,
  ToolCallRenderer,
  type ToolCall,
} from './helpers';

describe('ToolCallRenderer code maintainer cards', () => {
  it('shortens code maintainer tool names and renders file details without tree tables', () => {
    renderWithEnglishI18n(
      <ToolCallRenderer
        toolCall={buildToolCall({
          name: 'code_maintainer_read_read_file_range',
          arguments: {
            path: 'src/main.rs',
            start_line: 10,
            end_line: 20,
          },
          result: {
            path: 'src/main.rs',
            size_bytes: 128,
            sha256: 'abc123',
            start_line: 10,
            end_line: 20,
            total_lines: 80,
            content: '10: fn main() {\n11:   println!("hello");\n12: }',
          },
        })}
      />,
    );

    expect(screen.getByText('@read_file_range')).toBeInTheDocument();
    expect(screen.getByTitle('read_file_range')).toBeInTheDocument();
    expect(screen.queryByText('@code_maintainer_read_read_file_range')).not.toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: 'View details' }));

    expect(screen.getByText('Input summary')).toBeInTheDocument();
    expect(screen.getAllByText('src/main.rs').length).toBeGreaterThan(0);
    expect(screen.queryByText('File summary')).not.toBeInTheDocument();
    expect(screen.getByText('File content')).toBeInTheDocument();
    const fileContentCard = screen
      .getByText('File content')
      .closest('.tool-detail-card') as HTMLElement;
    expect(fileContentCard).toBeInTheDocument();
    expect(within(fileContentCard).getByText('10-20')).toBeInTheDocument();
    const codeBlock = fileContentCard.querySelector('.tool-detail-code');
    expect(codeBlock).toHaveTextContent('println!("hello");');
    expect(screen.queryByText('字段')).not.toBeInTheDocument();
    expect(screen.queryByText('值')).not.toBeInTheDocument();
  });

  it('hides redundant search summary and keeps only the match list for search tools', () => {
    renderWithEnglishI18n(
      <ToolCallRenderer
        toolCall={buildToolCall({
          name: 'code_maintainer_read_search_text',
          arguments: {
            path: 'src',
            pattern: 'TODO',
            max_results: 200,
          },
          result: {
            count: 2,
            results: [
              { path: 'src/a.ts', line: 3, text: 'TODO first' },
              { path: 'src/b.ts', line: 8, text: 'TODO second' },
            ],
          },
        })}
      />,
    );

    fireEvent.click(screen.getByRole('button', { name: 'View details' }));

    expect(screen.getByText('@search_text')).toBeInTheDocument();
    expect(screen.getByText('Input summary')).toBeInTheDocument();
    expect(screen.queryByText('max results')).not.toBeInTheDocument();
    expect(screen.queryByText('Search summary')).not.toBeInTheDocument();
    expect(screen.getByText('Matches')).toBeInTheDocument();
    expect(screen.getByText('src/a.ts')).toBeInTheDocument();
    expect(screen.getByText('TODO second')).toBeInTheDocument();
    expect(screen.queryByText('字段')).not.toBeInTheDocument();
  });

  it('normalizes builtin list_dir names and renders directory entries cards', () => {
    renderWithEnglishI18n(
      <ToolCallRenderer
        toolCall={buildToolCall({
          name: 'code_maintainer_read_builtin__list_dir',
          arguments: {
            path: 'src',
            max_entries: 20,
          },
          result: {
            entries: [
              { name: 'components', path: 'src/components', type: 'dir' },
              { name: 'main.tsx', path: 'src/main.tsx', type: 'file', size: 1024 },
            ],
          },
        })}
      />,
    );

    expect(screen.getByText('@list_dir')).toBeInTheDocument();
    expect(screen.queryByText('@builtin__list_dir')).not.toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: 'View details' }));

    expect(screen.getByText('Directory entries')).toBeInTheDocument();
    expect(screen.getByText('components')).toBeInTheDocument();
    expect(screen.getByText('main.tsx')).toBeInTheDocument();
    expect(screen.queryByText('max entries')).not.toBeInTheDocument();
    expect(screen.queryByText('字段')).not.toBeInTheDocument();
  });

  it('normalizes builtin apply_patch names and avoids tree-table fallback', () => {
    const patchText = '*** Begin Patch\n*** Update File: src/a.ts\n@@\n-old\n+new\n*** End Patch';

    renderWithEnglishI18n(
      <ToolCallRenderer
        toolCall={buildToolCall({
          name: 'code_maintainer_write_builtin__apply_patch',
          arguments: {
            patch: patchText,
          },
          result: {
            result: {
              updated: ['src/a.ts'],
              added: [],
              deleted: [],
            },
            files: [
              { path: 'src/a.ts', sha256: 'abc123' },
            ],
            message: 'Applied patch successfully.',
          },
        })}
      />,
    );

    expect(screen.getByText('@apply_patch')).toBeInTheDocument();
    expect(screen.queryByText('@builtin__apply_patch')).not.toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: 'View details' }));

    expect(screen.queryByText('Patch')).not.toBeInTheDocument();
    expect(screen.getByText('Patch payload')).toBeInTheDocument();
    const patchCard = screen.getByText('Patch payload').closest('.tool-detail-card') as HTMLElement;
    const patchContent = patchCard.querySelector('pre');
    expect(patchContent).not.toBeNull();
    expect(patchContent).toHaveTextContent(/\*\*\* Begin Patch[\s\S]*\*\*\* End Patch/);
    expect(screen.queryByText('Patch summary')).not.toBeInTheDocument();
    expect(screen.queryByText('Change summary')).not.toBeInTheDocument();
    expect(screen.queryByText('Touched files')).not.toBeInTheDocument();
    expect(screen.queryByText('abc123')).not.toBeInTheDocument();
    expect(screen.queryByText('字段')).not.toBeInTheDocument();
  });

  it('prefers structured finalResult JSON strings for read_file_raw cards', () => {
    renderWithEnglishI18n(
      <ToolCallRenderer
        toolCall={buildToolCall({
          name: 'code_maintainer_read_read_file_raw',
          arguments: {
            path: 'src/app.ts',
          },
          result: 'streaming placeholder',
          finalResult: JSON.stringify({
            path: 'src/app.ts',
            size_bytes: 320,
            sha256: 'hash',
            line_count: 3,
            content: 'export const a = 1;\nexport const b = 2;',
          }),
        } as Partial<ToolCall> & { finalResult?: string })}
      />,
    );

    fireEvent.click(screen.getByRole('button', { name: 'View details' }));

    expect(screen.getByText('@read_file_raw')).toBeInTheDocument();
    expect(screen.getByText('Built-in panel')).toBeInTheDocument();
    expect(screen.queryByText('Structured panel')).not.toBeInTheDocument();
    const fileContentCard = screen
      .getByText('File content')
      .closest('.tool-detail-card') as HTMLElement;
    expect(fileContentCard).toBeInTheDocument();
    expect(fileContentCard).toHaveTextContent('export const a = 1;');
    expect(fileContentCard).toHaveTextContent('export const b = 2;');
    expect(screen.queryByText('streaming placeholder')).not.toBeInTheDocument();
    expect(screen.queryByText('字段')).not.toBeInTheDocument();
  });

  it('recovers truncated read_file_raw JSON-ish text into builtin file cards', () => {
    const truncatedResult = [
      'Recovered tool result snapshot:',
      '{',
      '  "path": "src/app.ts",',
      '  "size_bytes": 320,',
      '  "sha256": "hash",',
      '  "line_count": 3,',
      '  "content": "export const a = 1;\\nexport const b = 2;"',
    ].join('\n');

    renderWithEnglishI18n(
      <ToolCallRenderer
        toolCall={buildToolCall({
          name: 'code_maintainer_read_read_file_raw',
          arguments: {
            path: 'src/app.ts',
          },
          result: 'streaming placeholder',
          finalResult: truncatedResult,
        } as Partial<ToolCall> & { finalResult?: string })}
      />,
    );

    expect(screen.getByText('Built-in panel')).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: 'View details' }));

    const fileContentCard = screen
      .getByText('File content')
      .closest('.tool-detail-card') as HTMLElement;
    expect(fileContentCard).toBeInTheDocument();
    expect(fileContentCard).toHaveTextContent('export const a = 1;');
    expect(fileContentCard).toHaveTextContent('export const b = 2;');
    expect(screen.queryByText('Recovered tool result snapshot:')).not.toBeInTheDocument();
    expect(screen.queryByText('streaming placeholder')).not.toBeInTheDocument();
    expect(screen.queryByText('字段')).not.toBeInTheDocument();
  });
});
