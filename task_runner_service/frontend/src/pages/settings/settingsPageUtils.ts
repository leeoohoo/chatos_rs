export type RuntimeSettingsFormValues = {
  task_execution_max_iterations?: number;
  execution_timeout_seconds?: number;
  tool_result_model_max_chars?: number;
  tool_results_model_total_max_chars?: number;
};

export type SettingsTabKey = 'overview' | 'external-skill' | 'plan-skill' | 'internal-prompts';

export type SettingsPromptLocale = 'zh-CN' | 'en-US';

export function errorMessage(error: unknown): string {
  if (error instanceof Error) {
    return error.message;
  }
  return String(error);
}

export function millisecondsToWholeSeconds(value: number): number {
  return Math.max(1, Math.ceil(value / 1000));
}

export function formatSecondsFromMs(value: number): string {
  const seconds = value / 1000;
  return `${Number.isInteger(seconds) ? seconds : seconds.toFixed(1)} s`;
}
