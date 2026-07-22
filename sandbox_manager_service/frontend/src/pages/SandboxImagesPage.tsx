// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { CloudDownloadOutlined, ReloadOutlined } from '@ant-design/icons';
import { App, Button, Input, Select, Space, Table, Tag, Typography } from 'antd';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import dayjs from 'dayjs';
import { useEffect, useMemo, useState } from 'react';

import { sandboxesApi } from '../api/sandboxes';
import { QueryErrorAlert } from '../components/QueryErrorAlert';
import { useI18n } from '../i18n';
import type {
  SandboxImageFeatureRecord,
  SandboxImageJobRecord,
  SandboxImageRecord,
} from '../types';

const IMAGE_SELECTION_STORAGE_KEY = 'sandbox_manager_image_runtime_versions';

export function SandboxImagesPage() {
  const { t } = useI18n();
  const { message } = App.useApp();
  const queryClient = useQueryClient();
  const [selectedRuntimeVersions, setSelectedRuntimeVersions] = useState<Record<string, string | undefined>>(
    loadSelectedRuntimeVersions,
  );
  const [customBuildScript, setCustomBuildScript] = useState('');
  const [customBuildScriptHash, setCustomBuildScriptHash] = useState<string>();
  const imagesQuery = useQuery({
    queryKey: ['sandbox-images'],
    queryFn: sandboxesApi.images,
    refetchInterval: 5000,
  });
  const jobsQuery = useQuery({
    queryKey: ['sandbox-image-jobs'],
    queryFn: sandboxesApi.imageJobs,
    refetchInterval: 2000,
  });
  const initializeMutation = useMutation({
    mutationFn: (payload: { features: string[]; custom_build_script?: string }) =>
      sandboxesApi.initializeImage(payload),
    onSuccess: async () => {
      message.success(t('image.initializeStarted'));
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: ['sandbox-image-jobs'] }),
        queryClient.invalidateQueries({ queryKey: ['sandbox-images'] }),
      ]);
    },
    onError: (error) =>
      message.error(error instanceof Error ? error.message : t('image.initializeFailure')),
  });
  const jobs = jobsQuery.data ?? [];
  const runtimes = imagesQuery.data?.features ?? [];
  const queryError = imagesQuery.error ?? jobsQuery.error;

  useEffect(() => {
    persistSelectedRuntimeVersions(selectedRuntimeVersions);
  }, [selectedRuntimeVersions]);

  const normalizedCustomBuildScript = customBuildScript.trim();

  useEffect(() => {
    let cancelled = false;
    if (!normalizedCustomBuildScript) {
      setCustomBuildScriptHash(undefined);
      return;
    }
    setCustomBuildScriptHash(undefined);
    void sha256Short(normalizedCustomBuildScript).then((hash) => {
      if (!cancelled) {
        setCustomBuildScriptHash(hash);
      }
    });
    return () => {
      cancelled = true;
    };
  }, [normalizedCustomBuildScript]);

  const runningJobImageIds = useMemo(
    () => new Set(jobs.filter((job) => job.status === 'running').map((job) => job.image_id)),
    [jobs],
  );
  const selectedFeatures = useMemo(
    () => buildSelectedFeatures(runtimes, selectedRuntimeVersions),
    [runtimes, selectedRuntimeVersions],
  );
  const selectedImage = useMemo(
    () =>
      buildSelectedImagePreview(
        imagesQuery.data?.image_tag_prefix,
        selectedFeatures,
        normalizedCustomBuildScript ? customBuildScriptHash : undefined,
      ),
    [
      customBuildScriptHash,
      imagesQuery.data?.image_tag_prefix,
      normalizedCustomBuildScript,
      selectedFeatures,
    ],
  );
  const selectedImageInitialized = useMemo(
    () => imagesQuery.data?.images.some((image) => image.id === selectedImage?.id) ?? false,
    [imagesQuery.data?.images, selectedImage?.id],
  );
  const selectedImageRunning = selectedImage ? runningJobImageIds.has(selectedImage.id) : false;
  const canInitialize =
    Boolean(selectedImage) && (selectedFeatures.length > 0 || Boolean(normalizedCustomBuildScript));

  return (
    <Space direction="vertical" size={16} style={{ width: '100%' }}>
      <div className="page-heading">
        <div>
          <Typography.Title level={3}>{t('image.title')}</Typography.Title>
          <Typography.Text type="secondary">{t('image.subtitle')}</Typography.Text>
        </div>
        <Button
          icon={<ReloadOutlined />}
          onClick={() => {
            void imagesQuery.refetch();
            void jobsQuery.refetch();
          }}
          loading={imagesQuery.isFetching || jobsQuery.isFetching}
        >
          {t('common.refresh')}
        </Button>
      </div>

      <QueryErrorAlert
        error={queryError}
        loadFailedTitle={t('image.loadFailed')}
        authorizationDescription={t('image.authorizationFailed')}
        retryLabel={t('common.retry')}
        onRetry={() => {
          void imagesQuery.refetch();
          void jobsQuery.refetch();
        }}
      />

      <div className="surface">
        <Space direction="vertical" size={14} style={{ width: '100%' }}>
          <div className="section-heading-row">
            <Typography.Title level={4}>{t('image.create')}</Typography.Title>
          </div>
          <Table<SandboxImageFeatureRecord>
            rowKey="id"
            size="small"
            pagination={false}
            dataSource={runtimes}
            columns={[
              {
                title: t('image.runtime'),
                dataIndex: 'label',
                width: 180,
              },
              {
                title: t('image.version'),
                dataIndex: 'default_version',
                width: 260,
                render: (_, runtime) => {
                  const selectedVersion = runtime.versions.find(
                    (version) => version.id === selectedRuntimeVersions[runtime.id],
                  );
                  return (
                    <Space direction="vertical" size={4}>
                      <Select
                        allowClear
                        style={{ width: 220 }}
                        placeholder={t('image.notInstalled')}
                        value={selectedRuntimeVersions[runtime.id]}
                        options={runtime.versions.map((version) => ({
                          label: version.default ? `${version.label} · ${t('image.recommended')}` : version.label,
                          value: version.id,
                        }))}
                        onChange={(value) =>
                          setSelectedRuntimeVersions((current) => ({
                            ...current,
                            [runtime.id]: value,
                          }))
                        }
                      />
                      <Typography.Text type="secondary">
                        {selectedVersion?.description ?? t('image.notInstalled')}
                      </Typography.Text>
                    </Space>
                  );
                },
              },
              {
                title: t('image.description'),
                dataIndex: 'description',
              },
            ]}
          />
          <Space direction="vertical" size={6} style={{ width: '100%' }}>
            <Typography.Text>{t('image.customBuildScript')}</Typography.Text>
            <Input.TextArea
              value={customBuildScript}
              onChange={(event) => setCustomBuildScript(event.target.value)}
              placeholder={t('image.customBuildScriptPlaceholder')}
              rows={6}
              className="json-input"
            />
            <Typography.Text type="secondary">{t('image.customBuildScriptHint')}</Typography.Text>
          </Space>
          <Space direction="vertical" size={8}>
            <Space wrap>
              <Typography.Text type="secondary">{t('image.selected')}</Typography.Text>
              {selectedFeatures.length > 0 || customBuildScriptHash ? (
                renderFeatureTags(
                  customBuildScriptHash ? [...selectedFeatures, `script@${customBuildScriptHash}`] : selectedFeatures,
                  t,
                )
              ) : (
                <Tag>{t('image.noSelection')}</Tag>
              )}
            </Space>
            <Space wrap>
              <Typography.Text type="secondary">{selectedImage?.image_ref ?? '-'}</Typography.Text>
              {selectedImageInitialized ? <Tag color="success">{t('image.ready')}</Tag> : null}
              <Button
                type="primary"
                icon={<CloudDownloadOutlined />}
                loading={initializeMutation.isPending || selectedImageRunning}
                disabled={!canInitialize || selectedImageInitialized}
                onClick={() =>
                  initializeMutation.mutate({
                    features: selectedFeatures,
                    custom_build_script: normalizedCustomBuildScript || undefined,
                  })
                }
              >
                {selectedImageRunning ? t('image.initializing') : t('image.initialize')}
              </Button>
            </Space>
          </Space>
        </Space>
      </div>

      <div className="surface">
        <Space direction="vertical" size={14} style={{ width: '100%' }}>
          <div className="section-heading-row">
            <Typography.Title level={4}>{t('image.jobs')}</Typography.Title>
            <Typography.Text type="secondary">{t('image.jobsHint')}</Typography.Text>
          </div>
          <Table<SandboxImageJobRecord>
            rowKey="id"
            size="middle"
            loading={jobsQuery.isLoading}
            dataSource={jobs}
            pagination={{ pageSize: 5 }}
            scroll={{ x: 1000 }}
            expandable={{
              rowExpandable: (job) => Boolean(job.output || job.error),
              expandedRowRender: (job) => (
                <pre className="log-panel">{job.error ? `${job.error}\n\n` : ''}{job.output || '-'}</pre>
              ),
            }}
            columns={[
              {
                title: t('common.status'),
                dataIndex: 'status',
                width: 130,
                render: (status) => renderJobStatus(status, t),
              },
              {
                title: t('common.image'),
                dataIndex: 'image_name',
                width: 180,
              },
              {
                title: t('image.features'),
                dataIndex: 'features',
                width: 220,
                render: (_, job) => renderFeatureTags(job.features, t),
              },
              {
                title: t('image.reference'),
                dataIndex: 'image_ref',
                render: (value) => <Typography.Text copyable>{value}</Typography.Text>,
              },
              {
                title: t('image.updatedAt'),
                dataIndex: 'updated_at',
                width: 170,
                render: (value) => dayjs(value).format('MM-DD HH:mm:ss'),
              },
            ]}
          />
        </Space>
      </div>

      <div className="surface">
        <Space direction="vertical" size={14} style={{ width: '100%' }}>
          <div className="section-heading-row">
            <Typography.Title level={4}>{t('image.initializedImages')}</Typography.Title>
          </div>
          <Table<SandboxImageRecord>
            rowKey="id"
            size="middle"
            loading={imagesQuery.isLoading}
            dataSource={imagesQuery.data?.images ?? []}
            pagination={{ pageSize: 8 }}
            scroll={{ x: 1000 }}
            columns={[
              {
                title: t('common.status'),
                dataIndex: 'status',
                width: 130,
                render: (_, image) => renderImageStatus(image, runningJobImageIds, t),
              },
              {
                title: t('common.image'),
                dataIndex: 'name',
                width: 180,
              },
              {
                title: t('image.features'),
                dataIndex: 'features',
                width: 260,
                render: (_, image) =>
                  image.is_default ? <Tag color="blue">{t('image.default')}</Tag> : renderFeatureTags(image.features, t),
              },
              {
                title: t('image.reference'),
                dataIndex: 'image_ref',
                render: (value) => <Typography.Text copyable>{value}</Typography.Text>,
              },
              {
                title: t('common.backend'),
                dataIndex: 'backend',
                width: 110,
              },
            ]}
          />
        </Space>
      </div>
    </Space>
  );
}

