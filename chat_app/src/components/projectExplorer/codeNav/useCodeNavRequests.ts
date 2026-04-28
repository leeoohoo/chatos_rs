import { useCallback, useState } from 'react';

import type {
  CodeNavLocation,
  CodeNavLocationsResult,
} from '../../../types';
import {
  buildCodeNavLocationId,
  normalizeCodeNavLocationsResult,
} from '../../../lib/domain/codeNav';
import type {
  NavRequestKind,
  ProjectExplorerCodeNavApiClient,
} from './codeNavTypes';

interface UseCodeNavRequestsOptions {
  client: ProjectExplorerCodeNavApiClient;
  projectRootPath?: string | null;
  selectedFilePath?: string | null;
  selectedToken: string | null;
  selectedTokenLine: number | null;
  selectedTokenColumn: number | null;
  openLocation: (location: CodeNavLocation) => Promise<void>;
}

export const useCodeNavRequests = ({
  client,
  projectRootPath,
  selectedFilePath,
  selectedToken,
  selectedTokenLine,
  selectedTokenColumn,
  openLocation,
}: UseCodeNavRequestsOptions) => {
  const [navResult, setNavResult] = useState<CodeNavLocationsResult | null>(null);
  const [navRequestKind, setNavRequestKind] = useState<NavRequestKind | null>(null);
  const [navLoading, setNavLoading] = useState(false);
  const [navError, setNavError] = useState<string | null>(null);
  const [activeNavLocationId, setActiveNavLocationId] = useState<string | null>(null);

  const clearNavState = useCallback(() => {
    setNavResult(null);
    setNavRequestKind(null);
    setNavError(null);
    setActiveNavLocationId(null);
  }, []);

  const runNavRequest = useCallback(async (mode: NavRequestKind) => {
    if (!projectRootPath || !selectedFilePath || !selectedToken || !selectedTokenLine || !selectedTokenColumn) {
      setNavError('请先在代码中选中一个 token');
      return;
    }

    setNavLoading(true);
    setNavRequestKind(mode);
    setNavError(null);
    setActiveNavLocationId(null);
    try {
      const response = mode === 'definition'
        ? await client.getCodeNavDefinition({
          projectRoot: projectRootPath,
          filePath: selectedFilePath,
          line: selectedTokenLine,
          column: selectedTokenColumn,
        })
        : await client.getCodeNavReferences({
          projectRoot: projectRootPath,
          filePath: selectedFilePath,
          line: selectedTokenLine,
          column: selectedTokenColumn,
        });
      const normalized = normalizeCodeNavLocationsResult(response);
      setNavResult(normalized);
      if (normalized.locations.length === 0) {
        setNavError(mode === 'definition' ? '没有找到可跳转定义' : '没有找到引用结果');
      }
    } catch (error) {
      setNavResult(null);
      setNavError(error instanceof Error ? error.message : '代码导航失败');
    } finally {
      setNavLoading(false);
    }
  }, [
    client,
    projectRootPath,
    selectedFilePath,
    selectedToken,
    selectedTokenColumn,
    selectedTokenLine,
  ]);

  const requestDefinition = useCallback(async () => {
    await runNavRequest('definition');
  }, [runNavRequest]);

  const requestReferences = useCallback(async () => {
    await runNavRequest('references');
  }, [runNavRequest]);

  const handleOpenNavLocation = useCallback(async (location: CodeNavLocation) => {
    await openLocation(location);
    setActiveNavLocationId(buildCodeNavLocationId(location));
  }, [openLocation]);

  return {
    navResult,
    navRequestKind,
    navLoading,
    navError,
    activeNavLocationId,
    clearNavState,
    requestDefinition,
    requestReferences,
    handleOpenNavLocation,
  };
};
