// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import React, { useState } from 'react';
import { Eye, FileText, Loader2, PlayCircle } from 'lucide-react';

import { useI18n } from '../../i18n/I18nProvider';
import type { ProjectRuntimeEnvironmentImageResponse } from '../../lib/api/client/types';

interface CloudRuntimeImagePlansProps {
  images: ProjectRuntimeEnvironmentImageResponse[];
  isCloudProject: boolean;
  buildingImageId: string | null;
  onGenerateImage: (imageId: string) => void;
}

type ImageRecord = Record<string, unknown>;

const READY_IMAGE_STATUSES = new Set([
  'ready',
  'available',
  'local',
  'succeeded',
  'completed',
  'running',
]);

export const CloudRuntimeImagePlans: React.FC<CloudRuntimeImagePlansProps> = ({
  images,
  isCloudProject,
  buildingImageId,
  onGenerateImage,
}) => {
  const { t } = useI18n();
  const [expandedImageId, setExpandedImageId] = useState<string | null>(null);
  const applicationImageId = images
    .map((image, index) => {
      const record = asRecord(image);
      return readString(record, ['dockerfile'])
        ? readString(record, ['id'], `image-${index}`)
        : '';
    })
    .find(Boolean);
  const isPreparingAll = buildingImageId !== null;
  const allImagesReady = images.length > 0 && images.every((image) => (
    READY_IMAGE_STATUSES.has(readString(asRecord(image), ['status']).toLowerCase())
  ));
  const prepareDisabled = !applicationImageId || isPreparingAll || allImagesReady;

  return (
    <section className="border-t border-border px-4 py-4">
      <div className="flex items-center justify-between gap-3">
        <h3 className="text-sm font-semibold text-foreground">{t('cloudRuntime.images')}</h3>
        <button
          type="button"
          disabled={prepareDisabled}
          onClick={() => {
            if (applicationImageId) onGenerateImage(applicationImageId);
          }}
          title={!applicationImageId ? t('cloudRuntime.noPreparatableImages') : undefined}
          className="inline-flex h-8 items-center gap-1.5 bg-primary px-3 text-xs text-primary-foreground hover:bg-primary/90 disabled:cursor-not-allowed disabled:opacity-50"
        >
          {isPreparingAll ? (
            <Loader2 className="h-3.5 w-3.5 animate-spin" aria-hidden="true" />
          ) : (
            <PlayCircle className="h-3.5 w-3.5" aria-hidden="true" />
          )}
          {isPreparingAll
            ? isCloudProject
              ? t('cloudRuntime.preparingAllImages')
              : t('cloudRuntime.localBuilding')
            : allImagesReady
              ? isCloudProject
                ? t('cloudRuntime.allImagesReady')
                : t('cloudRuntime.localRunning')
              : applicationImageId
                ? isCloudProject
                  ? t('cloudRuntime.prepareAllImages')
                  : t('cloudRuntime.localBuild')
                : t('cloudRuntime.noPreparatableImages')}
        </button>
      </div>
      <div className="mt-3 overflow-x-auto border border-border">
        <table className="w-full min-w-[1120px] text-left text-xs">
          <thead className="bg-muted/40 text-muted-foreground">
            <tr>
              <th className="px-3 py-2 font-medium">{t('cloudRuntime.environment')}</th>
              <th className="px-3 py-2 font-medium">{t('cloudRuntime.image')}</th>
              <th className="px-3 py-2 font-medium">{t('cloudRuntime.provider')}</th>
              <th className="px-3 py-2 font-medium">{t('cloudRuntime.mcpPolicy')}</th>
              <th className="px-3 py-2 font-medium">{t('cloudRuntime.status')}</th>
              <th className="px-3 py-2 font-medium">{t('cloudRuntime.ports')}</th>
              <th className="px-3 py-2 font-medium">{t('cloudRuntime.envVars')}</th>
              <th className="px-3 py-2 font-medium">Dockerfile</th>
              <th className="px-3 py-2 font-medium">{t('cloudRuntime.action')}</th>
              <th className="px-3 py-2 font-medium">{t('cloudRuntime.error')}</th>
            </tr>
          </thead>
          <tbody className="divide-y divide-border">
            {images.length === 0 ? (
              <tr>
                <td className="px-3 py-6 text-center text-muted-foreground" colSpan={10}>
                  {t('cloudRuntime.noImages')}
                </td>
              </tr>
            ) : images.map((image, index) => {
              const record = asRecord(image);
              const id = readString(record, ['id'], `image-${index}`);
              const status = readString(record, ['status'], '-');
              const dockerfile = readString(record, ['dockerfile']);
              const serviceRole = readString(record, ['service_role', 'serviceRole'], 'unknown');
              const serviceId = readString(record, ['service_id', 'serviceId', 'environment_key', 'environmentKey'], '-');
              const mcpPolicy = asRecord(record.mcp_policy ?? record.mcpPolicy);
              const isMcpTarget = readString(mcpPolicy, ['managed_by', 'managedBy']) === 'system'
                && readString(mcpPolicy, ['attachment']) === 'project_gateway_target';
              const isBuilding = isPreparingAll;
              const expanded = expandedImageId === id;
              return (
                <React.Fragment key={id}>
                  <tr>
                    <td className="px-3 py-2">
                      <div>{readString(record, ['display_name', 'displayName', 'environment_key', 'environmentKey'], '-')}</div>
                      <div className="mt-1 font-mono text-[10px] text-muted-foreground">{serviceId} · {serviceRole}</div>
                    </td>
                    <td className="px-3 py-2 font-mono">{readString(record, ['image_ref', 'imageRef', 'image_id', 'imageId'], '-')}</td>
                    <td className="px-3 py-2 font-mono">{readString(record, ['image_provider', 'imageProvider'], '-')}</td>
                    <td className="px-3 py-2">
                      <span className={isMcpTarget ? 'text-emerald-700' : 'text-muted-foreground'}>
                        {isMcpTarget ? t('cloudRuntime.mcpTarget') : t('cloudRuntime.mcpNone')}
                      </span>
                    </td>
                    <td className="px-3 py-2">{status}</td>
                    <td className="whitespace-pre-wrap px-3 py-2 font-mono">{displayValue(record.ports)}</td>
                    <td className="whitespace-pre-wrap px-3 py-2 font-mono">{displayValue(record.env_vars ?? record.envVars)}</td>
                    <td className="px-3 py-2">
                      {dockerfile ? (
                        <button
                          type="button"
                          onClick={() => setExpandedImageId(expanded ? null : id)}
                          className="inline-flex items-center gap-1 text-primary hover:underline"
                        >
                          <Eye className="h-3.5 w-3.5" aria-hidden="true" />
                          {expanded ? t('cloudRuntime.hideDockerfile') : t('cloudRuntime.viewDockerfile')}
                        </button>
                      ) : '-'}
                    </td>
                    <td className="px-3 py-2">
                      {isCloudProject && dockerfile ? (
                        <span className={isBuilding ? 'text-primary' : 'text-muted-foreground'}>
                          {isBuilding
                            ? t('cloudRuntime.generatingImage')
                            : t('cloudRuntime.includedInBatchPreparation')}
                        </span>
                      ) : !isCloudProject && dockerfile ? (
                        <span className="text-muted-foreground">{t('cloudRuntime.localDockerfilePlan')}</span>
                      ) : (
                        <span className="text-muted-foreground">{t('cloudRuntime.platformImage')}</span>
                      )}
                    </td>
                    <td className="px-3 py-2 text-destructive">{readString(record, ['error'], '-')}</td>
                  </tr>
                  {expanded && dockerfile ? (
                    <tr>
                      <td colSpan={10} className="bg-muted/20 p-3">
                        <div className="mb-2 flex items-center gap-2 text-xs font-medium text-foreground">
                          <FileText className="h-3.5 w-3.5" aria-hidden="true" />
                          Dockerfile
                        </div>
                        <pre className="max-h-96 overflow-auto whitespace-pre p-3 font-mono text-[11px] leading-5 text-foreground">
                          {dockerfile}
                        </pre>
                      </td>
                    </tr>
                  ) : null}
                </React.Fragment>
              );
            })}
          </tbody>
        </table>
      </div>
    </section>
  );
};

const asRecord = (value: unknown): ImageRecord => (
  value && typeof value === 'object' && !Array.isArray(value) ? value as ImageRecord : {}
);

const readString = (record: ImageRecord, keys: string[], fallback = ''): string => {
  for (const key of keys) {
    const value = record[key];
    if (typeof value === 'string' && value.trim()) return value.trim();
  }
  return fallback;
};

const displayValue = (value: unknown): string => {
  if (value == null || value === '') return '-';
  if (typeof value === 'string' || typeof value === 'number' || typeof value === 'boolean') {
    return String(value);
  }
  try {
    return JSON.stringify(value, null, 2);
  } catch {
    return String(value);
  }
};

export default CloudRuntimeImagePlans;
