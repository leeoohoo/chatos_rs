// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import React from 'react';

import { useI18n } from '../../i18n/I18nProvider';
import { cn } from '../../lib/utils';

export const MAX_ANALYSIS_REQUIREMENT_LENGTH = 4_000;

interface DependencyGroup {
  key: string;
  titleKey: string;
  dependencies: string[];
}

const DEPENDENCY_GROUPS: DependencyGroup[] = [
  {
    key: 'database',
    titleKey: 'cloudRuntime.dependencyGroup.database',
    dependencies: [
      'PostgreSQL',
      'MySQL',
      'MariaDB',
      'MongoDB',
      'SQLite',
      'Microsoft SQL Server',
      'Oracle Database',
      'ClickHouse',
      'CockroachDB',
      'TiDB',
      'Apache Cassandra',
      'Neo4j',
      'InfluxDB',
    ],
  },
  {
    key: 'cache',
    titleKey: 'cloudRuntime.dependencyGroup.cache',
    dependencies: ['Redis', 'Valkey', 'Memcached', 'Dragonfly'],
  },
  {
    key: 'messaging',
    titleKey: 'cloudRuntime.dependencyGroup.messaging',
    dependencies: [
      'RabbitMQ',
      'Apache Kafka',
      'Redpanda',
      'Apache RocketMQ',
      'Apache Pulsar',
      'NATS',
      'MQTT / Mosquitto',
      'ActiveMQ Artemis',
    ],
  },
  {
    key: 'search',
    titleKey: 'cloudRuntime.dependencyGroup.searchAndVector',
    dependencies: [
      'Elasticsearch',
      'OpenSearch',
      'Meilisearch',
      'Typesense',
      'Milvus',
      'Qdrant',
      'Weaviate',
      'pgvector',
      'Chroma',
    ],
  },
  {
    key: 'platform',
    titleKey: 'cloudRuntime.dependencyGroup.platform',
    dependencies: [
      'MinIO',
      'S3-compatible storage',
      'Nacos',
      'Consul',
      'etcd',
      'ZooKeeper',
      'Keycloak',
      'HashiCorp Vault',
      'LocalStack',
      'Temporal',
      'Mailpit',
    ],
  },
  {
    key: 'observability',
    titleKey: 'cloudRuntime.dependencyGroup.observability',
    dependencies: [
      'Prometheus',
      'Grafana',
      'Loki',
      'Jaeger',
      'Tempo',
      'Zipkin',
      'OpenTelemetry Collector',
      'Sentry-compatible service',
    ],
  },
];

const COMMON_DEPENDENCIES = [
  'PostgreSQL',
  'Redis',
  'RabbitMQ',
  'MinIO',
  'Nacos',
  'Elasticsearch',
];

interface RuntimeAnalysisRequirementDialogProps {
  requirement: string;
  selectedDependencies: string[];
  preferChinaMirrors: boolean;
  error: string | null;
  onRequirementChange: (value: string) => void;
  onSelectedDependenciesChange: (dependencies: string[]) => void;
  onPreferChinaMirrorsChange: (value: boolean) => void;
  onCancel: () => void;
  onSubmit: () => void;
}

