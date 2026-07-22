// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

// @vitest-environment jsdom

import '@testing-library/jest-dom/vitest';
import { cleanup, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { I18nProvider } from '../../i18n/I18nProvider';
import { ProjectContactPickerModal } from './ProjectContactPickerModal';

afterEach(() => {
  cleanup();
  window.localStorage.clear();
});

describe('ProjectContactPickerModal', () => {
  it('shows contact names without exposing internal agent identifiers', () => {
    render(
      <I18nProvider>
        <ProjectContactPickerModal
          isOpen
          projectName="FocusFlow"
          contacts={[{ id: 'contact-1', name: '规划助手' }]}
          selectedContactId={null}
          error={null}
          onClose={vi.fn()}
          onSelectedContactChange={vi.fn()}
          onConfirm={vi.fn()}
        />
      </I18nProvider>,
    );

    expect(screen.getByText('规划助手')).toBeInTheDocument();
    expect(screen.queryByText(/agent[-_ ]?id/i)).not.toBeInTheDocument();
  });
});
