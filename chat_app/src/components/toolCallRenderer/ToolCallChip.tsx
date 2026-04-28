import React from 'react';

import type { ToolFamily } from '../../lib/tools/catalog';
import { ToolFamilyIcon } from './ToolCallIcons';

interface ToolCallChipProps {
  toolFamily: ToolFamily;
  toolName: string;
  toolFamilyLabel: string;
  toolSourceLabel: string;
  statusText: string;
  statusClass: string;
  toolDescription: string;
  canToggleDetails: boolean;
  showDetails: boolean;
  toggleLabel: string;
  toggleTitle: string;
  onToggle: () => void;
}

export const ToolCallChip: React.FC<ToolCallChipProps> = ({
  toolFamily,
  toolName,
  toolFamilyLabel,
  toolSourceLabel,
  statusText,
  statusClass,
  toolDescription,
  canToggleDetails,
  showDetails,
  toggleLabel,
  toggleTitle,
  onToggle,
}) => {
  const chipContent = (
    <>
      <div className="tool-chip-left">
        <div className="tool-icon-shell">
          <ToolFamilyIcon family={toolFamily} />
        </div>
        <div className="tool-chip-main">
          <div className="tool-chip-topline">
            <span className="tool-family-badge">{toolFamilyLabel}</span>
            <span className="tool-source-badge">{toolSourceLabel}</span>
            <span className={`tool-status ${statusClass}`}>{statusText}</span>
          </div>
          <div className="tool-name-row">
            <div className="tool-name" title={toolName}>@{toolName}</div>
            {canToggleDetails && (
              <span className={`tool-inline-toggle ${showDetails ? 'expanded' : ''}`}>
                <span>{toggleLabel}</span>
                <svg className="tool-toggle-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
                  <polyline points="6 9 12 15 18 9" />
                </svg>
              </span>
            )}
          </div>
          {(showDetails || !canToggleDetails) && (
            <div className="tool-chip-subtitle">{toolDescription}</div>
          )}
        </div>
      </div>
    </>
  );

  if (canToggleDetails) {
    return (
      <button
        type="button"
        onClick={onToggle}
        className={`tool-chip tool-chip--clickable ${showDetails ? 'expanded' : ''}`}
        aria-label={toggleTitle}
        aria-expanded={showDetails}
        title={toggleTitle}
      >
        {chipContent}
      </button>
    );
  }

  return (
    <div className={`tool-chip ${showDetails ? 'expanded' : ''}`}>
      {chipContent}
    </div>
  );
};
