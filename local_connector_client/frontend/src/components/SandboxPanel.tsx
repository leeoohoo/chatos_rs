// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import React from 'react';
import {
  Activity,
  CloudOff,
  Container,
  Cpu,
  HardDrive,
  Image,
  Layers,
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
  type PermissionProfileId,
  type SandboxApprovalPolicy,
  type SandboxApprovalReviewer,
  type SandboxBackendCapability,
  type SandboxBackendKind,
  type SandboxCapabilities,
  type SandboxImageCatalog,
  type SandboxImageJob,
  type SandboxLease,
  type SandboxSettings,
} from '../api';

type SandboxIcon = typeof Shield;
type SandboxApprovalMode = 'user' | 'auto_review' | 'never';

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
      setMessage(enabled ? '本地沙箱已开启' : '本地沙箱已关闭');
      await Promise.all([refreshSandboxConfig(), onRefresh()]);
    } catch (err) {
      setMessage(err instanceof Error ? err.message : '沙箱设置失败');
    }
  };

  const saveSandboxSettings = async (
    patch: Partial<SandboxSettings> & { risk_acknowledged?: boolean },
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

  const currentBackend = normalizeSandboxBackend(
    settings?.default_backend || status.sandbox.default_backend || status.sandbox.backend,
  );
  const currentPermissionProfile = normalizePermissionProfile(
    settings?.default_permission_profile_id || status.sandbox.default_permission_profile_id,
  );
  const currentApprovalPolicy = normalizeApprovalPolicy(
    settings?.default_approval_policy || status.sandbox.default_approval_policy,
  );
  const currentApprovalReviewer = normalizeApprovalReviewer(
    settings?.default_approval_reviewer || status.sandbox.default_approval_reviewer,
  );
  const currentApprovalMode = approvalModeFromPolicy(
    currentApprovalPolicy,
    currentApprovalReviewer,
  );
  const capabilityByBackend = new Map(
    (capabilities?.backends || []).map((capability) => [capability.backend, capability]),
  );
  const currentCapability = capabilityByBackend.get(currentBackend);
  const currentIsolationDetail = sandboxBackendIsolationDetail(
    currentBackend,
    currentCapability,
    status.sandbox,
  );
  const currentBackendTone = sandboxBackendTone(currentBackend, currentCapability);

  const setDefaultBackend = (backend: SandboxBackendKind) => {
    const capability = capabilityByBackend.get(backend);
    if (capability && !capability.selectable) {
      setMessage(capability.message);
      return;
    }
    if (backend === currentBackend) {
      return;
    }
    void saveSandboxSettings({ default_backend: backend }, '沙箱后端');
  };

  const setPermissionProfile = (profile: PermissionProfileId) => {
    if (profile === currentPermissionProfile) {
      return;
    }
    if (
      profile === 'full_access'
      && !window.confirm('切换到“完全访问”会允许本地沙箱访问更多本机文件。确认继续吗？')
    ) {
      return;
    }
    void saveSandboxSettings(
      { default_permission_profile_id: profile, risk_acknowledged: profile === 'full_access' },
      '权限档位',
    );
  };

  const setApprovalMode = (mode: SandboxApprovalMode) => {
    if (mode === currentApprovalMode) {
      return;
    }
    if (mode === 'never') {
      if (!window.confirm('切换到“从不询问”后，命令将不再弹出用户审批。确认继续吗？')) {
        return;
      }
      void saveSandboxSettings(
        {
          default_approval_policy: 'never',
          default_approval_reviewer: currentApprovalReviewer,
          risk_acknowledged: true,
        },
        '审批方式',
      );
      return;
    }
    if (
      mode === 'auto_review'
      && !window.confirm('切换到“自动审批”后，命令会由本地自动审批策略判断。确认继续吗？')
    ) {
      return;
    }
    void saveSandboxSettings(
      {
        default_approval_policy: 'on_request',
        default_approval_reviewer: mode,
        risk_acknowledged: mode === 'auto_review',
      },
      '审批方式',
    );
  };

  return (
    <section className="sandboxPage">
      <div className="panel sandboxHero">
        <div className="panelHeader">
          <div>
            <h2><Shield size={18} />本地沙箱</h2>
            <p>Local Connector Core 在本机 Docker 内创建、启动和释放沙箱；Local Connector Service 只转发长连接消息。</p>
          </div>
          <div className="headerActions">
            <button className="iconButton" onClick={() => void refreshSandboxDetails()} title="刷新沙箱">
              <RefreshCw size={17} />
            </button>
            <label className="switch">
              <input
                type="checkbox"
                checked={status.sandbox.enabled}
                onChange={(event) => void setEnabled(event.target.checked)}
              />
              <span />
            </label>
          </div>
        </div>
        <div className="sandboxStatusGrid">
          <StatusTile
            icon={Container}
            label="沙箱开关"
            value={status.sandbox.enabled ? '已开启' : '已关闭'}
            tone={status.sandbox.enabled ? 'ok' : 'muted'}
          />
          <StatusTile
            icon={HardDrive}
            label="Docker"
            value={status.docker.installed ? (status.docker.running ? '运行中' : '未运行') : '未安装'}
            detail={status.docker.version || status.docker.error || undefined}
            tone={status.docker.installed && status.docker.running ? 'ok' : 'warn'}
          />
          <StatusTile
            icon={Cpu}
            label="运行后端"
            value={sandboxBackendLabel(currentBackend)}
            detail={currentIsolationDetail}
            tone={currentBackendTone}
          />
          <StatusTile
            icon={Image}
            label="默认镜像"
            value={status.sandbox.selected_image_ref || 'chatos-sandbox-agent:latest'}
            tone="muted"
          />
        </div>
        <div className="sandboxSettingsGrid">
          <div className="sandboxSettingBlock backendPickerBlock">
            <span className="settingLabel">默认后端</span>
            <div className="backendChoiceList">
              {(['local_process', 'docker'] as SandboxBackendKind[]).map((backend) => {
                const capability = capabilityByBackend.get(backend);
                const selectable = capability?.selectable ?? backend === 'docker';
                const active = currentBackend === backend;
                return (
                  <button
                    key={backend}
                    type="button"
                    className={active ? 'backendChoice active' : 'backendChoice'}
                    disabled={savingSettings || !selectable}
                    title={capability?.message}
                    onClick={() => setDefaultBackend(backend)}
                  >
                    <strong>{sandboxBackendLabel(backend)}</strong>
                    <small>{sandboxBackendDetail(backend, capability)}</small>
                  </button>
                );
              })}
            </div>
          </div>
          <label className="sandboxSettingBlock">
            <span className="settingLabel">权限档位</span>
            <select
              value={currentPermissionProfile}
              disabled={savingSettings}
              onChange={(event) => setPermissionProfile(event.target.value as PermissionProfileId)}
            >
              <option value="read_only">只读</option>
              <option value="workspace_write">工作区可写</option>
              <option value="full_access">完全访问</option>
            </select>
          </label>
          <label className="sandboxSettingBlock">
            <span className="settingLabel">审批方式</span>
            <select
              value={currentApprovalMode}
              disabled={savingSettings}
              onChange={(event) => setApprovalMode(event.target.value as SandboxApprovalMode)}
            >
              <option value="user">每次询问</option>
              <option value="auto_review">自动审批</option>
              <option value="never">从不询问</option>
            </select>
          </label>
          <div className="sandboxSettingBlock policyRevisionBlock">
            <span className="settingLabel">策略版本</span>
            <strong>{settings?.policy_revision || status.sandbox.policy_revision || '默认'}</strong>
          </div>
        </div>
        <div className="boundaryList sandboxBoundary">
          <div><CloudOff size={16} />不调用云端 Sandbox Manager，不使用云端沙箱实例。</div>
          <div><Activity size={16} />Task Runner 请求经 Local Connector 长连接转到本机执行。</div>
          <div><Layers size={16} />可复用 common 里的镜像定义和 Dockerfile 生成逻辑，但运行时状态属于本机。</div>
          <div><Shield size={16} />Docker bridge 只提供容器文件系统/进程边界，不声明出站网络隔离。</div>
        </div>
        {message ? <div className="banner">{message}</div> : null}
      </div>

      {status.sandbox.enabled ? (
        <>
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

          <section className="panel">
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
                <p>Task Runner 运行时创建的本机 Docker 沙箱租约。</p>
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
        </>
      ) : (
        <section className="panel">
          <div className="emptyState">本地沙箱默认关闭。打开开关后会检查 Docker，并在本机 Docker 内创建沙箱。</div>
        </section>
      )}
    </section>
  );
}

