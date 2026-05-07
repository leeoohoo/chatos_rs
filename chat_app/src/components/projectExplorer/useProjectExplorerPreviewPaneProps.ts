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
