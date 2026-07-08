// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { useEffect, useMemo, useState } from 'react';
import {
  AlertTriangle,
  ArrowRight,
  Boxes,
  Brain,
  CheckCircle2,
  Clock3,
  Contact,
  Cpu,
  Database,
  Fingerprint,
  GitBranch,
  Layers3,
  MemoryStick,
  MessageSquare,
  Play,
  RefreshCw,
  Route,
  ShieldCheck,
  Sparkles,
  TerminalSquare,
} from 'lucide-react';

type DefaultPort = {
  name: string;
  backend?: number | null;
  frontend?: number | null;
};

type ServiceInfo = {
  name: string;
  directory: string;
  role: string;
  capability: string;
};

type ShowcaseImage = {
  id: string;
  title: string;
  path: string;
  source_url: string;
};

type SiteManifest = {
  product_name: string;
  tagline: string;
  default_ports: DefaultPort[];
  services: ServiceInfo[];
  showcase_images: ShowcaseImage[];
};

type ServiceState = 'online' | 'degraded' | 'offline';

type ServiceHealth = {
  name: string;
  role: string;
  url: string;
  state: ServiceState;
  status_code?: number | null;
  latency_ms?: number | null;
  detail: string;
};

type SiteStatusResponse = {
  checked_at_ms: number;
  timeout_ms: number;
  live_status_enabled: boolean;
  detail: string;
  services: ServiceHealth[];
};

const fallbackManifest: SiteManifest = {
  product_name: 'Chatos RS',
  tagline: '让 AI 成为可以长期协作的联系人。',
  default_ports: [
    { name: 'Chatos main', backend: 3997, frontend: 8088 },
    { name: 'Memory Engine', backend: 7081, frontend: 4178 },
    { name: 'Task Runner', backend: 39090, frontend: 39091 },
    { name: 'User Service', backend: 39190, frontend: 39191 },
    { name: 'Project Management', backend: 39210, frontend: 39211 },
    { name: 'Sandbox Manager', backend: 8095, frontend: 8096 },
    { name: 'Official Website', backend: 39250, frontend: 39251 },
  ],
  services: [
    {
      name: 'chatos',
      directory: 'chatos/',
      role: '主应用微服务',
      capability: 'frontend 提供联系人驱动的主交互界面，backend 承载消息、流式响应、工具路由和跨服务编排。',
    },
    {
      name: 'memory_engine',
      directory: 'memory_engine/',
      role: '长期记忆微服务',
      capability: '把线程、消息、摘要、主题记忆和上下文组装从主聊天中解耦。',
    },
    {
      name: 'task_runner_service',
      directory: 'task_runner_service/',
      role: '异步执行链路',
      capability: '让复杂任务排队、执行、复核、回调，并保留可观察运行记录。',
    },
    {
      name: 'user_service',
      directory: 'user_service/',
      role: '统一身份与模型配置',
      capability: '管理真实用户、agent account、令牌交换和共享模型配置。',
    },
    {
      name: 'project_management_service',
      directory: 'project_management_service/',
      role: '工程计划管理',
      capability: '沉淀需求、技术方案、项目任务和依赖关系。',
    },
    {
      name: 'sandbox_manager_service',
      directory: 'sandbox_manager_service/',
      role: '隔离执行底座',
      capability: '管理 Docker/Kata 沙箱租约、镜像初始化和沙箱 MCP 代理。',
    },
  ],
  showcase_images: [
    {
      id: 'chatos-main',
      title: '联系人驱动的主聊天',
      path: '/showcase/chatos-main.png',
      source_url: 'http://127.0.0.1:8088',
    },
    {
      id: 'memory-engine',
      title: 'Memory Engine 控制台',
      path: '/showcase/memory-engine.png',
      source_url: 'http://127.0.0.1:4178',
    },
    {
      id: 'task-runner',
      title: 'Task Runner 运行台',
      path: '/showcase/task-runner.png',
      source_url: 'http://127.0.0.1:39091',
    },
    {
      id: 'sandbox-manager',
      title: 'Sandbox Manager 管理台',
      path: '/showcase/sandbox-manager.png',
      source_url: 'http://127.0.0.1:8096',
    },
    {
      id: 'project-management',
      title: 'Project Management 工作台',
      path: '/showcase/project-management.png',
      source_url: 'http://127.0.0.1:39211',
    },
  ],
};