function normalizeSandboxBackend(value?: string | null): SandboxBackendKind {
  return value === 'local_process' ? 'local_process' : 'docker';
}

function normalizePermissionProfile(value?: string | null): PermissionProfileId {
  if (value === 'read_only' || value === 'full_access') {
    return value;
  }
  return 'workspace_write';
}

function normalizeApprovalPolicy(value?: string | null): SandboxApprovalPolicy {
  return value === 'never' ? 'never' : 'on_request';
}

function normalizeApprovalReviewer(value?: string | null): SandboxApprovalReviewer {
  return value === 'auto_review' ? 'auto_review' : 'user';
}

function approvalModeFromPolicy(
  policy: SandboxApprovalPolicy,
  reviewer: SandboxApprovalReviewer,
): SandboxApprovalMode {
  if (policy === 'never') {
    return 'never';
  }
  return reviewer === 'auto_review' ? 'auto_review' : 'user';
}

function sandboxBackendLabel(backend: SandboxBackendKind) {
  return backend === 'local_process' ? '进程隔离' : 'Docker';
}

function sandboxBackendDetail(
  backend: SandboxBackendKind,
  capability?: SandboxBackendCapability,
) {
  if (capability) {
    return readinessLabel(capability.status);
  }
  return backend === 'local_process' ? '开发中' : '可用';
}

