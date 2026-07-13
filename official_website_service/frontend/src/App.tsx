// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { type FormEvent, useEffect, useMemo, useState } from 'react';
import {
  ArrowRight,
  Bot,
  BrainCircuit,
  Check,
  ChevronRight,
  Cloud,
  Code2,
  Download,
  FolderLock,
  Laptop,
  Mail,
  MessageCircle,
  MonitorDown,
  ShieldCheck,
  Sparkles,
  TerminalSquare,
  Workflow,
  Zap,
} from 'lucide-react';
import { BrandMark } from './BrandMark';

type SiteManifest = {
  product_name: string;
  tagline: string;
  app_url: string;
  registration_enabled: boolean;
  downloads_enabled: boolean;
};

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

type RegistrationForm = {
  email: string;
  displayName: string;
  inviteCode: string;
  verificationCode: string;
  password: string;
  confirmPassword: string;
};

const fallbackManifest: SiteManifest = {
  product_name: 'Okra',
  tagline: '给你的项目一位真正能动手的 AI 搭档。',
  app_url: 'http://localhost:8088',
  registration_enabled: true,
  downloads_enabled: false,
};

const initialRegistration: RegistrationForm = {
  email: '',
  displayName: '',
  inviteCode: '',
  verificationCode: '',
  password: '',
  confirmPassword: '',
};

const outcomes = [
  {
    icon: BrainCircuit,
    title: '记得项目来龙去脉',
    body: '重要决定、项目背景和你的偏好会被持续整理，下次回来不必重新解释。',
  },
  {
    icon: Code2,
    title: '不只回答，还能动手',
    body: '让 AI 读取代码、运行命令、拆解任务并持续汇报进度，把建议真正变成结果。',
  },
  {
    icon: FolderLock,
    title: '工作区由你授权',
    body: '通过桌面连接器只开放指定目录，也可以选择云端隔离沙箱，边界清晰可控。',
  },
];

const workflowSteps = [
  {
    number: '01',
    title: '告诉 Okra 你想完成什么',
    body: '从一句自然语言开始，讨论需求、约束和验收目标。',
  },
  {
    number: '02',
    title: '确认计划与工作环境',
    body: '选择云端沙箱或已授权的本机项目，复杂任务会进入可追踪的后台执行。',
  },
  {
    number: '03',
    title: '随时回来查看结果',
    body: '进度、工具输出、代码变更和后续问题都保留在同一个协作上下文中。',
  },
];

const useCases = [
  {
    icon: Workflow,
    eyebrow: '从想法到任务',
    title: '把模糊需求变成可执行计划',
    body: '一起澄清目标、拆分步骤和依赖，再把长耗时工作交给后台任务持续推进。',
    points: ['需求澄清与方案比较', '任务拆解与依赖管理', '进度与结果集中回看'],
  },
  {
    icon: TerminalSquare,
    eyebrow: '真实工程环境',
    title: '让 AI 进入项目，而不是复制粘贴代码',
    body: '连接本机工作区或使用云端沙箱，让文件、终端、Git 和工具调用发生在正确的环境里。',
    points: ['明确授权的本机目录', 'Docker 隔离运行环境', '命令输出与改动可追踪'],
  },
  {
    icon: MessageCircle,
    eyebrow: '长期协作',
    title: '今天的讨论，明天还能接着做',
    body: 'Okra 会把消息、摘要、项目事实和长期记忆组织起来，让合作不被单次会话切断。',
    points: ['跨会话上下文', '项目与角色记忆', '重要信息按需召回'],
  },
];

