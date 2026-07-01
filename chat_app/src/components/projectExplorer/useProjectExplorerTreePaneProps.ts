// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { useMemo } from 'react';
import type React from 'react';

import { ProjectTreePane } from './TreePane';
import {
  buildProjectExplorerTreePaneProps,
  buildProjectExplorerTreePanePropsDeps,
} from './treePane/projectTreePanePropsBuilder';
import type { UseProjectExplorerTreePanePropsParams } from './treePane/projectTreePanePropTypes';

export const useProjectExplorerTreePaneProps = (
  params: UseProjectExplorerTreePanePropsParams,
): React.ComponentProps<typeof ProjectTreePane> => useMemo(
  () => buildProjectExplorerTreePaneProps(params),
  buildProjectExplorerTreePanePropsDeps(params),
);
