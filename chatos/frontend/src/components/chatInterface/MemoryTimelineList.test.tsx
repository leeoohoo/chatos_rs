// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team
// @vitest-environment jsdom

import '@testing-library/jest-dom/vitest';
import { cleanup, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it } from 'vitest';

import { I18nProvider } from '../../i18n/I18nProvider';
import { MemoryTimelineList } from './MemoryTimelineList';

afterEach(cleanup);

const recall = {
  id: 'recall:lc_recall_1',
  sourceId: 'lc_recall_1',
  kind: 'agent_recall' as const,
  text: 'Cloud project decision',
  time: '2026-07-15T00:00:00Z',
  sourceLabel: '项目记忆 L0',
};

const renderList = (
  items = [recall],
) => render(
  <I18nProvider>
    <MemoryTimelineList items={items} />
  </I18nProvider>,
);

describe('MemoryTimelineList', () => {
  it('renders cloud memory entries without local mutation controls', () => {
    renderList();
    expect(screen.getByText('项目记忆 L0')).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: '忘记' })).not.toBeInTheDocument();
  });
});