const painPoints = [
  {
    title: '会话不是协作对象',
    body: '工程问题往往跨越多天，单次聊天窗口很难承载长期关系、项目状态和角色能力。',
  },
  {
    title: '上下文成本持续膨胀',
    body: '越想保留历史，越容易把 token 花在重复铺垫上，真正关键的事实反而不稳定。',
  },
  {
    title: '执行链路难以续接',
    body: '工具调用、后台执行、复核和回调如果只挂在当前会话里，就很难变成可运维流程。',
  },
];

const runtimeShifts = [
  {
    label: '用户入口',
    title: '稳定联系人',
    body: '用户回到同一个联系人身上继续推进，而不是在一串会话标题里猜哪一个还保留上下文。',
  },
  {
    label: '运行记录',
    title: '会话退到后台',
    body: '会话仍然保存消息、工具调用和上下文快照，但它服务于联系人连续性，不再抢占用户心智。',
  },
  {
    label: '工程延伸',
    title: '任务与记忆接管长尾',
    body: '摘要、主体记忆、异步任务、项目计划和沙箱执行共同接住跨天工作的后半段。',
  },
];

const contactModelSteps = [
  { icon: Contact, title: '虚拟化联系人', body: '用户选择长期协作对象，而不是翻找一次性的历史会话。' },
  { icon: Cpu, title: 'Agent 能力绑定', body: '角色定义、技能、MCP 能力和模型配置随联系人进入运行上下文。' },
  { icon: GitBranch, title: '项目作用域', body: '同一联系人可以在不同项目里继续工作，项目和工作区成为上下文边界。' },
  { icon: Brain, title: '主体记忆召回', body: 'Memory Engine 从会话摘要继续提炼用户、联系人、agent 和项目记忆。' },
  { icon: Clock3, title: '异步执行延伸', body: 'Task Runner 把后续执行、复核和回调变成稳定后台链路。' },
];

const architectureFlow = [
  ['用户消息', '主聊天前端'],
  ['主聊天前端', 'Rust 编排后端'],
  ['Rust 编排后端', 'Memory Engine'],
  ['Rust 编排后端', 'Task Runner'],
  ['Task Runner', 'Sandbox Manager'],
  ['Task Runner', 'Project Management'],
  ['Memory Engine', '联系人上下文'],
  ['Task Runner', '联系人上下文'],
];

const serviceIcons = [MessageSquare, Route, MemoryStick, Clock3, Fingerprint, Layers3, ShieldCheck];

