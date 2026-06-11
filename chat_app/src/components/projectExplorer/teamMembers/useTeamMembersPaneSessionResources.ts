import type { Project } from '../../../types';
import { useTeamMembersContactResources } from './useTeamMembersContactResources';
import { useTeamMembersPaneStoreBridge } from './useTeamMembersPaneStoreBridge';
import { useTeamMembersRuntimeResources } from './useTeamMembersRuntimeResources';

interface UseTeamMembersPaneSessionResourcesOptions {
  project: Project;
  store: ReturnType<typeof useTeamMembersPaneStoreBridge>;
}

export const useTeamMembersPaneSessionResources = ({
  project,
  store,
}: UseTeamMembersPaneSessionResourcesOptions) => {
  const contactResources = useTeamMembersContactResources({ project, store });
  const runtimeResources = useTeamMembersRuntimeResources({
    store,
    contacts: contactResources,
  });

  return {
    members: {
      ...contactResources.members,
      handleRemoveMember: runtimeResources.handleRemoveMember,
    },
    isTaskRunnerAsyncContactMode: runtimeResources.isTaskRunnerAsyncContactMode,
    conversation: contactResources.conversation,
    summary: contactResources.summary,
    composer: runtimeResources.composer,
    workbar: runtimeResources.workbar,
    runtimeContext: runtimeResources.runtimeContext,
  };
};
