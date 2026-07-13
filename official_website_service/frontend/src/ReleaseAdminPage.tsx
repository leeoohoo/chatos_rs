// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { type FormEvent, useEffect, useMemo, useState } from 'react';
import { sha256 } from '@noble/hashes/sha2.js';
import { bytesToHex } from '@noble/hashes/utils.js';
import {
  AlertCircle,
  ArrowLeft,
  Check,
  FileArchive,
  KeyRound,
  LoaderCircle,
  PackageCheck,
  Plus,
  RefreshCw,
  ShieldCheck,
  Trash2,
  UploadCloud,
} from 'lucide-react';
import { BrandMark } from './BrandMark';

type ClientArtifact = {
  platform: string;
  label: string;
  file_name: string;
  content_type: string;
  size_bytes: number;
  sha256: string;
  download_url: string;
};

type ClientRelease = {
  product: string;
  channel: string;
  version: string;
  published_at: string;
  artifacts: ClientArtifact[];
};

type DownloadCatalog = {
  storage_configured: boolean;
  available: boolean;
  message: string;
  release?: ClientRelease | null;
};

type ArtifactDraft = {
  id: string;
  platform: string;
  label: string;
  file: File | null;
};

type PresignResponse = {
  manifest: Omit<ClientRelease, 'artifacts'> & {
    artifacts: Array<Omit<ClientArtifact, 'download_url'>>;
  };
  artifact_uploads: Array<{
    platform: string;
    object_key: string;
    upload_url: string;
  }>;
  manifest_upload: {
    object_key: string;
    upload_url: string;
  };
};

type PublishPhase = 'idle' | 'hashing' | 'authorizing' | 'uploading' | 'publishing' | 'success' | 'error';

const TOKEN_SESSION_KEY = 'chat-os-release-admin-token';
const platformOptions = [
  { value: 'windows-x64', label: 'Windows 10/11 · x64' },
  { value: 'windows-arm64', label: 'Windows 11 · ARM64' },
  { value: 'macos-arm64', label: 'macOS · Apple Silicon' },
  { value: 'macos-x64', label: 'macOS · Intel' },
  { value: 'linux-x64', label: 'Linux · x64' },
];

function newArtifact(index = 0): ArtifactDraft {
  const option = platformOptions[Math.min(index, platformOptions.length - 1)];
  return {
    id: crypto.randomUUID(),
    platform: option.value,
    label: option.label,
    file: null,
  };
}