function sandboxBackendIsolationDetail(
  backend: SandboxBackendKind,
  capability: SandboxBackendCapability | undefined,
  sandbox: ConnectorStatus['sandbox'],
) {
  if (sandbox.isolation_note) {
    return sandbox.isolation_note;
  }
  if (capability) {
    const fileBoundary = capability.filesystem_isolation ? '文件系统隔离' : '无文件系统隔离';
    const networkBoundary = capability.network_isolation ? '网络隔离' : '无出站网络隔离';
    return `${fileBoundary} / ${networkBoundary}`;
  }
  return backend === 'local_process'
    ? '进程隔离开发中，暂不可选择'
    : 'Docker 文件系统隔离 / 无出站网络隔离';
}

function sandboxBackendTone(
  backend: SandboxBackendKind,
  capability?: SandboxBackendCapability,
): 'ok' | 'warn' | 'muted' {
  if (capability?.status === 'ready' && capability.selectable) {
    return 'ok';
  }
  if (backend === 'local_process' || capability?.status === 'under_development') {
    return 'warn';
  }
  return capability ? 'warn' : 'muted';
}

function readinessLabel(status: string) {
  const labels: Record<string, string> = {
    ready: '已就绪',
    setup_required: '需要设置',
    unsupported: '不支持',
    under_development: '开发中',
  };
  return labels[status] || status;
}

function StatusTile({
  icon: Icon,
  label,
  value,
  detail,
  tone,
}: {
  icon: SandboxIcon;
  label: string;
  value: string;
  detail?: string;
  tone: 'ok' | 'warn' | 'muted';
}) {
  return (
    <div className={`statusTile ${tone}`}>
      <Icon size={18} />
      <span>{label}</span>
      <strong>{value}</strong>
      {detail ? <small>{detail}</small> : null}
    </div>
  );
}
