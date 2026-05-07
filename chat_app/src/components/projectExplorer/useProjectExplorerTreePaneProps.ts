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