export const RuntimeAnalysisRequirementDialog: React.FC<RuntimeAnalysisRequirementDialogProps> = ({
  requirement,
  selectedDependencies,
  preferChinaMirrors,
  error,
  onRequirementChange,
  onSelectedDependenciesChange,
  onPreferChinaMirrorsChange,
  onCancel,
  onSubmit,
}) => {
  const { t } = useI18n();
  const selectedSet = new Set(selectedDependencies);
  const canSubmit = Boolean(
    requirement.trim() || selectedDependencies.length > 0 || preferChinaMirrors,
  );

  const setDependencySelected = (dependency: string, checked: boolean) => {
    if (checked) {
      onSelectedDependenciesChange(
        selectedSet.has(dependency) ? selectedDependencies : [...selectedDependencies, dependency],
      );
      return;
    }
    onSelectedDependenciesChange(selectedDependencies.filter((item) => item !== dependency));
  };

  const selectCommonDependencies = () => {
    const next = [...selectedDependencies];
    for (const dependency of COMMON_DEPENDENCIES) {
      if (!next.includes(dependency)) {
        next.push(dependency);
      }
    }
    onSelectedDependenciesChange(next);
  };

  return (
    <div
      className="fixed inset-0 z-[70] flex items-center justify-center overflow-y-auto bg-black/50 p-3 sm:p-6"
      role="dialog"
      aria-modal="true"
      aria-labelledby="runtime-analysis-requirement-title"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget) {
          onCancel();
        }
      }}
      onKeyDown={(event) => {
        if (event.key === 'Escape') {
          onCancel();
        }
        if ((event.metaKey || event.ctrlKey) && event.key === 'Enter') {
          event.preventDefault();
          if (canSubmit) {
            onSubmit();
          }
        }
      }}
    >
      <div className="flex max-h-[92vh] w-full max-w-6xl flex-col overflow-hidden rounded-xl border border-border bg-card shadow-2xl">
        <div className="shrink-0 border-b border-border px-5 py-4 sm:px-6">
          <h3
            id="runtime-analysis-requirement-title"
            className="text-lg font-semibold text-foreground"
          >
            {t('cloudRuntime.requirementDialogTitle')}
          </h3>
          <p className="mt-1 max-w-4xl text-sm leading-6 text-muted-foreground">
            {t('cloudRuntime.requirementDialogDescription')}
          </p>
        </div>

        <div className="grid min-h-0 flex-1 overflow-y-auto lg:grid-cols-[minmax(0,1.45fr)_minmax(340px,0.75fr)] lg:overflow-hidden">
          <section className="min-h-0 border-b border-border p-5 lg:overflow-y-auto lg:border-b-0 lg:border-r sm:p-6">
            <div className="mb-4 flex flex-wrap items-start justify-between gap-3">
              <div>
                <div className="text-sm font-semibold text-foreground">
                  {t('cloudRuntime.dependencySectionTitle')}
                </div>
                <div className="mt-1 text-xs leading-5 text-muted-foreground">
                  {t('cloudRuntime.dependencySectionDescription')}
                </div>
              </div>
              <div className="flex flex-wrap items-center gap-2">
                <span className="rounded-full bg-primary/10 px-2.5 py-1 text-xs font-medium text-primary">
                  {t('cloudRuntime.dependencySelectedCount', {
                    count: selectedDependencies.length,
                  })}
                </span>
                <button
                  type="button"
                  onClick={selectCommonDependencies}
                  className="h-8 rounded-md border border-border bg-background px-3 text-xs text-foreground hover:bg-accent"
                >
                  {t('cloudRuntime.selectCommonDependencies')}
                </button>
                <button
                  type="button"
                  onClick={() => onSelectedDependenciesChange([])}
                  disabled={selectedDependencies.length === 0}
                  className="h-8 rounded-md border border-border bg-background px-3 text-xs text-foreground hover:bg-accent disabled:cursor-not-allowed disabled:opacity-50"
                >
                  {t('cloudRuntime.clearDependencies')}
                </button>
              </div>
            </div>

            <div className="space-y-5">
              {DEPENDENCY_GROUPS.map((group) => (
                <fieldset key={group.key}>
                  <legend className="mb-2 text-xs font-semibold text-foreground/80">
                    {t(group.titleKey)}
                  </legend>
                  <div className="grid gap-2 sm:grid-cols-2 xl:grid-cols-3">
                    {group.dependencies.map((dependency) => {
                      const checked = selectedSet.has(dependency);
                      return (
                        <label
                          key={dependency}
                          className={cn(
                            'flex min-h-10 cursor-pointer items-center gap-2 rounded-md border px-3 py-2 text-xs transition-colors',
                            checked
                              ? 'border-primary bg-primary/5 text-foreground'
                              : 'border-border bg-background text-muted-foreground hover:border-primary/50 hover:bg-accent/50',
                          )}
                        >
                          <input
                            type="checkbox"
                            checked={checked}
                            onChange={(event) => {
                              setDependencySelected(dependency, event.target.checked);
                            }}
                            className="h-4 w-4 shrink-0 accent-primary"
                          />
                          <span>{dependency}</span>
                        </label>
                      );
                    })}
                  </div>
                </fieldset>
              ))}
            </div>
          </section>

          <section className="flex min-h-[330px] flex-col p-5 sm:p-6 lg:min-h-0 lg:overflow-y-auto">
            <label
              htmlFor="runtime-analysis-requirement"
              className="mb-2 block text-sm font-semibold text-foreground"
            >
              {t('cloudRuntime.requirementLabel')}
            </label>
            <textarea
              id="runtime-analysis-requirement"
              autoFocus
              maxLength={MAX_ANALYSIS_REQUIREMENT_LENGTH}
              value={requirement}
              onChange={(event) => onRequirementChange(event.target.value)}
              placeholder={t('cloudRuntime.requirementPlaceholder')}
              className="min-h-64 flex-1 resize-y rounded-md border border-border bg-background px-3 py-2 text-sm leading-6 text-foreground outline-none transition-colors placeholder:text-muted-foreground focus:border-primary lg:min-h-[360px]"
            />
            <div className="mt-2 flex items-start justify-between gap-3 text-xs">
              <span className={error ? 'text-destructive' : 'text-muted-foreground'}>
                {error || t('cloudRuntime.requirementHint')}
              </span>
              <span className="shrink-0 text-muted-foreground">
                {requirement.length}/{MAX_ANALYSIS_REQUIREMENT_LENGTH}
              </span>
            </div>
            <label className="mt-4 flex cursor-pointer items-start gap-3 rounded-lg border border-border bg-background px-3 py-3 text-sm transition-colors hover:border-primary/50 hover:bg-accent/40">
              <input
                type="checkbox"
                checked={preferChinaMirrors}
                onChange={(event) => onPreferChinaMirrorsChange(event.target.checked)}
                className="mt-0.5 h-4 w-4 shrink-0 accent-primary"
              />
              <span className="min-w-0">
                <span className="block font-medium text-foreground">
                  {t('cloudRuntime.preferChinaMirrorsLabel')}
                </span>
                <span className="mt-1 block text-xs leading-5 text-muted-foreground">
                  {t('cloudRuntime.preferChinaMirrorsHint')}
                </span>
              </span>
            </label>
          </section>
        </div>

        <div className="flex shrink-0 items-center justify-between gap-3 border-t border-border px-5 py-4 sm:px-6">
          <span className="hidden text-xs text-muted-foreground sm:inline">
            {t('cloudRuntime.requirementSubmitHint')}
          </span>
          <div className="ml-auto flex gap-2">
            <button
              type="button"
              onClick={onCancel}
              className="h-9 rounded-md border border-border bg-background px-4 text-sm text-foreground hover:bg-accent"
            >
              {t('common.cancel')}
            </button>
            <button
              type="button"
              onClick={onSubmit}
              disabled={!canSubmit}
              className="h-9 rounded-md bg-primary px-5 text-sm text-primary-foreground hover:bg-primary/90 disabled:cursor-not-allowed disabled:opacity-50"
            >
              {t('cloudRuntime.startAnalysis')}
            </button>
          </div>
        </div>
      </div>
    </div>
  );
};

export default RuntimeAnalysisRequirementDialog;
