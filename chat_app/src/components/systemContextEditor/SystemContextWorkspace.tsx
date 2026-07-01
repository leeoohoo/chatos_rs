// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { useI18n } from '../../i18n/I18nProvider';
import type {
  AssistantFormState,
  PromptCandidate,
  PromptQualityReport,
  SystemContextFormData,
  ViewMode,
} from './types';

interface SystemContextWorkspaceProps {
  viewMode: ViewMode;
  selectedContextName: string;
  formData: SystemContextFormData;
  assistantForm: AssistantFormState;
  assistantError: string | null;
  qualityReport: PromptQualityReport | null;
  candidates: PromptCandidate[];
  actionError: string | null;
  onNameChange: (value: string) => void;
  onContentChange: (value: string) => void;
  onAssistantFieldChange: <K extends keyof AssistantFormState>(
    field: K,
    value: AssistantFormState[K],
  ) => void;
  onSelectCandidate: (candidate: PromptCandidate) => void;
}

export default function SystemContextWorkspace({
  viewMode,
  selectedContextName,
  formData,
  assistantForm,
  assistantError,
  qualityReport,
  candidates,
  actionError,
  onNameChange,
  onContentChange,
  onAssistantFieldChange,
  onSelectCandidate,
}: SystemContextWorkspaceProps) {
  const { t } = useI18n();

  return (
    <>
      <div className="px-6 py-4 border-b border-border">
        <div className="flex flex-wrap items-center gap-3">
          <span className="text-sm text-muted-foreground">{t('systemContext.mode.label')}</span>
          <span className="text-sm font-medium">
            {viewMode === 'create' ? t('systemContext.mode.create') : viewMode === 'edit' ? t('systemContext.mode.edit') : t('systemContext.mode.list')}
          </span>
          {selectedContextName ? (
            <span className="text-xs px-2 py-1 rounded-full bg-accent text-secondary-foreground">
              {selectedContextName}
            </span>
          ) : null}
        </div>
      </div>

      <div className="flex-1 min-h-0 overflow-y-auto px-6 py-5 space-y-4">
        <div className="grid grid-cols-1 gap-4">
          <div className="max-w-xl">
            <label className="block text-sm font-medium mb-2">{t('systemContext.field.name')}</label>
            <input
              type="text"
              value={formData.name}
              onChange={(event) => onNameChange(event.target.value)}
              placeholder={t('systemContext.field.namePlaceholder')}
              className="w-full px-3 py-2 border border-input bg-background rounded-md focus:outline-none focus:ring-2 focus:ring-ring"
            />
          </div>
        </div>

        <div className="grid grid-cols-1 lg:grid-cols-4 gap-3">
          <input
            value={assistantForm.scene}
            onChange={(event) => onAssistantFieldChange('scene', event.target.value)}
            placeholder={t('systemContext.field.scenePlaceholder')}
            className="px-3 py-2 text-sm border border-input bg-background rounded-md"
          />
          <input
            value={assistantForm.style}
            onChange={(event) => onAssistantFieldChange('style', event.target.value)}
            placeholder={t('systemContext.field.stylePlaceholder')}
            className="px-3 py-2 text-sm border border-input bg-background rounded-md"
          />
          <input
            value={assistantForm.language}
            onChange={(event) => onAssistantFieldChange('language', event.target.value)}
            placeholder={t('systemContext.field.languagePlaceholder')}
            className="px-3 py-2 text-sm border border-input bg-background rounded-md"
          />
          <input
            value={assistantForm.outputFormat}
            onChange={(event) => onAssistantFieldChange('outputFormat', event.target.value)}
            placeholder={t('systemContext.field.outputFormatPlaceholder')}
            className="px-3 py-2 text-sm border border-input bg-background rounded-md"
          />
        </div>

        <div className="grid grid-cols-1 lg:grid-cols-2 gap-3">
          <textarea
            value={assistantForm.constraintsText}
            onChange={(event) => onAssistantFieldChange('constraintsText', event.target.value)}
            placeholder={t('systemContext.field.constraintsPlaceholder')}
            rows={3}
            className="w-full px-3 py-2 text-sm border border-input bg-background rounded-md resize-none"
          />
          <textarea
            value={assistantForm.forbiddenText}
            onChange={(event) => onAssistantFieldChange('forbiddenText', event.target.value)}
            placeholder={t('systemContext.field.forbiddenPlaceholder')}
            rows={3}
            className="w-full px-3 py-2 text-sm border border-input bg-background rounded-md resize-none"
          />
        </div>

        <div>
          <label className="block text-sm font-medium mb-2">{t('systemContext.field.optimizeGoal')}</label>
          <input
            value={assistantForm.optimizeGoal}
            onChange={(event) => onAssistantFieldChange('optimizeGoal', event.target.value)}
            placeholder={t('systemContext.field.optimizeGoalPlaceholder')}
            className="w-full px-3 py-2 text-sm border border-input bg-background rounded-md"
          />
        </div>

        {assistantError ? (
          <div className="text-sm text-red-600 bg-red-50 dark:bg-red-950/20 border border-red-200 dark:border-red-900 rounded-md px-3 py-2">
            {assistantError}
          </div>
        ) : null}

        {qualityReport ? (
          <div className="grid grid-cols-2 md:grid-cols-5 gap-2 text-xs">
            <div className="px-2 py-2 border rounded-md">{t('systemContext.quality.clarity')}: {qualityReport.clarity ?? '-'}</div>
            <div className="px-2 py-2 border rounded-md">{t('systemContext.quality.constraintCompleteness')}: {qualityReport.constraint_completeness ?? '-'}</div>
            <div className="px-2 py-2 border rounded-md">{t('systemContext.quality.conflictRisk')}: {qualityReport.conflict_risk ?? '-'}</div>
            <div className="px-2 py-2 border rounded-md">{t('systemContext.quality.verbosity')}: {qualityReport.verbosity ?? '-'}</div>
            <div className="px-2 py-2 border rounded-md font-medium">{t('systemContext.quality.overall')}: {qualityReport.overall ?? '-'}</div>
          </div>
        ) : null}

        {candidates.length > 0 ? (
          <div className="space-y-2">
            <p className="text-sm font-medium">{t('systemContext.candidates.title')}</p>
            <div className="space-y-2 max-h-48 overflow-y-auto">
              {candidates.map((candidate, index) => (
                <div
                  key={`${candidate.title || 'candidate'}-${index}`}
                  className="border border-border rounded-md p-3"
                >
                  <div className="flex items-center justify-between gap-3">
                    <div className="text-sm font-medium">
                      {candidate.title || t('systemContext.candidates.item', { index: index + 1 })}
                      {typeof candidate.score === 'number' ? ` - ${t('systemContext.candidates.score', { score: candidate.score })}` : ''}
                    </div>
                    <button
                      onClick={() => onSelectCandidate(candidate)}
                      className="px-2 py-1 text-xs border border-border rounded hover:bg-accent"
                    >
                      {t('systemContext.candidates.use')}
                    </button>
                  </div>
                  <p className="mt-2 text-xs text-muted-foreground line-clamp-2">
                    {candidate.content}
                  </p>
                </div>
              ))}
            </div>
          </div>
        ) : null}

        <div className="min-h-[360px] flex flex-col">
          <div className="mb-2 flex items-center justify-between">
            <label className="text-sm font-medium">{t('systemContext.content.title')}</label>
            <span className="text-xs text-muted-foreground">{t('systemContext.content.characterCount', { count: formData.content.length })}</span>
          </div>
          <textarea
            value={formData.content}
            onChange={(event) => onContentChange(event.target.value)}
            className="flex-1 w-full min-h-[360px] px-4 py-3 border border-input bg-background rounded-md resize-none font-mono text-sm"
            placeholder={t('systemContext.content.placeholder')}
          />
        </div>

        {actionError ? (
          <div className="text-sm text-red-600">{actionError}</div>
        ) : null}
      </div>
    </>
  );
}
