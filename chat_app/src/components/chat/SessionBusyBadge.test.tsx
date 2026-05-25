// @vitest-environment jsdom

import '@testing-library/jest-dom/vitest';
import { cleanup, render, screen } from '@testing-library/react';
import type { ComponentProps } from 'react';
import { afterEach, describe, expect, it } from 'vitest';

import { I18nProvider } from '../../i18n/I18nProvider';
import SessionBusyBadge from './SessionBusyBadge';

const renderBadge = (props: ComponentProps<typeof SessionBusyBadge>) => {
  window.localStorage.setItem('chat_ui_locale', 'en-US');
  return render(
    <I18nProvider>
      <SessionBusyBadge {...props} />
    </I18nProvider>,
  );
};

describe('SessionBusyBadge', () => {
  afterEach(() => {
    window.localStorage.removeItem('chat_ui_locale');
    cleanup();
  });

  it('renders idle by default', () => {
    renderBadge({ phase: null });
    expect(screen.getByText('空闲')).toBeInTheDocument();
  });

  it('renders reviewing label when phase is reviewing', () => {
    renderBadge({ phase: 'reviewing' });
    expect(screen.getByText('Reviewing')).toBeInTheDocument();
  });

  it('falls back to thinking when legacy busy flag is true', () => {
    renderBadge({ busy: true });
    expect(screen.getByText('Thinking')).toBeInTheDocument();
  });
});
