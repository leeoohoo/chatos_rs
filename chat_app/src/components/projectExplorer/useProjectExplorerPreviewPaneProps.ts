// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { useMemo } from 'react';
import type React from 'react';

import { ProjectPreviewPane } from './PreviewPane';
import {
  buildProjectExplorerPreviewPaneProps,
  buildProjectExplorerPreviewPanePropsDeps,
} from './previewPane/projectPreviewPanePropsBuilder';
import type { UseProjectExplorerPreviewPanePropsParams } from './previewPane/projectPreviewPanePropTypes';

export const useProjectExplorerPreviewPaneProps = (
  params: UseProjectExplorerPreviewPanePropsParams,
): React.ComponentProps<typeof ProjectPreviewPane> => useMemo(
  () => buildProjectExplorerPreviewPaneProps(params),
  buildProjectExplorerPreviewPanePropsDeps(params),
);
