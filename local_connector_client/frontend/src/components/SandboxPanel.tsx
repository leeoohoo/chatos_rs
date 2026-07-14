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
  type SandboxImageCatalog,
  type SandboxImageJob,
  type SandboxLease,
} from '../api';

type SandboxIcon = typeof Shield;

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
  const [jobs, setJobs] = React.useState<SandboxImageJob[]>([]);
  const [leases, setLeases] = React.useState<SandboxLease[]>([]);
  const [features, setFeatures] = React.useState<Record<string, string>>({});
  const [customScript, setCustomScript] = React.useState('');
  const [message, setMessage] = React.useState<string | null>(null);
  const [loadingDetails, setLoadingDetails] = React.useState(false);
  const [building, setBuilding] = React.useState(false);
  const [imageActionId, setImageActionId] = React.useState<string | null>(null);

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
      await onRefresh();
    } catch (err) {
      setMessage(err instanceof Error ? err.message : '沙箱设置失败');
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

  const deleteImage = async (imageId: string, imageRef: string) => {
    if (!window.confirm(`确定删除本机 Docker 镜像 ${imageRef} 吗？`)) {
      return;
    }
    setMessage(null);
    setImageActionId(imageId);
    try {
      await api.deleteSandboxImage(imageId);
      setMessage(`镜像已删除: ${imageRef}`);
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
            value={status.sandbox.backend || 'docker'}
            detail={status.sandbox.isolation || 'local_docker'}
            tone="ok"
          />
          <StatusTile
            icon={Image}
            label="默认镜像"
            value={status.sandbox.selected_image_ref || 'chatos-sandbox-agent:latest'}
            tone="muted"
          />
        </div>
        <div className="boundaryList sandboxBoundary">
          <div><CloudOff size={16} />不调用云端 Sandbox Manager，不使用云端沙箱实例。</div>
          <div><Activity size={16} />Task Runner 请求经 Local Connector 长连接转到本机执行。</div>
          <div><Layers size={16} />可复用 common 里的镜像定义和 Dockerfile 生成逻辑，但运行时状态属于本机。</div>
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
                      {image.status !== 'local' && image.rebuildable !== false ? (
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
                        title="删除本机镜像"
                        disabled={imageActionId === image.id || image.status !== 'local'}
                        onClick={() => void deleteImage(image.id, image.image_ref)}
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