function App() {
  const [manifest, setManifest] = useState<SiteManifest>(fallbackManifest);
  const [siteStatus, setSiteStatus] = useState<SiteStatusResponse | null>(null);
  const [statusError, setStatusError] = useState<string | null>(null);
  const [failedImages, setFailedImages] = useState<Record<string, boolean>>({});

  useEffect(() => {
    let cancelled = false;
    fetch('/api/site/manifest')
      .then((response) => {
        if (!response.ok) {
          throw new Error(`manifest request failed: ${response.status}`);
        }
        return response.json() as Promise<SiteManifest>;
      })
      .then((payload) => {
        if (!cancelled) {
          setManifest(payload);
        }
      })
      .catch(() => {
        if (!cancelled) {
          setManifest(fallbackManifest);
        }
      });
    return () => {
      cancelled = true;
    };
  }, []);

  useEffect(() => {
    let cancelled = false;

    const loadStatus = async () => {
      try {
        const response = await fetch('/api/site/status');
        if (!response.ok) {
          throw new Error(`status request failed: ${response.status}`);
        }
        const payload = (await response.json()) as SiteStatusResponse;
        if (!cancelled) {
          setSiteStatus(payload);
          setStatusError(null);
        }
      } catch (error) {
        if (!cancelled) {
          setStatusError(error instanceof Error ? error.message : 'status request failed');
        }
      }
    };

    void loadStatus();
    const timer = window.setInterval(() => {
      void loadStatus();
    }, 15_000);

    return () => {
      cancelled = true;
      window.clearInterval(timer);
    };
  }, []);

  const heroStyle = useMemo(
    () => ({
      backgroundImage:
        failedImages['chatos-main']
          ? undefined
          : 'linear-gradient(90deg, rgba(5, 8, 7, 0.96) 0%, rgba(5, 8, 7, 0.86) 48%, rgba(5, 8, 7, 0.58) 100%), linear-gradient(180deg, rgba(5, 8, 7, 0.18), rgba(5, 8, 7, 0.96)), url("/showcase/chatos-main.png")',
    }),
    [failedImages],
  );

  const setImageFailed = (id: string) => {
    setFailedImages((current) => ({ ...current, [id]: true }));
  };

  const liveStatusEnabled = siteStatus?.live_status_enabled ?? true;
  const onlineCount = liveStatusEnabled
    ? (siteStatus?.services.filter((service) => service.state === 'online').length ?? 0)
    : 0;
  const totalCount = liveStatusEnabled ? (siteStatus?.services.length ?? 0) : 0;

  return (
    <main>
      <section className="hero" style={heroStyle}>
        <nav className="topbar" aria-label="主导航">
          <a className="brand" href="#top" aria-label="Chatos RS 首页">
            <span className="brand-mark">
              <Sparkles size={18} />
            </span>
            <span>{manifest.product_name}</span>
          </a>
          <div className="nav-links">
            <a href="#contact-model">联系人模型</a>
            <a href="#architecture">微服务</a>
            <a href="#service-status">状态</a>
            <a href="#showcase">截图</a>
            <a href="#local-run">本地运行</a>
          </div>
        </nav>

        <div className="hero-content" id="top">
          <p className="eyebrow">AI engineering workflow platform</p>
          <h1>{manifest.product_name}</h1>
          <p className="hero-claim">{manifest.tagline}</p>
          <p className="hero-copy">
            用虚拟化联系人承载长期关系，用 Memory Engine 跨会话沉淀上下文，
            用 Task Runner 把后续执行变成可观察、可回调、可复核的后台链路。
          </p>
          <div className="hero-actions">
            <a className="primary-action" href="#contact-model">
              <Play size={18} />
              查看工作模型
            </a>
            <a className="secondary-action" href="#local-run">
              <TerminalSquare size={18} />
              本地启动
            </a>
          </div>
          <div className="hero-metrics" aria-label="平台能力概览">
            <span><strong>7</strong> 个核心微服务</span>
            <span><strong>3</strong> 层记忆沉淀</span>
            <span><strong>1</strong> 个联系人入口</span>
          </div>
        </div>
      </section>

      <section className="section intro-band" id="why">
        <div className="section-heading">
          <p className="eyebrow">Why it matters</p>
          <h2>工程协作不能被会话边界切断</h2>
          <p>
            Chatos RS 没有否定会话，而是把会话从用户入口退到运行记录层。
            用户面对稳定联系人，系统在底层维护会话、记忆、任务和工具链路。
          </p>
        </div>
        <div className="pain-grid">
          {painPoints.map((item) => (
            <article className="info-tile" key={item.title}>
              <CheckCircle2 size={20} />
              <h3>{item.title}</h3>
              <p>{item.body}</p>
            </article>
          ))}
        </div>
        <div className="runtime-shifts" aria-label="从会话制到联系人制的变化">
          {runtimeShifts.map((item) => (
            <article className="shift-item" key={item.title}>
              <span>{item.label}</span>
              <h3>{item.title}</h3>
              <p>{item.body}</p>
            </article>
          ))}
        </div>
      </section>

      <section className="section contact-model" id="contact-model">
        <div className="section-heading">
          <p className="eyebrow">Contact-first runtime</p>
          <h2>从“打开一个会话”变成“找一个联系人继续做事”</h2>
          <p>
            联系人是面向用户的稳定协作对象。Agent、项目、记忆、会话和任务都被挂接到这个对象上，
            让跨天、跨项目、跨工具的工作自然续接。
          </p>
        </div>
        <div className="contact-flow">
          {contactModelSteps.map((item, index) => {
            const Icon = item.icon;
            return (
              <article className="flow-step" key={item.title}>
                <div className="step-index">{String(index + 1).padStart(2, '0')}</div>
                <Icon size={24} />
                <h3>{item.title}</h3>
                <p>{item.body}</p>
              </article>
            );
          })}
        </div>
      </section>

      <section className="section architecture" id="architecture">
        <div className="section-heading compact">
          <p className="eyebrow">Microservice architecture</p>
          <h2>一个入口，多条工程链路协同</h2>
          <p>
            主聊天负责实时编排，Memory Engine 负责长期上下文，Task Runner 负责异步执行，
            User Service、Project Management 和 Sandbox Manager 补齐身份、计划和隔离执行。
          </p>
        </div>
        <div className="architecture-layout">
          <div className="flow-map" aria-label="系统流程">
            {architectureFlow.map(([from, to]) => (
              <div className="flow-link" key={`${from}-${to}`}>
                <span>{from}</span>
                <ArrowRight size={16} />
                <span>{to}</span>
              </div>
            ))}
          </div>
          <div className="service-grid">
            {manifest.services.map((service, index) => {
              const Icon = serviceIcons[index % serviceIcons.length];
              return (
                <article className="service-card" key={service.name}>
                  <div className="service-card-header">
                    <Icon size={21} />
                    <span>{service.directory}</span>
                  </div>
                  <h3>{service.name}</h3>
                  <strong>{service.role}</strong>
                  <p>{service.capability}</p>
                </article>
              );
            })}
          </div>
        </div>
      </section>

      <section className="section service-status-band" id="service-status">
        <div className="section-heading compact">
          <p className="eyebrow">Live local stack</p>
          <h2>官网也能看见微服务是否在线</h2>
          <p>
            官网后端会以短超时探测本机各核心服务的健康检查端点，把本地开发环境的运行状态直接呈现出来。
          </p>
        </div>
        <div className="status-summary" aria-label="本地服务状态汇总">
          <span>
            {liveStatusEnabled ? <CheckCircle2 size={17} /> : <ShieldCheck size={17} />}
            {liveStatusEnabled
              ? `${onlineCount}/${totalCount || fallbackManifest.default_ports.length} online`
              : 'live status disabled'}
          </span>
          <span>
            <Clock3 size={17} />
            timeout {siteStatus?.timeout_ms ?? 800}ms
          </span>
          <span>
            <RefreshCw size={17} />
            {siteStatus ? formatCheckedTime(siteStatus.checked_at_ms) : 'checking'}
          </span>
        </div>
        {statusError && !siteStatus ? (
          <div className="status-error">
            <AlertTriangle size={18} />
            <span>{statusError}</span>
          </div>
        ) : siteStatus && !siteStatus.live_status_enabled ? (
          <div className="status-disabled">
            <ShieldCheck size={20} />
            <div>
              <strong>Live status is disabled for this deployment.</strong>
              <p>{siteStatus.detail}</p>
            </div>
          </div>
        ) : (
          <div className="status-grid">
            {(siteStatus?.services ?? []).map((service) => {
              const Icon = service.state === 'online' ? CheckCircle2 : AlertTriangle;
              return (
                <article className={`status-card status-${service.state}`} key={service.name}>
                  <div className="status-card-head">
                    <span>
                      <Icon size={17} />
                      {statusLabel(service.state)}
                    </span>
                    <code>{formatLatency(service)}</code>
                  </div>
                  <h3>{service.name}</h3>
                  <p>{service.role}</p>
                  <small>{service.url}</small>
                </article>
              );
            })}
            {!siteStatus &&
              fallbackManifest.default_ports.slice(0, 6).map((service) => (
                <article className="status-card status-pending" key={service.name}>
                  <div className="status-card-head">
                    <span>
                      <RefreshCw size={17} />
                      checking
                    </span>
                    <code>--</code>
                  </div>
                  <h3>{service.name}</h3>
                  <p>waiting for health probe</p>
                  <small>{formatPorts(service)}</small>
                </article>
              ))}
          </div>
        )}
      </section>

      <section className="section memory-task-band">
        <div className="split-copy">
          <div>
            <p className="eyebrow">Memory</p>
            <h2>记忆从会话摘要升级为主体记忆</h2>
            <p>
              Memory Engine 将消息记录整理为线程摘要，再继续 rollup 成项目级知识和 subject memory。
              下一轮请求通过 context compose 拉回最近记录、摘要和长期记忆。
            </p>
          </div>
          <div>
            <p className="eyebrow">Task Runner</p>
            <h2>后台执行不再丢在当前窗口里</h2>
            <p>
              Task Runner 把复杂任务拆成可排队、可执行、可复核的运行链路。结果回到主聊天，
              用户感知到的是联系人持续推进，而不是另一个割裂系统。
            </p>
          </div>
        </div>
      </section>

      <section className="section showcase" id="showcase">
        <div className="section-heading">
          <p className="eyebrow">Product screens</p>
          <h2>用真实系统界面说明能力</h2>
          <p>
            官网素材来自本地运行的微服务。没有截图时会显示安全占位，截图采集完成后会自动替换为真实界面。
          </p>
        </div>
        <div className="showcase-grid">
          {manifest.showcase_images.map((image) => (
            <figure className="showcase-item" key={image.id}>
              {failedImages[image.id] ? (
                <div className="screenshot-fallback">
                  <Boxes size={28} />
                  <span>{image.title}</span>
                  <small>{image.source_url}</small>
                </div>
              ) : (
                <img
                  src={image.path}
                  alt={image.title}
                  loading="lazy"
                  onError={() => setImageFailed(image.id)}
                />
              )}
              <figcaption>
                <span>{image.title}</span>
                <a href={image.source_url} target="_blank" rel="noreferrer">打开来源</a>
              </figcaption>
            </figure>
          ))}
        </div>
      </section>

      <section className="section developer-stack">
        <div className="section-heading compact">
          <p className="eyebrow">Developer stack</p>
          <h2>为工程工作流而不是演示玩具设计</h2>
        </div>
        <div className="stack-row">
          <span><Database size={18} /> MongoDB / SQLite</span>
          <span><Cpu size={18} /> Rust / Axum / Tokio</span>
          <span><Layers3 size={18} /> React / Vite</span>
          <span><Route size={18} /> MCP-style tooling</span>
          <span><ShieldCheck size={18} /> Docker / Kata sandbox</span>
        </div>
      </section>

      <section className="section local-run" id="local-run">
        <div className="section-heading">
          <p className="eyebrow">Local run</p>
          <h2>在本地把整套系统拉起来</h2>
          <p>
            官网作为独立微服务运行；主系统仍由已有脚本管理。默认端口来自仓库根目录 `.env.example`。
          </p>
        </div>
        <div className="run-layout">
          <div className="command-panel">
            <div className="command-header">
              <TerminalSquare size={18} />
              <span>启动命令</span>
            </div>
            <pre>{`make restart-all
make restart-official-website

# production-style website
make build-official-website
OFFICIAL_WEBSITE_MODE=prod make restart-official-website`}</pre>
          </div>
          <div className="port-table" aria-label="默认服务端口">
            {manifest.default_ports.map((item) => (
              <div className="port-row" key={item.name}>
                <span>{item.name}</span>
                <code>{formatPorts(item)}</code>
              </div>
            ))}
          </div>
        </div>
      </section>

      <footer className="footer">
        <span>{manifest.product_name}</span>
        <span>Source-available under PolyForm Noncommercial License 1.0.0</span>
      </footer>
    </main>
  );
}

function formatPorts(item: DefaultPort) {
  const backend = item.backend ? `backend:${item.backend}` : '';
  const frontend = item.frontend ? `frontend:${item.frontend}` : '';
  return [backend, frontend].filter(Boolean).join(' / ');
}

function statusLabel(state: ServiceState) {
  if (state === 'online') {
    return 'online';
  }
  if (state === 'degraded') {
    return 'degraded';
  }
  return 'offline';
}

function formatLatency(service: ServiceHealth) {
  if (service.latency_ms == null) {
    return service.detail;
  }
  return `${service.latency_ms}ms`;
}

function formatCheckedTime(value: number) {
  return new Date(value).toLocaleTimeString('zh-CN', {
    hour: '2-digit',
    minute: '2-digit',
    second: '2-digit',
  });
}

export default App;
