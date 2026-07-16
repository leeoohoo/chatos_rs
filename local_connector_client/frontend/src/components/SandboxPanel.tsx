// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import React from 'react';
import {
  Container,
  Image,
  ListChecks,
  RefreshCw,
  RotateCcw,
  Settings2,
  Shield,
  Trash2,
} from 'lucide-react';

import {
  api,
  type ConnectorStatus,
  type SandboxCapabilities,
  type SandboxImageCatalog,
  type SandboxImageJob,
  type SandboxLease,
  type SandboxSettings,
  type SandboxSettingsUpdate,
} from '../api';
import { SandboxPolicySettings } from './SandboxPolicySettings';

export function SandboxPanel({
  status,
  onStatus,
  onRefresh,
}: {
  status: ConnectorStatus;
  onStatus: (status: ConnectorStatus) => void;
  onRefresh: () => Promise<void>;
}) {
  const [catalog, setCatalog] = React.useState<SandboxImageCatalog | null>(null);
  const [capabilities, setCapabilities] = React.useState<SandboxCapabilities | null>(null);
  const [settings, setSettings] = React.useState<SandboxSettings | null>(null);
  const [jobs, setJobs] = React.useState<SandboxImageJob[]>([]);
  const [leases, setLeases] = React.useState<SandboxLease[]>([]);
  const [features, setFeatures] = React.useState<Record<string, string>>({});
  const [customScript, setCustomScript] = React.useState('');
  const [message, setMessage] = React.useState<string | null>(null);
  const [loadingDetails, setLoadingDetails] = React.useState(false);
  const [building, setBuilding] = React.useState(false);
  const [imageActionId, setImageActionId] = React.useState<string | null>(null);
  const [savingSettings, setSavingSettings] = React.useState(false);

  const refreshSandboxConfig = React.useCallback(async () => {
    try {
      const [nextCapabilities, nextSettings] = await Promise.all([
        api.sandboxCapabilities(),
        api.sandboxSettings(),
      ]);
      setCapabilities(nextCapabilities);
      setSettings(nextSettings);
    } catch (err) {
      setMessage(err instanceof Error ? err.message : '读取沙箱设置失败');
    }
  }, []);

  const refreshSandboxDetails = React.useCallback(async () => {
    if (!status.sandbox.enabled) {
      setCatalog(null);
      setJobs([]);
      setLeases([]);
      return;
    }
    setLoadingDetails(true);
    try {
      const [next, nextJobs, nextLeases] = await Promise.all([
        api.sandboxImages(),
        api.sandboxImageJobs(),
        api.sandboxLeases(),
      ]);
      setCatalog(next);
      setJobs(nextJobs);
      setLeases(nextLeases);
      setFeatures((current) => {
        const merged = { ...current };
        for (const feature of next.features) {
          if (typeof merged[feature.id] !== 'string') {
            merged[feature.id] = '';
          }
        }
        return merged;
      });
    } catch (err) {
      setMessage(err instanceof Error ? err.message : '读取镜像信息失败');
    } finally {
      setLoadingDetails(false);
    }
  }, [status.sandbox.enabled]);

  React.useEffect(() => {
    void refreshSandboxConfig();
  }, [refreshSandboxConfig]);

  React.useEffect(() => {
    void refreshSandboxDetails();
  }, [refreshSandboxDetails]);

  React.useEffect(() => {
    if (!status.sandbox.enabled) {
      return;
    }
    const interval = window.setInterval(() => {
      void refreshSandboxDetails();
    }, jobs.some((job) => job.status === 'running') ? 2500 : 6000);
    return () => window.clearInterval(interval);
  }, [jobs, refreshSandboxDetails, status.sandbox.enabled]);

  const setEnabled = async (enabled: boolean) => {
    setMessage(null);
    try {
      const next = await api.setSandboxEnabled({ enabled });
      onStatus(next);
      setMessage(enabled ? '本地保护已开启' : '本地保护已关闭');
      await Promise.all([refreshSandboxConfig(), onRefresh()]);
    } catch (err) {
      setMessage(err instanceof Error ? err.message : '沙箱设置失败');
    }
  };

  const saveSandboxSettings = async (
    patch: SandboxSettingsUpdate,
    label: string,
  ) => {
    setMessage(null);
    setSavingSettings(true);
    try {
      const next = await api.updateSandboxSettings(patch);
      setSettings(next);
      setMessage(`${label}已更新`);
      await Promise.all([refreshSandboxConfig(), onRefresh()]);
    } catch (err) {
      setMessage(err instanceof Error ? err.message : `${label}更新失败`);
    } finally {
      setSavingSettings(false);
    }
  };

  const selectedFeatures = Object.entries(features)
    .filter(([, version]) => version)
    .map(([id, version]) => `${id}@${version}`);

  const initialize = async () => {
    setMessage(null);
    setBuilding(true);
    try {
      const job = await api.initializeSandboxImage({
        features: selectedFeatures,
        custom_build_script: customScript.trim() || undefined,
      });
      setMessage(`镜像任务已创建: ${job.image_name}`);
      await refreshSandboxDetails();
    } catch (err) {
      setMessage(err instanceof Error ? err.message : '创建镜像失败');
    } finally {
      setBuilding(false);
    }
  };

  const deleteImage = async (imageId: string, imageRef: string, imageExists: boolean) => {
    const actionLabel = imageExists ? '删除本机 Docker 镜像' : '清理已缺失的镜像记录';
    if (!window.confirm(`确定${actionLabel} ${imageRef} 吗？`)) {
      return;
    }
    setMessage(null);
    setImageActionId(imageId);
    try {
      await api.deleteSandboxImage(imageId);
      setMessage(`${imageExists ? '镜像已删除' : '镜像记录已清理'}: ${imageRef}`);
      await Promise.all([refreshSandboxDetails(), onRefresh()]);
    } catch (err) {
      setMessage(err instanceof Error ? err.message : '删除镜像失败');
    } finally {
      setImageActionId(null);
    }
  };

  const reinitializeImage = async (imageId: string) => {
    setMessage(null);
    setImageActionId(imageId);
    try {
      const job = await api.reinitializeSandboxImage(imageId);
      setMessage(`重新初始化任务已创建: ${job.image_name}`);
      await refreshSandboxDetails();
    } catch (err) {
      setMessage(err instanceof Error ? err.message : '重新初始化镜像失败');
    } finally {
      setImageActionId(null);
    }
  };

  return (
    <section className="sandboxPage">
      <div className="panel sandboxHero">
        <div className="panelHeader">
          <div>
            <h2><Shield size={18} />本地保护</h2>
            <p>限制本地任务能访问的文件和网络；所有执行与数据都留在当前电脑。</p>
          </div>
          <div className="headerActions">
            <button className="iconButton" onClick={() => void refreshSandboxDetails()} title="刷新本地保护状态">
              <RefreshCw size={17} />
            </button>
            <label className="switch">
              <input
                type="checkbox"
                checked={status.sandbox.enabled}
                disabled={Boolean(status.sandbox.permission_configuration_error)}
                onChange={(event) => void setEnabled(event.target.checked)}
              />
              <span />
            </label>
          </div>
        </div>
        {status.sandbox.permission_configuration_error ? (
          <div className="formError">
            受管权限策略尚未安全加载，沙箱执行已阻止：{status.sandbox.permission_configuration_error}
          </div>
        ) : null}
        <SandboxPolicySettings
          status={status}
          settings={settings}
          capabilities={capabilities}
          saving={savingSettings}
          onSave={saveSandboxSettings}
        />
        {message ? <div className="banner">{message}</div> : null}
      </div>

      {status.sandbox.enabled ? (
        <details className="panel sandboxAdvancedPanel">
          <summary>
            <span><Settings2 size={16} />高级运行信息</span>
            <small>Docker 镜像、构建任务和当前运行实例</small>
          </summary>
          <div className="sandboxAdvancedContent">
          <div className="sandboxContentGrid">
            <section className="panel">
              <div className="panelHeader">
                <div>
                  <h2><Settings2 size={18} />创建沙箱镜像</h2>
                  <p>选择本机 Docker 镜像内要预装的运行时。</p>
                </div>
                <button
                  className="primaryButton compact"
                  disabled={building || (selectedFeatures.length === 0 && !customScript.trim())}
                  onClick={() => void initialize()}
                >
                  {building ? '创建中' : '创建镜像'}
                </button>
              </div>
              {catalog ? (
                <>
                  <div className="runtimeGrid">
                    {catalog.features.map((feature) => (
                      <label key={feature.id} className="runtimeSelect">
                        <span>
                          <strong>{feature.label}</strong>
                          <small>{feature.description}</small>
                        </span>
                        <select
                          value={features[feature.id] || ''}
                          onChange={(event) => setFeatures((current) => ({
                            ...current,
                            [feature.id]: event.target.value,
                          }))}
                        >
                          <option value="">不安装</option>
                          {feature.versions.map((version) => (
                            <option key={version.id} value={version.id}>
                              {version.label}{version.default ? ' · 推荐' : ''}
                            </option>
                          ))}
                        </select>
                      </label>
                    ))}
                  </div>
                  <label className="scriptEditor">
                    自定义构建脚本
                    <textarea
                      value={customScript}
                      onChange={(event) => setCustomScript(event.target.value)}
                      rows={7}
                      placeholder="apt-get update && apt-get install -y ..."
                    />
                  </label>
                </>
              ) : (
                <div className="emptyState">{loadingDetails ? '正在读取本地镜像配置...' : '暂无镜像配置'}</div>
              )}
            </section>

            <section className="panel">
              <div className="panelHeader">
                <div>
                  <h2><Image size={18} />本地镜像</h2>
                  <p>这些镜像只存在于当前电脑的 Docker 环境。</p>
                </div>
              </div>
              <div className="imageList">
                {(catalog?.images || []).map((image) => (
                  <div className="imageRow" key={image.id}>
                    <div>
                      <strong>{image.image_ref}</strong>
                      <span>{image.features.length ? image.features.join(', ') : 'base'}</span>
                    </div>
                    <div className="imageActions">
                      <span className={image.status === 'local' ? 'status ok' : 'status bad'}>
                        {image.status === 'local' ? (image.id === 'default' ? '默认' : '本机') : '镜像缺失'}
                      </span>
                      {image.rebuildable !== false ? (
                        <button
                          className="ghostButton compact"
                          disabled={imageActionId === image.id}
                          onClick={() => void reinitializeImage(image.id)}
                        >
                          <RotateCcw size={14} />重新初始化
                        </button>
                      ) : null}
                      <button
                        className="iconButton danger"
                        title={image.status === 'local' ? '删除本机镜像' : '清理缺失镜像记录'}
                        disabled={imageActionId === image.id}
                        onClick={() => void deleteImage(image.id, image.image_ref, image.status === 'local')}
                      >
                        <Trash2 size={14} />
                      </button>
                    </div>
                  </div>
                ))}
                {!catalog?.images.length ? <div className="emptyState">还没有读取到本地沙箱镜像。</div> : null}
              </div>
            </section>
          </div>

          <section className="panel sandboxAdvancedInnerPanel">
            <div className="panelHeader">
              <div>
                <h2><ListChecks size={18} />镜像任务</h2>
                <p>构建日志保留在 Local Connector Core 内存中。</p>
              </div>
            </div>
            {jobs.length ? (
              <div className="jobList">
                {jobs.map((job) => (
                  <details className="jobRow" key={job.id} open={job.status === 'running' || Boolean(job.error)}>
                    <summary>
                      <span>
                        <strong>{job.image_name}</strong>
                        <small>{job.features.length ? job.features.join(', ') : 'base'}</small>
                      </span>
                      <span className={job.status === 'succeeded' ? 'status ok' : job.status === 'failed' ? 'status bad' : 'status warn'}>
                        {job.status}
                      </span>
                    </summary>
                    {job.error ? <div className="formError">{job.error}</div> : null}
                    <pre className="logText">{job.output || '暂无日志'}</pre>
                  </details>
                ))}
              </div>
            ) : (
              <div className="emptyState">还没有镜像构建任务。</div>
            )}
          </section>

          <section className="panel">
            <div className="panelHeader">
              <div>
                <h2><Container size={18} />当前沙箱</h2>
                <p>本地任务运行时创建的隔离实例。</p>
              </div>
            </div>
            {leases.length ? (
              <div className="leaseTable">
                <div className="leaseHeader">
                  <span>Sandbox</span>
                  <span>Run</span>
                  <span>Image</span>
                  <span>Status</span>
                </div>
                {leases.map((lease) => (
                  <div className="leaseRow" key={lease.id}>
                    <span className="mono">{lease.sandbox_id}</span>
                    <span className="mono">{lease.run_id}</span>
                    <span>{lease.image_ref || '-'}</span>
                    <span className={lease.status === 'ready' ? 'status ok' : 'status warn'}>{lease.status}</span>
                  </div>
                ))}
              </div>
            ) : (
              <div className="emptyState">当前没有运行中的本地沙箱。</div>
            )}
          </section>
          </div>
        </details>
      ) : (
        <section className="panel">
          <div className="emptyState">打开本地保护后，客户端会自动选择当前电脑可用的安全隔离方式。</div>
        </section>
      )}
    </section>
  );
}