function ReleaseAdminPage() {
  const [token, setToken] = useState(() => window.sessionStorage.getItem(TOKEN_SESSION_KEY) ?? '');
  const [rememberToken, setRememberToken] = useState(Boolean(window.sessionStorage.getItem(TOKEN_SESSION_KEY)));
  const [version, setVersion] = useState('');
  const [artifacts, setArtifacts] = useState<ArtifactDraft[]>([newArtifact()]);
  const [catalog, setCatalog] = useState<DownloadCatalog | null>(null);
  const [catalogLoading, setCatalogLoading] = useState(true);
  const [phase, setPhase] = useState<PublishPhase>('idle');
  const [statusText, setStatusText] = useState('等待填写发布信息');
  const [progress, setProgress] = useState(0);

  const busy = !['idle', 'success', 'error'].includes(phase);
  const selectedBytes = useMemo(
    () => artifacts.reduce((total, artifact) => total + (artifact.file?.size ?? 0), 0),
    [artifacts],
  );

  useEffect(() => {
    const previousTitle = document.title;
    const existingRobots = document.querySelector<HTMLMetaElement>('meta[name="robots"]');
    const robots = existingRobots ?? document.createElement('meta');
    const previousRobots = existingRobots?.content;
    if (!existingRobots) {
      robots.name = 'robots';
      document.head.appendChild(robots);
    }
    robots.content = 'noindex,nofollow';
    document.title = '安装包发布管理 | Okra';
    void refreshCatalog();
    return () => {
      document.title = previousTitle;
      if (existingRobots) robots.content = previousRobots ?? 'index,follow';
      else robots.remove();
    };
  }, []);

  const refreshCatalog = async () => {
    setCatalogLoading(true);
    try {
      const response = await fetch('/api/site/downloads', { cache: 'no-store' });
      if (!response.ok) throw new Error('无法读取当前发布版本');
      setCatalog(await response.json() as DownloadCatalog);
    } catch (error) {
      setCatalog({
        storage_configured: false,
        available: false,
        message: error instanceof Error ? error.message : String(error),
      });
    } finally {
      setCatalogLoading(false);
    }
  };

  const updateArtifact = (id: string, patch: Partial<ArtifactDraft>) => {
    setArtifacts((current) => current.map((artifact) => (
      artifact.id === id ? { ...artifact, ...patch } : artifact
    )));
    if (phase === 'error' || phase === 'success') setPhase('idle');
  };

  const changePlatform = (artifact: ArtifactDraft, platform: string) => {
    const option = platformOptions.find((item) => item.value === platform);
    updateArtifact(artifact.id, {
      platform,
      label: option?.label ?? artifact.label,
    });
  };

  const removeArtifact = (id: string) => {
    setArtifacts((current) => current.length === 1 ? current : current.filter((item) => item.id !== id));
  };

  const publishRelease = async (event: FormEvent) => {
    event.preventDefault();
    setProgress(0);
    try {
      validateRelease(version, token, artifacts);
      if (rememberToken) {
        window.sessionStorage.setItem(TOKEN_SESSION_KEY, token.trim());
      } else {
        window.sessionStorage.removeItem(TOKEN_SESSION_KEY);
      }

      setPhase('hashing');
      const artifactMetadata = [];
      for (let index = 0; index < artifacts.length; index += 1) {
        const artifact = artifacts[index];
        const file = artifact.file as File;
        setStatusText(`正在校验 ${file.name} 的 SHA-256`);
        const digest = await hashFile(file, (fileProgress) => {
          setProgress(((index + fileProgress) / artifacts.length) * 32);
        });
        artifactMetadata.push({
          platform: artifact.platform,
          label: artifact.label.trim(),
          file_name: file.name,
          content_type: file.type || 'application/octet-stream',
          size_bytes: file.size,
          sha256: digest,
        });
      }

      setPhase('authorizing');
      setStatusText('正在获取受保护的 MinIO 上传地址');
      setProgress(35);
      const response = await fetch('/api/site/admin/releases/presign', {
        method: 'POST',
        headers: {
          authorization: `Bearer ${token.trim()}`,
          'content-type': 'application/json',
        },
        body: JSON.stringify({ version: version.trim(), artifacts: artifactMetadata }),
      });
      const payload = await readJson<PresignResponse & { error?: string }>(response);
      if (!response.ok) throw new Error(payload.error || '无法获取安装包上传地址');

      setPhase('uploading');
      for (let index = 0; index < artifacts.length; index += 1) {
        const artifact = artifacts[index];
        const file = artifact.file as File;
        const upload = payload.artifact_uploads.find((item) => item.platform === artifact.platform);
        if (!upload) throw new Error(`没有找到 ${artifact.label} 的上传地址`);
        setStatusText(`正在上传 ${file.name}`);
        await uploadFile(upload.upload_url, file, (fileProgress) => {
          setProgress(38 + ((index + fileProgress) / artifacts.length) * 54);
        });
      }

      setPhase('publishing');
      setStatusText('安装包已上传，正在发布最新版本清单');
      setProgress(94);
      const manifestResponse = await fetch(payload.manifest_upload.upload_url, {
        method: 'PUT',
        headers: { 'content-type': 'application/json; charset=utf-8' },
        body: JSON.stringify(payload.manifest),
      });
      if (!manifestResponse.ok) {
        throw new Error(`版本清单上传失败（HTTP ${manifestResponse.status}）`);
      }

      setProgress(100);
      setPhase('success');
      setStatusText(`Okra Local Connector ${payload.manifest.version} 已发布`);
      await refreshCatalog();
    } catch (error) {
      setPhase('error');
      setStatusText(toAdminError(error));
    }
  };

  return (
    <main className="admin-shell">
      <header className="admin-header">
        <a className="brand" href="/" aria-label="返回 Okra 官网">
          <BrandMark />
          <span>Okra</span>
        </a>
        <span className="admin-header-title">官方管理后台</span>
        <a className="admin-back-link" href="/"><ArrowLeft size={16} /> 返回官网</a>
      </header>

      <section className="admin-hero">
        <div>
          <span className="admin-eyebrow"><PackageCheck size={16} /> Release Center</span>
          <h1>安装包发布管理</h1>
          <p>上传桌面连接器安装包、生成版本清单，并把最新稳定版安全地发布到官网。</p>
        </div>
        <div className="admin-security-note">
          <ShieldCheck size={21} />
          <div><strong>MinIO 密钥不会进入浏览器</strong><span>页面只使用短期预签名地址，发布令牌仅用于申请上传权限。</span></div>
        </div>
      </section>

      <section className="admin-layout">
        <form className="admin-card release-form" onSubmit={publishRelease}>
          <div className="admin-card-heading">
            <div><span>新版本</span><h2>创建一次客户端发布</h2></div>
            <span className="admin-step">1 / 3</span>
          </div>

          <div className="admin-field-grid">
            <label>
              <span>版本号</span>
              <input value={version} onChange={(event) => setVersion(event.target.value)} placeholder="例如 2.0.5" disabled={busy} required />
              <small>只允许字母、数字、点、横线和下划线。</small>
            </label>
            <label>
              <span>发布令牌</span>
              <span className="admin-token-input"><KeyRound size={17} /><input type="password" value={token} onChange={(event) => setToken(event.target.value)} placeholder="OFFICIAL_WEBSITE_RELEASE_UPLOAD_TOKEN" disabled={busy} required /></span>
              <small>令牌由部署环境配置，不会写入发布清单。</small>
            </label>
          </div>

          <label className="admin-check-row">
            <input type="checkbox" checked={rememberToken} onChange={(event) => setRememberToken(event.target.checked)} disabled={busy} />
            <span>在当前标签页会话中记住发布令牌</span>
          </label>

          <div className="artifact-heading">
            <div><span>安装包</span><small>每个平台选择一个制品，发布时会统一生成清单。</small></div>
            <button className="admin-text-button" type="button" onClick={() => setArtifacts((current) => [...current, newArtifact(current.length)])} disabled={busy || artifacts.length >= 5}><Plus size={16} /> 添加平台</button>
          </div>

          <div className="artifact-list">
            {artifacts.map((artifact, index) => (
              <article className="artifact-row" key={artifact.id}>
                <span className="artifact-number">{index + 1}</span>
                <div className="artifact-fields">
                  <label><span>平台</span><select value={artifact.platform} onChange={(event) => changePlatform(artifact, event.target.value)} disabled={busy}>{platformOptions.map((option) => <option value={option.value} key={option.value}>{option.label}</option>)}</select></label>
                  <label><span>展示名称</span><input value={artifact.label} onChange={(event) => updateArtifact(artifact.id, { label: event.target.value })} disabled={busy} required /></label>
                  <label className="artifact-file-field">
                    <span>安装包文件</span>
                    <input type="file" accept=".zip,.exe,.msi,.dmg,.pkg,.AppImage,.deb,.rpm" onChange={(event) => updateArtifact(artifact.id, { file: event.target.files?.[0] ?? null })} disabled={busy} required />
                    <small>{artifact.file ? `${artifact.file.name} · ${formatBytes(artifact.file.size)}` : '选择构建完成的安装包或压缩包'}</small>
                  </label>
                </div>
                <button className="artifact-remove" type="button" onClick={() => removeArtifact(artifact.id)} disabled={busy || artifacts.length === 1} aria-label="移除安装包"><Trash2 size={17} /></button>
              </article>
            ))}
          </div>

          <div className={`publish-status publish-status-${phase}`}>
            <span className="publish-status-icon">
              {busy ? <LoaderCircle className="spin" size={20} /> : phase === 'success' ? <Check size={20} /> : phase === 'error' ? <AlertCircle size={20} /> : <UploadCloud size={20} />}
            </span>
            <div><strong>{statusText}</strong><small>{selectedBytes > 0 ? `本次共 ${formatBytes(selectedBytes)}` : '选择文件后会在浏览器中计算 SHA-256'}</small></div>
            <span className="publish-progress-value">{Math.round(progress)}%</span>
            <span className="publish-progress-track"><i style={{ width: `${progress}%` }} /></span>
          </div>

          <button className="button button-primary admin-publish-button" type="submit" disabled={busy}>
            {busy ? <><LoaderCircle className="spin" size={18} /> 正在发布</> : <><UploadCloud size={18} /> 校验并发布安装包</>}
          </button>
          <p className="admin-form-footnote">安装包全部上传成功后才会写入 `latest.json`，因此上传中断不会替换官网当前版本。</p>
        </form>

        <aside className="admin-sidebar">
          <section className="admin-card current-release-card">
            <div className="admin-card-heading compact">
              <div><span>线上状态</span><h2>当前官网版本</h2></div>
              <button className="icon-button" type="button" onClick={() => void refreshCatalog()} disabled={catalogLoading} aria-label="刷新版本"><RefreshCw className={catalogLoading ? 'spin' : ''} size={17} /></button>
            </div>
            {catalogLoading ? (
              <div className="admin-empty"><LoaderCircle className="spin" size={23} /><span>正在读取版本目录</span></div>
            ) : catalog?.release ? (
              <div className="release-summary">
                <div className="release-version"><span>稳定版</span><strong>{catalog.release.version}</strong></div>
                <dl><div><dt>发布时间</dt><dd>{formatDate(catalog.release.published_at)}</dd></div><div><dt>发布通道</dt><dd>{catalog.release.channel}</dd></div><div><dt>安装包</dt><dd>{catalog.release.artifacts.length} 个</dd></div></dl>
                <ul>{catalog.release.artifacts.map((artifact) => <li key={artifact.platform}><FileArchive size={16} /><span><strong>{artifact.label}</strong><small>{artifact.file_name} · {formatBytes(artifact.size_bytes)}</small></span></li>)}</ul>
              </div>
            ) : (
              <div className="admin-empty"><AlertCircle size={23} /><strong>暂无可下载版本</strong><span>{catalog?.message ?? '对象存储尚未配置'}</span></div>
            )}
          </section>

          <section className="admin-card publish-guide-card">
            <span className="admin-card-kicker">发布顺序</span>
            <ol><li><span>1</span><div><strong>本地校验</strong><small>分块计算每个文件的 SHA-256。</small></div></li><li><span>2</span><div><strong>上传制品</strong><small>通过短期预签名 URL 直传 MinIO。</small></div></li><li><span>3</span><div><strong>切换版本</strong><small>最后上传版本清单，官网立即生效。</small></div></li></ol>
          </section>
        </aside>
      </section>
    </main>
  );
}

