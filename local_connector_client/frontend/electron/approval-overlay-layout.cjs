// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

const OVERLAY_MARGIN = 12;
const OVERLAY_TOP = 68;
const COMPACT_WIDTH = 276;
const COMPACT_HEIGHT = 68;
const EXPANDED_WIDTH = 460;
const EXPANDED_HEIGHT = 560;

function normalizeApprovalOverlayMode(mode) {
  return mode === 'compact' || mode === 'expanded' ? mode : 'hidden';
}

function approvalOverlayBounds(windowBounds, mode) {
  const normalizedMode = normalizeApprovalOverlayMode(mode);
  if (normalizedMode === 'hidden') {
    return null;
  }
  const preferredWidth = normalizedMode === 'expanded' ? EXPANDED_WIDTH : COMPACT_WIDTH;
  const preferredHeight = normalizedMode === 'expanded' ? EXPANDED_HEIGHT : COMPACT_HEIGHT;
  const windowWidth = Number(windowBounds?.width || 0);
  const windowHeight = Number(windowBounds?.height || 0);
  const y = Math.min(OVERLAY_TOP, Math.max(0, windowHeight - OVERLAY_MARGIN));
  const availableWidth = Math.max(1, windowWidth - (OVERLAY_MARGIN * 2));
  const availableHeight = Math.max(1, windowHeight - y - OVERLAY_MARGIN);
  const width = Math.min(preferredWidth, availableWidth);
  const height = Math.min(preferredHeight, availableHeight);
  return {
    x: Math.max(0, windowWidth - width - OVERLAY_MARGIN),
    y,
    width,
    height,
  };
}

module.exports = {
  approvalOverlayBounds,
  normalizeApprovalOverlayMode,
};
