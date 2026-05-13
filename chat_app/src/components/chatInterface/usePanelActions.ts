import { useTaskReviewPanelActions } from './useTaskReviewPanelActions';
import { useUiPromptPanelActions } from './useUiPromptPanelActions';
import type {
  TaskReviewPanelActionsArgs,
  UiPromptPanelActionsArgs,
} from './panelActionTypes';

type UsePanelActionsArgs = TaskReviewPanelActionsArgs & UiPromptPanelActionsArgs;

export function usePanelActions(args: UsePanelActionsArgs) {
  const {
    handleTaskReviewConfirm,
    handleTaskReviewCancel,
  } = useTaskReviewPanelActions(args);

  const {
    handleUiPromptSubmit,
    handleUiPromptCancel,
  } = useUiPromptPanelActions(args);

  return {
    handleTaskReviewConfirm,
    handleTaskReviewCancel,
    handleUiPromptSubmit,
    handleUiPromptCancel,
  };
}