const faqs = [
  {
    question: 'Okra 和普通 AI 聊天工具有什么不同？',
    answer: 'Okra 面向持续的工程协作。它不仅生成回答，还能连接项目环境、运行工具、跟踪后台任务，并在后续协作中继续使用已经沉淀的项目上下文。',
  },
  {
    question: '必须把代码上传到云端吗？',
    answer: '不需要。安装桌面连接器后，你可以只授权本机的指定工作区。云端通过连接器转发请求，不会保存你的本机绝对路径，也不会直接访问你的 localhost。',
  },
  {
    question: '为什么注册需要邀请码？',
    answer: '当前处于邀请测试阶段，我们希望控制服务容量并及时处理反馈。获得邀请码后，可以在官网直接完成邮箱验证和账号注册。',
  },
  {
    question: '桌面连接器支持哪些系统？',
    answer: '当前首先提供 Windows 10/11 64 位版本。macOS 和 Linux 客户端会在后续版本中提供。',
  },
];

function App() {
  const [manifest, setManifest] = useState<SiteManifest>(fallbackManifest);
  const [downloads, setDownloads] = useState<DownloadCatalog | null>(null);
  const [registration, setRegistration] = useState<RegistrationForm>(initialRegistration);
  const [registrationState, setRegistrationState] = useState<'idle' | 'submitting' | 'success'>('idle');
  const [codeSending, setCodeSending] = useState(false);
  const [codeCountdown, setCodeCountdown] = useState(0);
  const [formMessage, setFormMessage] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    Promise.all([
      fetch('/api/site/manifest').then((response) => response.ok ? response.json() as Promise<SiteManifest> : fallbackManifest),
      fetch('/api/site/downloads').then((response) => response.ok ? response.json() as Promise<DownloadCatalog> : null),
    ]).then(([manifestPayload, downloadPayload]) => {
      if (!cancelled) {
        setManifest(manifestPayload);
        setDownloads(downloadPayload);
      }
    }).catch(() => {
      if (!cancelled) {
        setManifest(fallbackManifest);
      }
    });
    return () => {
      cancelled = true;
    };
  }, []);

  useEffect(() => {
    if (codeCountdown <= 0) return undefined;
    const timer = window.setInterval(() => {
      setCodeCountdown((value) => Math.max(0, value - 1));
    }, 1000);
    return () => window.clearInterval(timer);
  }, [codeCountdown]);

  const windowsArtifact = useMemo(
    () => downloads?.release?.artifacts.find((artifact) => artifact.platform === 'windows-x64') ?? null,
    [downloads],
  );

  const updateRegistration = (field: keyof RegistrationForm, value: string) => {
    setRegistration((current) => ({ ...current, [field]: value }));
    setFormMessage(null);
  };

  const sendCode = async () => {
    if (!registration.email.trim()) {
      setFormMessage('请先填写邮箱地址。');
      return;
    }
    if (!registration.inviteCode.trim()) {
      setFormMessage('请先填写邀请码。');
      return;
    }
    setCodeSending(true);
    setFormMessage(null);
    try {
      const response = await fetch('/api/site/auth/register/send-code', {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify({
          email: registration.email.trim(),
          invite_code: registration.inviteCode.trim(),
        }),
      });
      const payload = await response.json() as { error?: string; resend_after_seconds?: number };
      if (!response.ok) throw new Error(payload.error || '验证码发送失败');
      setCodeCountdown(payload.resend_after_seconds ?? 60);
      setFormMessage('验证码已发送，请查看邮箱。');
    } catch (error) {
      setFormMessage(toFriendlyRegistrationError(error));
    } finally {
      setCodeSending(false);
    }
  };

  const submitRegistration = async (event: FormEvent) => {
    event.preventDefault();
    setFormMessage(null);
    if (registration.password.length < 6) {
      setFormMessage('密码至少需要 6 个字符。');
      return;
    }
    if (registration.password !== registration.confirmPassword) {
      setFormMessage('两次输入的密码不一致。');
      return;
    }
    setRegistrationState('submitting');
    try {
      const response = await fetch('/api/site/auth/register', {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify({
          email: registration.email.trim(),
          display_name: registration.displayName.trim() || null,
          password: registration.password,
          invite_code: registration.inviteCode.trim(),
          verification_code: registration.verificationCode.trim(),
        }),
      });
      const payload = await response.json() as { error?: string };
      if (!response.ok) throw new Error(payload.error || '注册失败');
      setRegistrationState('success');
      setRegistration(initialRegistration);
      setFormMessage(null);
    } catch (error) {
      setRegistrationState('idle');
      setFormMessage(toFriendlyRegistrationError(error));
    }
  };

  return (
    <main id="top">
      <header className="site-header">
        <a className="brand" href="#top" aria-label="Okra 首页">
          <BrandMark />
          <span>{manifest.product_name}</span>
        </a>
        <nav className="nav-links" aria-label="主导航">
          <a href="#why">为什么选择</a>
          <a href="#how">如何使用</a>
          <a href="#download">下载客户端</a>
          <a href="#register">注册</a>
        </nav>
        <a className="header-login" href={manifest.app_url}>登录 Okra <ArrowRight size={15} /></a>
      </header>

      <section className="hero-section">
        <div className="hero-glow hero-glow-one" />
        <div className="hero-glow hero-glow-two" />
        <div className="hero-copy">
          <div className="announcement"><Sparkles size={15} /> 面向真实项目的 AI 工作伙伴</div>
          <h1><span className="hero-line">让 AI 进入项目，</span><span className="hero-highlight">把事情做完。</span></h1>
          <p>{manifest.tagline} 它能记住背景、理解代码、使用工具，并在你离开页面后继续推进复杂任务。</p>
          <div className="hero-actions">
            <a className="button button-primary" href="#register">免费注册 <ChevronRight size={17} /></a>
            <a className="button button-secondary" href="#download"><Download size={17} /> 下载桌面连接器</a>
          </div>
          <div className="hero-assurances">
            <span><Check size={15} /> 邀请测试期间免费</span>
            <span><Check size={15} /> 支持本机工作区</span>
            <span><Check size={15} /> 云端沙箱可选</span>
          </div>
        </div>

        <div className="hero-product" aria-label="Okra 产品界面预览">
          <div className="product-window">
            <div className="window-topbar">
              <span className="window-dots"><i /><i /><i /></span>
              <span>Okra · 项目协作空间</span>
              <span className="window-secure"><ShieldCheck size={14} /> 已连接</span>
            </div>
            <img src="/showcase/chatos-main.png" alt="Okra 主界面" />
          </div>
          <div className="floating-card floating-task">
            <span className="floating-icon"><Zap size={17} /></span>
            <span><strong>任务正在后台执行</strong><small>已完成 7 / 10 个步骤</small></span>
          </div>
          <div className="floating-card floating-memory">
            <span className="floating-icon accent"><BrainCircuit size={17} /></span>
            <span><strong>项目上下文已同步</strong><small>下次回来继续工作</small></span>
          </div>
        </div>
      </section>

      <section className="trust-strip" aria-label="核心能力">
        <span><BrainCircuit size={20} /> 长期项目记忆</span>
        <span><TerminalSquare size={20} /> 真实工具执行</span>
        <span><Cloud size={20} /> 云端与本机环境</span>
        <span><ShieldCheck size={20} /> 明确授权边界</span>
      </section>

      <section className="section" id="why">
        <div className="section-heading">
          <span className="section-kicker">不只是聊天</span>
          <h2>一位能理解上下文、进入环境并持续工作的 AI 搭档</h2>
          <p>你不需要围绕工具重新组织工作。Okra 把对话、记忆、任务和执行环境连接起来，让协作自然发生。</p>
        </div>
        <div className="outcome-grid">
          {outcomes.map((item) => {
            const Icon = item.icon;
            return (
              <article className="outcome-card" key={item.title}>
                <span className="card-icon"><Icon size={24} /></span>
                <h3>{item.title}</h3>
                <p>{item.body}</p>
              </article>
            );
          })}
        </div>
      </section>

      <section className="section workflow-section" id="how">
        <div className="section-heading light-heading">
          <span className="section-kicker">三步开始</span>
          <h2>从一句话开始，把工作交给可靠的执行链路</h2>
          <p>简单问题即时回答，复杂任务进入可观察的后台执行；你始终知道它正在做什么。</p>
        </div>
        <div className="workflow-grid">
          {workflowSteps.map((step) => (
            <article className="workflow-card" key={step.number}>
              <span>{step.number}</span>
              <h3>{step.title}</h3>
              <p>{step.body}</p>
            </article>
          ))}
        </div>
      </section>

      <section className="section use-cases">
        <div className="section-heading">
          <span className="section-kicker">为工程协作而生</span>
          <h2>从讨论到交付，始终在同一个上下文里</h2>
        </div>
        <div className="use-case-list">
          {useCases.map((item, index) => {
            const Icon = item.icon;
            return (
              <article className={`use-case-card use-case-${index + 1}`} key={item.title}>
                <div>
                  <span className="case-icon"><Icon size={25} /></span>
                  <small>{item.eyebrow}</small>
                  <h3>{item.title}</h3>
                  <p>{item.body}</p>
                </div>
                <ul>{item.points.map((point) => <li key={point}><Check size={15} /> {point}</li>)}</ul>
              </article>
            );
          })}
        </div>
      </section>

      <section className="section download-section" id="download">
        <div className="download-copy">
          <span className="section-kicker">Okra Local Connector</span>
          <h2>让 Okra 安全地使用你的本机项目</h2>
          <p>桌面连接器只访问你明确授权的目录。连接建立后，Okra 可以在正确的工作区里读取文件、运行终端和管理本机沙箱。</p>
          <div className="connector-points">
            <span><ShieldCheck size={18} /> 云端不保存本机绝对路径</span>
            <span><Laptop size={18} /> 连接器主动建立出站连接</span>
            <span><FolderLock size={18} /> 每个工作区单独授权</span>
          </div>
        </div>
        <div className="download-card">
          <div className="download-platform">
            <span className="platform-icon"><MonitorDown size={28} /></span>
            <div><strong>Windows 客户端</strong><small>Windows 10 / 11 · 64 位</small></div>
          </div>
          {windowsArtifact ? (
            <>
              <a className="button button-primary download-button" href={windowsArtifact.download_url}>
                <Download size={18} /> 下载 {downloads?.release?.version}
              </a>
              <div className="release-meta">
                <span>{formatBytes(windowsArtifact.size_bytes)}</span>
                <span>SHA-256 {windowsArtifact.sha256.slice(0, 12)}…</span>
              </div>
            </>
          ) : (
            <>
              <button className="button button-muted download-button" type="button" disabled>
                <Download size={18} /> {downloads?.message ?? '正在读取最新版本'}
              </button>
              <div className="release-meta"><span>Windows 版本即将开放下载</span></div>
            </>
          )}
          <ol className="install-steps">
            <li><span>1</span> 下载并解压客户端</li>
            <li><span>2</span> 登录你的 Okra 账号</li>
            <li><span>3</span> 选择并授权项目目录</li>
          </ol>
          <p className="coming-soon">macOS 与 Linux 客户端正在准备中</p>
        </div>
      </section>

      <section className="section register-section" id="register">
        <div className="register-copy">
          <span className="section-kicker">开始使用</span>
          <h2>创建账号，和你的 AI 搭档开始第一个项目</h2>
          <p>注册后可以直接进入 Okra。当前为邀请测试阶段，需要邀请码和邮箱验证。</p>
          <div className="register-benefits">
            <span><Check size={16} /> 一个账号连接 Web 与桌面连接器</span>
            <span><Check size={16} /> 模型与项目配置随账号同步</span>
            <span><Check size={16} /> 注册完成后即可登录主应用</span>
          </div>
        </div>

        <div className="register-card">
          {registrationState === 'success' ? (
            <div className="registration-success">
              <span><Check size={28} /></span>
              <h3>账号创建成功</h3>
              <p>现在可以打开 Okra，使用邮箱和密码登录。</p>
              <a className="button button-primary" href={manifest.app_url}>打开 Okra <ArrowRight size={17} /></a>
            </div>
          ) : (
            <form onSubmit={submitRegistration}>
              <div className="form-heading"><Mail size={21} /><div><strong>注册 Okra</strong><small>注册信息由 Okra 账号服务处理</small></div></div>
              <label>邮箱<input type="email" value={registration.email} onChange={(event) => updateRegistration('email', event.target.value)} placeholder="you@example.com" autoComplete="email" required /></label>
              <label>昵称（选填）<input value={registration.displayName} onChange={(event) => updateRegistration('displayName', event.target.value)} placeholder="希望我们怎么称呼你" autoComplete="name" /></label>
              <label>邀请码<input value={registration.inviteCode} onChange={(event) => updateRegistration('inviteCode', event.target.value)} placeholder="输入邀请测试码" required /></label>
              <label>邮箱验证码<span className="code-field"><input inputMode="numeric" value={registration.verificationCode} onChange={(event) => updateRegistration('verificationCode', event.target.value)} placeholder="6 位验证码" required /><button type="button" onClick={() => void sendCode()} disabled={codeSending || codeCountdown > 0}>{codeSending ? '发送中' : codeCountdown > 0 ? `${codeCountdown}s` : '发送验证码'}</button></span></label>
              <div className="password-row">
                <label>密码<input type="password" value={registration.password} onChange={(event) => updateRegistration('password', event.target.value)} placeholder="至少 6 个字符" autoComplete="new-password" required /></label>
                <label>确认密码<input type="password" value={registration.confirmPassword} onChange={(event) => updateRegistration('confirmPassword', event.target.value)} placeholder="再次输入密码" autoComplete="new-password" required /></label>
              </div>
              {formMessage && <div className="form-message">{formMessage}</div>}
              <button className="button button-primary form-submit" type="submit" disabled={registrationState === 'submitting' || !manifest.registration_enabled}>
                {registrationState === 'submitting' ? '正在创建账号…' : '创建账号'} <ArrowRight size={17} />
              </button>
              <p className="form-legal">注册即表示你同意在邀请测试期间遵守平台使用规则与隐私约定。</p>
            </form>
          )}
        </div>
      </section>

      <section className="section faq-section">
        <div className="section-heading"><span className="section-kicker">常见问题</span><h2>开始前，你可能还想了解这些</h2></div>
        <div className="faq-list">
          {faqs.map((item) => <details key={item.question}><summary>{item.question}<ChevronRight size={18} /></summary><p>{item.answer}</p></details>)}
        </div>
      </section>

      <section className="final-cta">
        <span className="cta-icon"><Bot size={30} /></span>
        <div><h2>准备好让 AI 真正参与项目了吗？</h2><p>从注册开始，建立一段可以持续推进工作的协作关系。</p></div>
        <a className="button button-white" href="#register">免费注册 <ArrowRight size={17} /></a>
      </section>

      <footer className="footer">
        <div className="footer-brand"><BrandMark /><div><strong>{manifest.product_name}</strong><small>让 AI 进入项目，把事情做完。</small></div></div>
        <div className="footer-links"><a href="#why">产品能力</a><a href="#download">客户端下载</a><a href="#register">注册</a><a href={manifest.app_url}>登录</a><a href="/admin/releases">发布管理</a></div>
        <span className="copyright">© 2025–2026 Okra</span>
      </footer>
    </main>
  );
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

function toFriendlyRegistrationError(error: unknown) {
  const message = error instanceof Error ? error.message : String(error);
  const translations: Array<[string, string]> = [
    ['email already registered', '这个邮箱已经注册，可以直接登录。'],
    ['invite code is invalid', '邀请码无效或已经失效。'],
    ['verification code is invalid or expired', '邮箱验证码错误或已经过期。'],
    ['verification code was sent recently', '验证码刚刚发送，请稍后再试。'],
    ['too many verification emails', '验证码发送次数过多，请稍后再试。'],
    ['email format is invalid', '请输入有效的邮箱地址。'],
    ['registration service is temporarily unavailable', '注册服务暂时不可用，请稍后再试。'],
  ];
  return translations.find(([source]) => message.toLowerCase().includes(source))?.[1] ?? message;
}

export default App;