function loadSelectedRuntimeVersions() {
  try {
    const stored = window.localStorage.getItem(IMAGE_SELECTION_STORAGE_KEY);
    if (!stored) {
      return {};
    }
    const parsed = JSON.parse(stored) as unknown;
    if (!parsed || typeof parsed !== 'object' || Array.isArray(parsed)) {
      return {};
    }
    const selections: Record<string, string> = {};
    for (const [runtime, version] of Object.entries(parsed)) {
      if (typeof version === 'string' && version) {
        selections[runtime] = version;
      }
    }
    return selections;
  } catch {
    return {};
  }
}

function persistSelectedRuntimeVersions(selections: Record<string, string | undefined>) {
  const clean: Record<string, string> = {};
  for (const [runtime, version] of Object.entries(selections)) {
    if (version) {
      clean[runtime] = version;
    }
  }
  if (Object.keys(clean).length === 0) {
    window.localStorage.removeItem(IMAGE_SELECTION_STORAGE_KEY);
    return;
  }
  window.localStorage.setItem(IMAGE_SELECTION_STORAGE_KEY, JSON.stringify(clean));
}

function buildSelectedFeatures(
  runtimes: SandboxImageFeatureRecord[],
  selectedRuntimeVersions: Record<string, string | undefined>,
) {
  return runtimes
    .map((runtime) => {
      const version = selectedRuntimeVersions[runtime.id];
      if (!version || !runtime.versions.some((candidate) => candidate.id === version)) {
        return undefined;
      }
      return `${runtime.id}@${version}`;
    })
    .filter((value): value is string => Boolean(value));
}