function validateRelease(version: string, token: string, artifacts: ArtifactDraft[]) {
  if (!/^[A-Za-z0-9._-]+$/.test(version.trim())) throw new Error('请填写有效的版本号。');
  if (!token.trim()) throw new Error('请填写发布令牌。');
  if (artifacts.some((artifact) => !artifact.file)) throw new Error('请为每个平台选择安装包文件。');
  if (artifacts.some((artifact) => !artifact.label.trim())) throw new Error('请填写所有安装包的展示名称。');
  const platforms = artifacts.map((artifact) => artifact.platform);
  if (new Set(platforms).size !== platforms.length) throw new Error('同一次发布不能重复选择同一个平台。');
}

async function hashFile(file: File, onProgress: (progress: number) => void) {
  const hasher = sha256.create();
  const chunkSize = 8 * 1024 * 1024;
  for (let offset = 0; offset < file.size; offset += chunkSize) {
    const chunk = new Uint8Array(await file.slice(offset, offset + chunkSize).arrayBuffer());
    hasher.update(chunk);
    onProgress(Math.min(1, (offset + chunk.byteLength) / file.size));
    await new Promise((resolve) => window.setTimeout(resolve, 0));
  }
  return bytesToHex(hasher.digest());
}

function uploadFile(url: string, file: File, onProgress: (progress: number) => void) {
  return new Promise<void>((resolve, reject) => {
    const request = new XMLHttpRequest();
    request.open('PUT', url);
    request.setRequestHeader('content-type', file.type || 'application/octet-stream');
    request.upload.addEventListener('progress', (event) => {
      if (event.lengthComputable) onProgress(event.loaded / event.total);
    });
    request.addEventListener('load', () => {
      if (request.status >= 200 && request.status < 300) resolve();
      else reject(new Error(`${file.name} 上传失败（HTTP ${request.status}）`));
    });
    request.addEventListener('error', () => reject(new Error(`${file.name} 上传失败，请检查 MinIO CORS 和网络配置。`)));
    request.send(file);
  });
}

async function readJson<T>(response: Response): Promise<T> {
  const text = await response.text();
  if (!text) return {} as T;
  try {
    return JSON.parse(text) as T;
  } catch {
    throw new Error(`服务返回了无法解析的响应（HTTP ${response.status}）`);
  }
}

function formatBytes(value: number) {
  if (!Number.isFinite(value) || value <= 0) return '大小未知';
  const units = ['B', 'KB', 'MB', 'GB'];
  let size = value;
  let unit = 0;
  while (size >= 1024 && unit < units.length - 1) {
    size /= 1024;
    unit += 1;
  }
  return `${size.toFixed(unit === 0 ? 0 : 1)} ${units[unit]}`;
}

function formatDate(value: string) {
  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? value : date.toLocaleString('zh-CN', { hour12: false });
}

function toAdminError(error: unknown) {
  const message = error instanceof Error ? error.message : String(error);
  if (message.includes('invalid release upload token')) return '发布令牌无效，请检查部署配置。';
  if (message.includes('release storage is not configured')) return 'MinIO / S3 发布存储尚未配置。';
  if (message.includes('Failed to fetch')) return '请求失败，请检查官网 API、MinIO CORS 和网络连接。';
  return message;
}

export default ReleaseAdminPage;
