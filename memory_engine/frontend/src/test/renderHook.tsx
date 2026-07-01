// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type { ReactNode } from 'react';
import { render } from '@testing-library/react';

type HookResult<T> = {
  current: T;
};

export function renderHook<T>(hook: () => T) {
  const result: HookResult<T> = {
    current: undefined as T,
  };

  function HookHost() {
    result.current = hook();
    return null;
  }

  const rendered = render(<HookHost />);

  return {
    result,
    rerender: rendered.rerender,
    unmount: rendered.unmount,
  };
}
