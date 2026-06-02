// @vitest-environment jsdom

import '@testing-library/jest-dom/vitest';
import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import type { ComponentProps } from 'react';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { I18nProvider } from '../../i18n/I18nProvider';
import type {
  CodeNavCapabilities,
  CodeNavDocumentSymbolsResult,
  CodeNavLocationsResult,
  FsEntry,
  FsReadResult,
  ProjectSearchHit,
} from '../../types';
import { ProjectPreviewPane } from './PreviewPane';

vi.mock('../LazyMarkdownRenderer', () => ({
  LazyMarkdownRenderer: ({ content }: { content: string }) => (
    <div data-testid="markdown-preview">{content}</div>
  ),
}));

const baseFile: FsReadResult = {
  path: '/workspace/docs/example.md',
  name: 'example.md',
  size: 128,
  contentType: 'text/markdown',
  isBinary: false,
  writable: true,
  modifiedAt: '2026-06-02T12:00:00Z',
  content: '# Title\n\nHello markdown',
};

const baseEntry: FsEntry = {
  name: 'example.md',
  path: '/workspace/docs/example.md',
  isDir: false,
  writable: true,
  size: 128,
  modifiedAt: '2026-06-02T12:00:00Z',
};

const defaultCapabilities: CodeNavCapabilities = {
  language: 'markdown',
  provider: 'test',
  supportsDefinition: true,
  supportsReferences: true,
  supportsDocumentSymbols: true,
  fallbackAvailable: false,
};

const defaultDocumentSymbols: CodeNavDocumentSymbolsResult = {
  provider: 'test',
  language: 'markdown',
  mode: 'document_symbols',
  symbols: [],
};

const renderPane = (overrides: Partial<ComponentProps<typeof ProjectPreviewPane>> = {}) => {
  const props: ComponentProps<typeof ProjectPreviewPane> = {
    selectedFile: baseFile,
    selectedPath: baseFile.path,
    selectedEntry: baseEntry,
    loadingFile: false,
    error: null,
    saveError: null,
    savingFile: false,
    searchQuery: '',
    searchCaseSensitive: false,
    searchWholeWord: false,
    searchResults: [] as ProjectSearchHit[],
    activeSearchHitId: null,
    activeSearchHitIndex: -1,
    totalSearchHits: 0,
    canOpenPreviousSearchHit: false,
    canOpenNextSearchHit: false,
    targetLine: null,
    targetLineRevision: 0,
    navCapabilities: defaultCapabilities,
    navCapabilitiesError: null,
    selectedToken: null,
    navResult: null as CodeNavLocationsResult | null,
    navRequestKind: null,
    navLoading: false,
    navError: null,
    activeNavLocationId: null,
    canGoBackFromNav: false,
    documentSymbols: defaultDocumentSymbols,
    documentSymbolsLoading: false,
    documentSymbolsError: null,
    onRequestDocumentSymbols: vi.fn(),
    onTokenSelection: vi.fn(),
    onClearTokenSelection: vi.fn(),
    onRequestDefinition: vi.fn(),
    onRequestReferences: vi.fn(),
    onGoBackFromNav: vi.fn(),
    onSearchInProject: vi.fn(),
    onOpenPreviousSearchHit: vi.fn(),
    onOpenNextSearchHit: vi.fn(),
    onActivateSearchHit: vi.fn(),
    onOpenNavLocation: vi.fn(),
    onOpenDocumentSymbol: vi.fn(),
    onSaveFile: vi.fn().mockResolvedValue(true),
    ...overrides,
  };

  window.localStorage.setItem('chat_ui_locale', 'en-US');
  return render(
    <I18nProvider>
      <ProjectPreviewPane {...props} />
    </I18nProvider>,
  );
};

describe('ProjectPreviewPane', () => {
  afterEach(() => {
    window.localStorage.removeItem('chat_ui_locale');
    cleanup();
  });

  it('renders markdown files in preview mode by default and switches to editor on demand', () => {
    renderPane();

    expect(screen.getByTestId('markdown-preview')).toHaveTextContent('# Title');
    expect(screen.queryByText('Go to definition')).not.toBeInTheDocument();
    expect(screen.queryByRole('textbox')).not.toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: 'Edit' }));

    expect(screen.queryByTestId('markdown-preview')).not.toBeInTheDocument();
    expect(screen.getByRole('textbox')).toHaveValue('# Title\n\nHello markdown');
    expect(screen.getByRole('button', { name: 'Save' })).toBeDisabled();
  });

  it('keeps code navigation actions visible for non-markdown text files', () => {
    renderPane({
      selectedFile: {
        ...baseFile,
        path: '/workspace/src/example.ts',
        name: 'example.ts',
        contentType: 'text/typescript',
        content: 'const value = 1;\n',
      },
      selectedPath: '/workspace/src/example.ts',
      selectedEntry: {
        ...baseEntry,
        path: '/workspace/src/example.ts',
        name: 'example.ts',
      },
      navCapabilities: {
        ...defaultCapabilities,
        language: 'typescript',
      },
    });

    expect(screen.getByText('Go to definition')).toBeInTheDocument();
    expect(screen.queryByTestId('markdown-preview')).not.toBeInTheDocument();
  });
});