async function sha256Short(value: string) {
  const digest = await window.crypto.subtle.digest('SHA-256', new TextEncoder().encode(value));
  return Array.from(new Uint8Array(digest))
    .slice(0, 6)
    .map((byte) => byte.toString(16).padStart(2, '0'))
    .join('');
}

function buildSelectedImagePreview(
  imageTagPrefix: string | undefined,
  features: string[],
  customBuildScriptHash?: string,
) {
  if (!imageTagPrefix || (features.length === 0 && !customBuildScriptHash)) {
    return undefined;
  }
  const segments = features.map((feature) => feature.replace('@', ''));
  if (customBuildScriptHash) {
    segments.push(`script${customBuildScriptHash}`);
  }
  const id = `dev-${segments.join('-')}`;
  return {
    id,
    image_ref: `${imageTagPrefix}:${id}`,
  };
}

function renderImageStatus(
  image: SandboxImageRecord,
  runningJobImageIds: Set<string>,
  t: (key: string) => string,
) {
  if (runningJobImageIds.has(image.id)) {
    return <Tag color="processing">{t('image.initializing')}</Tag>;
  }
  if (image.initialized) {
    return <Tag color="success">{t('image.ready')}</Tag>;
  }
  if (image.status.startsWith('inspect_error')) {
    return <Tag color="error">{t('image.inspectError')}</Tag>;
  }
  return <Tag>{t('image.missing')}</Tag>;
}

