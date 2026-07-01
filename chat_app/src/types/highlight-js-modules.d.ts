// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

declare module 'highlight.js/lib/core' {
  export { default } from 'highlight.js';
}

declare module 'highlight.js/lib/languages/*' {
  import type { LanguageFn } from 'highlight.js';

  const language: LanguageFn;
  export default language;
}
