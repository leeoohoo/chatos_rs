import React from 'react';

import type { ToolFamily } from '../../lib/tools/catalog';

export const ToolFamilyIcon: React.FC<{ family: ToolFamily }> = ({ family }) => {
  if (family === 'browser') {
    return (
      <svg className="tool-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
        <rect x="3" y="4" width="18" height="14" rx="2" />
        <path d="M8 20h8" />
        <path d="M12 18v2" />
      </svg>
    );
  }
  if (family === 'web') {
    return (
      <svg className="tool-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
        <circle cx="12" cy="12" r="9" />
        <path d="M3 12h18" />
        <path d="M12 3a14.5 14.5 0 0 1 0 18" />
        <path d="M12 3a14.5 14.5 0 0 0 0 18" />
      </svg>
    );
  }
  if (family === 'code') {
    return (
      <svg className="tool-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
        <path d="m8 9-3 3 3 3" />
        <path d="m16 9 3 3-3 3" />
        <path d="m14 4-4 16" />
      </svg>
    );
  }
  if (family === 'process') {
    return (
      <svg className="tool-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
        <path d="M4 4h16v10H4z" />
        <path d="M8 20h8" />
        <path d="M12 14v6" />
      </svg>
    );
  }
  if (family === 'remote') {
    return (
      <svg className="tool-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
        <path d="M5 12h14" />
        <path d="m13 6 6 6-6 6" />
        <path d="M11 6H7a3 3 0 0 0-3 3v6a3 3 0 0 0 3 3h4" />
      </svg>
    );
  }
  if (family === 'notepad') {
    return (
      <svg className="tool-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
        <path d="M8 2v4" />
        <path d="M16 2v4" />
        <rect x="4" y="4" width="16" height="18" rx="2" />
        <path d="M8 10h8" />
        <path d="M8 14h8" />
      </svg>
    );
  }
  if (family === 'task') {
    return (
      <svg className="tool-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
        <path d="M9 11l3 3L22 4" />
        <path d="M21 12v7a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h11" />
      </svg>
    );
  }
  if (family === 'agent') {
    return (
      <svg className="tool-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
        <rect x="7" y="11" width="10" height="8" rx="2" />
        <path d="M12 2v4" />
        <path d="M9 7h6" />
        <circle cx="10" cy="15" r="1" />
        <circle cx="14" cy="15" r="1" />
      </svg>
    );
  }
  if (family === 'memory') {
    return (
      <svg className="tool-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
        <path d="M6 4h8a4 4 0 0 1 4 4v12H6a2 2 0 0 0-2 2V6a2 2 0 0 1 2-2z" />
        <path d="M18 20a2 2 0 0 0-2-2H4" />
      </svg>
    );
  }
  return (
    <svg className="tool-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
      <path d="M13 2 3 14h7l-1 8 12-14h-7l1-6z" />
    </svg>
  );
};

export const SectionIcon: React.FC<{ kind: 'input' | 'result' | 'stream' | 'error' | 'meta' }> = ({ kind }) => {
  if (kind === 'input') {
    return (
      <svg className="tool-section-icon-svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
        <path d="M14 5h6v6" />
        <path d="M10 19H4v-6" />
        <path d="M20 5 9 16" />
      </svg>
    );
  }
  if (kind === 'result') {
    return (
      <svg className="tool-section-icon-svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
        <path d="M20 6 9 17l-5-5" />
      </svg>
    );
  }
  if (kind === 'stream') {
    return (
      <svg className="tool-section-icon-svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
        <path d="M5 12h14" />
        <path d="m13 6 6 6-6 6" />
      </svg>
    );
  }
  if (kind === 'error') {
    return (
      <svg className="tool-section-icon-svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
        <circle cx="12" cy="12" r="9" />
        <path d="m15 9-6 6" />
        <path d="m9 9 6 6" />
      </svg>
    );
  }
  return (
    <svg className="tool-section-icon-svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
      <path d="M12 8v4l3 3" />
      <circle cx="12" cy="12" r="9" />
    </svg>
  );
};