function renderJobStatus(status: string, t: (key: string) => string) {
  if (status === 'running') {
    return <Tag color="processing">{t('image.job.running')}</Tag>;
  }
  if (status === 'succeeded') {
    return <Tag color="success">{t('image.job.succeeded')}</Tag>;
  }
  if (status === 'failed') {
    return <Tag color="error">{t('image.job.failed')}</Tag>;
  }
  return <Tag>{status}</Tag>;
}

function renderFeatureTags(features: string[], t: (key: string) => string) {
  if (features.length === 0) {
    return <Tag>{t('image.base')}</Tag>;
  }
  return (
    <Space size={[4, 4]} wrap>
      {features.map((feature) => (
        <Tag key={feature}>{formatFeatureLabel(feature)}</Tag>
      ))}
    </Space>
  );
}

function formatFeatureLabel(feature: string) {
  const [runtime, version] = feature.split('@');
  if (!version) {
    return feature;
  }
  if (runtime === 'script') {
    return `Custom script ${version}`;
  }
  const runtimeLabels: Record<string, string> = {
    java: 'JDK',
    node: 'Node.js',
    python: 'Python',
    rust: 'Rust',
    go: 'Go',
    dotnet: '.NET',
    php: 'PHP',
    ruby: 'Ruby',
    gcc: 'GCC',
    clang: 'Clang',
  };
  const runtimeLabel = runtimeLabels[runtime] ?? runtime.charAt(0).toUpperCase() + runtime.slice(1);
  return `${runtimeLabel} ${version}`;
}
