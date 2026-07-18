import { Canvas } from '@react-three/fiber';
import {
  Archive,
  Bot,
  Cat as CatIcon,
  CheckCircle2,
  ChevronLeft,
  CircleAlert,
  Clock3,
  CloudSun,
  FolderOpen,
  Home,
  Laptop,
  LockKeyhole,
  MoonStar,
  PanelRightOpen,
  RefreshCw,
  Send,
  Server,
  Smartphone,
  Sparkles,
  Sun,
  Sunset,
  Terminal,
  Wifi,
  LogIn,
  X,
} from 'lucide-react';
import { FormEvent, Suspense, useEffect, useMemo, useRef, useState } from 'react';
import { ACESFilmicToneMapping, SRGBColorSpace } from 'three';
import { demoProjects, demoTasks, initialMessages } from './demoData';
import { InWorldChatScreen } from './InWorldChatScreen';
import { RealisticRoom } from './scene/AnimeRoom';
import type { ChatAgentOption, ChatContact, ChatMessage, ChatModelOption, ChatRuntimeSettings, ChatSession, DemoProject, DemoTask, DemoTaskGraph, TimeMode, ViewMode } from './types';
import { useChatOSBridge } from './useChatOSBridge';

const TIME_MODES: Array<{ mode: TimeMode; label: string; icon: typeof Sun }> = [
  { mode: 'day', label: '白天', icon: Sun },
  { mode: 'sunset', label: '黄昏', icon: Sunset },
  { mode: 'night', label: '夜晚', icon: MoonStar },
];

const VIEW_LABELS: Record<ViewMode, string> = {
  room: '房间总览',
  computer: '电脑 · 全屏桌面',
  chat: '电脑 · AI 聊天',
  terminal: '电脑 · 终端',
  remote: '电脑 · 远程连接',
  archive: '左侧 · 用户项目书架',
  project: '左侧 · 翻阅项目档案册',
  projection: '右侧 · 实时任务墙',
  phone: '桌面 · 手机',
};

const STATUS_LABELS: Record<DemoTask['status'], string> = {
  doing: '进行中',
  todo: '待处理',
  blocked: '阻塞',
  done: '已完成',
};

const TASK_EMPTY_IMAGES: Record<TimeMode, string> = {
  day: '/assets/window-day.jpg',
  sunset: '/assets/window-sunset.jpg',
  night: '/assets/window-night.jpg',
};

const PROJECT_STATUS_LABELS: Record<DemoProject['status'], string> = {
  running: '运行中',
  planning: '规划中',
  idle: '空闲',
};

const projectSourceLabel = (sourceType?: string | null) => {
  const source = String(sourceType || '').trim().toLowerCase();
  if (source === 'cloud' || source === 'harness') return '云端工作区';
  if (source === 'git' || source === 'repository') return 'Git 仓库';
  if (source === 'local' || source === 'filesystem') return '本地工作区';
  return sourceType || '未标注';
};

const projectImportLabel = (importStatus?: string | null) => {
  const status = String(importStatus || '').trim();
  if (!status) return '未提供';
  const normalized = status.toLowerCase();
  if (['ready', 'complete', 'completed', 'success', 'imported'].includes(normalized)) return '已同步';
  if (normalized.includes('import') || normalized.includes('running') || normalized.includes('sync')) return '同步中';
  if (normalized.includes('fail') || normalized.includes('error')) return '同步异常';
  return status;
};

const projectItemKindLabel = (kind: NonNullable<DemoProject['planItems']>[number]['kind']) => {
  if (kind === 'requirement') return '项目需求';
  if (kind === 'work-item') return '执行事项';
  return '项目资料';
};

const projectItemStatusLabel = (status?: string | null) => {
  const normalized = String(status || '').trim().toLowerCase();
  if (!normalized) return '已收录';
  if (normalized === 'done' || normalized === 'completed') return '已完成';
  if (normalized === 'in_progress' || normalized === 'doing' || normalized === 'running') return '进行中';
  if (normalized === 'blocked') return '阻塞';
  if (normalized === 'todo' || normalized === 'pending') return '待处理';
  return status as string;
};

const formatTime = () => new Intl.DateTimeFormat('zh-CN', {
  hour: '2-digit',
  minute: '2-digit',
  hour12: false,
}).format(new Date());

const formatPhoneDate = () => {
  const now = new Date();
  const date = new Intl.DateTimeFormat('zh-CN', {
    month: 'long',
    day: 'numeric',
  }).format(now);
  const weekday = new Intl.DateTimeFormat('zh-CN', {
    weekday: 'short',
  }).format(now);
  return `${date} · ${weekday}`;
};

function SceneLoading() {
  return (
    <div className="scene-loading">
      <Sparkles size={20} />
      <span>正在进入你的写实 3D 书房…</span>
    </div>
  );
}

function TopBar({
  view,
  timeMode,
  onTimeModeChange,
}: {
  view: ViewMode;
  timeMode: TimeMode;
  onTimeModeChange: (timeMode: TimeMode) => void;
}) {
  const timeIndex = TIME_MODES.findIndex((item) => item.mode === timeMode);
  const nextTime = TIME_MODES[(timeIndex + 1) % TIME_MODES.length];
  const TimeIcon = TIME_MODES[timeIndex].icon;

  return (
    <header className="topbar">
      <div className="brand-block">
        <div className="brand-mark">
          <Sparkles size={18} />
        </div>
        <div>
          <div className="brand-title">ChatOS Room</div>
          <div className="brand-subtitle">独立写实 3D 房间原型 · 2.0.8</div>
        </div>
      </div>

      <div className="location-pill">
        <span className="location-dot" />
        {VIEW_LABELS[view]}
      </div>

      <button
        className="time-toggle"
        type="button"
        onClick={() => onTimeModeChange(nextTime.mode)}
        title={`切换到${nextTime.label}`}
      >
        <TimeIcon size={18} />
        <span>{TIME_MODES[timeIndex].label}</span>
      </button>
    </header>
  );
}

function BottomNavigation({
  view,
  onViewChange,
}: {
  view: ViewMode;
  onViewChange: (view: ViewMode) => void;
}) {
  const items: Array<{ view: ViewMode; label: string; icon: typeof Home }> = [
    { view: 'room', label: '房间', icon: Home },
    { view: 'archive', label: '项目', icon: Archive },
    { view: 'computer', label: '电脑', icon: Laptop },
    { view: 'projection', label: '任务', icon: PanelRightOpen },
    { view: 'phone', label: '手机', icon: Smartphone },
  ];

  return (
    <nav className="bottom-nav" aria-label="空间导航">
      {items.map((item) => {
        const Icon = item.icon;
        const active = view === item.view
          || (item.view === 'archive' && view === 'project')
          || (item.view === 'computer' && (view === 'chat' || view === 'terminal' || view === 'remote'));
        return (
          <button
            key={item.view}
            type="button"
            className={active ? 'bottom-nav__item is-active' : 'bottom-nav__item'}
            onClick={() => onViewChange(item.view)}
          >
            <Icon size={19} />
            <span>{item.label}</span>
          </button>
        );
      })}
    </nav>
  );
}

function RoomHint() {
  return (
    <div className="room-hint">
      <div className="room-hint__title">
        <CloudSun size={18} />
        欢迎回到书房
      </div>
      <p>点击电脑进入全屏桌面，左墙书架每页陈列 6 个用户项目，右侧大屏展示运行任务。桌上的小猫也可以摸。</p>
      <div className="room-hint__keys">
        <kbd>←</kbd>
        <kbd>→</kbd>
        <span>切换视角</span>
        <kbd>Esc</kbd>
        <span>返回房间</span>
      </div>
    </div>
  );
}

type ChatOSBridge = ReturnType<typeof useChatOSBridge>;

function OverlayShell({
  eyebrow,
  title,
  icon: Icon,
  onClose,
  children,
  wide = false,
}: {
  eyebrow: string;
  title: string;
  icon: typeof Home;
  onClose: () => void;
  children: React.ReactNode;
  wide?: boolean;
}) {
  return (
    <section className={wide ? 'workspace-overlay is-wide' : 'workspace-overlay'}>
      <div className="workspace-overlay__glow" />
      <header className="workspace-overlay__header">
        <div className="workspace-overlay__title">
          <div className="workspace-overlay__icon">
            <Icon size={21} />
          </div>
          <div>
            <span>{eyebrow}</span>
            <h1>{title}</h1>
          </div>
        </div>
        <button className="icon-button" type="button" onClick={onClose} aria-label="返回房间">
          <X size={19} />
        </button>
      </header>
      <div className="workspace-overlay__body">{children}</div>
    </section>
  );
}

function ComputerWorkspace({
  messages,
  thinking,
  onSend,
  onClose,
}: {
  messages: ChatMessage[];
  thinking: boolean;
  onSend: (content: string) => void;
  onClose: () => void;
}) {
  const [input, setInput] = useState('');
  const messageListRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    messageListRef.current?.scrollTo({
      top: messageListRef.current.scrollHeight,
      behavior: 'smooth',
    });
  }, [messages, thinking]);

  const handleSubmit = (event: FormEvent) => {
    event.preventDefault();
    const content = input.trim();
    if (!content || thinking) return;
    onSend(content);
    setInput('');
  };

  return (
    <OverlayShell
      eyebrow="DESK COMPUTER"
      title="与 AI 一起工作"
      icon={Laptop}
      onClose={onClose}
      wide
    >
      <div className="chat-workspace">
        <aside className="chat-sidebar">
          <div className="assistant-card">
            <div className="assistant-card__avatar">
              <Bot size={22} />
            </div>
            <div>
              <b>ChatOS Assistant</b>
              <span><i /> 独立演示模式</span>
            </div>
          </div>

          <div className="sidebar-section-label">今天的工作</div>
          <button className="sidebar-conversation is-active" type="button">
            <Laptop size={16} />
            <span>
              <b>3D 主页改造</b>
              <small>刚刚更新</small>
            </span>
          </button>
          <button className="sidebar-conversation" type="button">
            <FolderOpen size={16} />
            <span>
              <b>项目架构整理</b>
              <small>昨天</small>
            </span>
          </button>

          <div className="chat-sidebar__note">
            <Sparkles size={16} />
            <span>正式接入时复用现有聊天 Store、HTTP 命令提交和 WebSocket 流式事件。</span>
          </div>
        </aside>

        <main className="chat-main">
          <div className="chat-main__header">
            <div>
              <b>3D 主页改造</b>
              <span>Realistic room prototype</span>
            </div>
            <div className="model-pill">GPT · reasoning</div>
          </div>

          <div className="message-list" ref={messageListRef}>
            <div className="conversation-date"><span>今天</span></div>
            {messages.map((message) => (
              <article
                className={message.role === 'user' ? 'chat-message is-user' : 'chat-message is-assistant'}
                key={message.id}
              >
                <div className="chat-message__avatar">
                  {message.role === 'user' ? '你' : <Bot size={17} />}
                </div>
                <div className="chat-message__body">
                  <div className="chat-message__meta">
                    <b>{message.role === 'user' ? 'You' : 'ChatOS'}</b>
                    <time>{message.time}</time>
                  </div>
                  <p>{message.content}</p>
                </div>
              </article>
            ))}
            {thinking ? (
              <article className="chat-message is-assistant">
                <div className="chat-message__avatar"><Bot size={17} /></div>
                <div className="chat-message__body">
                  <div className="chat-message__meta"><b>ChatOS</b><time>正在思考</time></div>
                  <div className="typing-dots"><i /><i /><i /></div>
                </div>
              </article>
            ) : null}
          </div>

          <form className="chat-composer" onSubmit={handleSubmit}>
            <div className="chat-composer__input">
              <textarea
                value={input}
                onChange={(event) => setInput(event.target.value)}
                onKeyDown={(event) => {
                  if (event.key === 'Enter' && !event.shiftKey) {
                    event.preventDefault();
                    event.currentTarget.form?.requestSubmit();
                  }
                }}
                placeholder="告诉 AI 你想做什么…"
                rows={2}
              />
              <div className="chat-composer__footer">
                <span>Shift + Enter 换行</span>
                <button type="submit" disabled={!input.trim() || thinking}>
                  <Send size={17} />
                  发送
                </button>
              </div>
            </div>
          </form>
        </main>
      </div>
    </OverlayShell>
  );
}

function ProjectWorkspace({
  projects,
  selectedProject,
  onSelect,
  onClose,
}: {
  projects: DemoProject[];
  selectedProject: DemoProject;
  onSelect: (project: DemoProject) => void;
  onClose: () => void;
}) {
  return (
    <OverlayShell eyebrow="PROJECT ARCHIVE" title="用户项目书架档案" icon={Archive} onClose={onClose} wide>
      <div className="archive-workspace">
        <aside className="archive-list">
          <div className="archive-list__label">最近项目</div>
          {projects.map((project) => (
            <button
              type="button"
              key={project.id}
              className={selectedProject.id === project.id ? 'archive-item is-active' : 'archive-item'}
              onClick={() => onSelect(project)}
            >
              <span className="archive-item__folder" style={{ background: project.accent }}>
                <FolderOpen size={18} />
              </span>
              <span className="archive-item__content">
                <b>{project.name}</b>
                <small>{project.subtitle}</small>
              </span>
              <em>{project.progress}%</em>
            </button>
          ))}
        </aside>

        <main className="project-dossier">
          <div className="dossier-cover" style={{ '--project-accent': selectedProject.accent } as React.CSSProperties}>
            <div className="dossier-cover__topline">
              <span>PROJECT FILE / {selectedProject.id.toUpperCase()}</span>
              <span className={`project-status is-${selectedProject.status}`}>
                {PROJECT_STATUS_LABELS[selectedProject.status]}
              </span>
            </div>
            <div className="dossier-cover__title">
              <div className="dossier-logo"><FolderOpen size={28} /></div>
              <div>
                <h2>{selectedProject.name}</h2>
                <p>{selectedProject.subtitle}</p>
              </div>
            </div>
            <div className="project-progress">
              <div><span>完成度</span><b>{selectedProject.progress}%</b></div>
              <div className="project-progress__bar">
                <i style={{ width: `${selectedProject.progress}%`, background: selectedProject.accent }} />
              </div>
            </div>
          </div>

          <div className="dossier-grid">
            <section className="dossier-section">
              <span className="section-kicker">SUMMARY</span>
              <h3>项目摘要</h3>
              <p>{selectedProject.summary}</p>
              <div className="project-meta-row">
                <Clock3 size={15} />
                最后更新：{selectedProject.updatedAt}
              </div>
            </section>

            <section className="dossier-section">
              <span className="section-kicker">KEY FILES</span>
              <h3>关键内容</h3>
              <div className="file-stack">
                {selectedProject.files.map((file) => (
                  <div key={file}><span>{file}</span><ChevronLeft size={14} /></div>
                ))}
              </div>
            </section>
          </div>

          <div className="dossier-actions">
            <button type="button" className="secondary-button">查看运行状态</button>
            <button type="button" className="primary-button"><FolderOpen size={16} />打开项目详情</button>
          </div>
        </main>
      </div>
    </OverlayShell>
  );
}

function TaskWorkspace({
  tasks,
  selectedTask,
  onSelect,
  onClose,
}: {
  tasks: DemoTask[];
  selectedTask: DemoTask;
  onSelect: (task: DemoTask) => void;
  onClose: () => void;
}) {
  const counts = useMemo(() => ({
    doing: tasks.filter((task) => task.status === 'doing').length,
    blocked: tasks.filter((task) => task.status === 'blocked').length,
    done: tasks.filter((task) => task.status === 'done').length,
  }), [tasks]);

  return (
    <OverlayShell eyebrow="LIVE PROJECTION" title="正在运行的任务" icon={PanelRightOpen} onClose={onClose} wide>
      <div className="task-workspace">
        <div className="task-stats">
          <div className="task-stat is-running"><Sparkles size={19} /><span><b>{counts.doing}</b>正在运行</span></div>
          <div className="task-stat is-blocked"><CircleAlert size={19} /><span><b>{counts.blocked}</b>需要处理</span></div>
          <div className="task-stat is-done"><CheckCircle2 size={19} /><span><b>{counts.done}</b>已经完成</span></div>
        </div>

        <div className="task-content-grid">
          <div className="task-board">
            {tasks.map((task) => (
              <button
                type="button"
                key={task.id}
                className={selectedTask.id === task.id ? `task-row is-${task.status} is-active` : `task-row is-${task.status}`}
                onClick={() => onSelect(task)}
              >
                <i className="task-row__status" />
                <span className="task-row__content">
                  <b>{task.title}</b>
                  <small>{STATUS_LABELS[task.status]}</small>
                </span>
                <span className="task-row__progress">
                  <em>{task.progress}%</em>
                  <span><i style={{ width: `${task.progress}%` }} /></span>
                </span>
              </button>
            ))}
          </div>

          <aside className={`task-detail is-${selectedTask.status}`}>
            <span className="section-kicker">TASK DETAIL</span>
            <div className="task-detail__title">
              <h2>{selectedTask.title}</h2>
              <span>{STATUS_LABELS[selectedTask.status]}</span>
            </div>
            <p>{selectedTask.detail}</p>
            <div className="task-detail__progress">
              <div><span>执行进度</span><b>{selectedTask.progress}%</b></div>
              <span><i style={{ width: `${selectedTask.progress}%` }} /></span>
            </div>
            <div className="task-timeline">
              <div className="is-complete"><i /><span><b>任务已创建</b><small>09:32</small></span></div>
              <div className={selectedTask.progress > 25 ? 'is-complete' : ''}><i /><span><b>Agent 已接管</b><small>09:33</small></span></div>
              <div className={selectedTask.status === 'doing' ? 'is-current' : selectedTask.status === 'done' ? 'is-complete' : ''}>
                <i /><span><b>执行与验证</b><small>{selectedTask.status === 'blocked' ? '等待确认' : '进行中'}</small></span>
              </div>
              <div className={selectedTask.status === 'done' ? 'is-complete' : ''}><i /><span><b>交付结果</b><small>等待中</small></span></div>
            </div>
          </aside>
        </div>
      </div>
    </OverlayShell>
  );
}

function PhoneWorkspace({ timeMode, onTimeModeChange, onClose }: {
  timeMode: TimeMode;
  onTimeModeChange: (timeMode: TimeMode) => void;
  onClose: () => void;
}) {
  return (
    <OverlayShell eyebrow="DESK PHONE" title="快捷控制" icon={Smartphone} onClose={onClose}>
      <div className="phone-workspace">
        <div className="phone-mock">
          <div className="phone-mock__island" />
          <div className="phone-mock__time">{formatTime()}</div>
          <div className="phone-mock__date">{formatPhoneDate()}</div>
          <div className="phone-widget">
            <div><Bot size={18} /><b>ChatOS</b><span>在线</span></div>
            <p>3D 房间原型正在运行</p>
          </div>
          <div className="phone-widget is-task">
            <div><CheckCircle2 size={18} /><b>任务</b><span>1 running</span></div>
            <p>搭建写实 3D 房间 · 72%</p>
          </div>
        </div>

        <div className="phone-controls">
          <span className="section-kicker">ROOM AMBIENCE</span>
          <h2>房间时间</h2>
          <p>切换窗外的天空、房间光照和屏幕亮度。</p>
          <div className="time-options">
            {TIME_MODES.map((item) => {
              const Icon = item.icon;
              return (
                <button
                  type="button"
                  key={item.mode}
                  className={timeMode === item.mode ? 'is-active' : ''}
                  onClick={() => onTimeModeChange(item.mode)}
                >
                  <Icon size={21} />
                  <span>{item.label}</span>
                </button>
              );
            })}
          </div>
          <div className="phone-tip"><CatIcon size={18} /><span>提示：回到房间后可以点击桌上的小猫。</span></div>
        </div>
      </div>
    </OverlayShell>
  );
}

function InWorldLoginScreen({
  bridge,
  onDemo,
}: {
  bridge: ChatOSBridge;
  onDemo: () => void;
}) {
  const [username, setUsername] = useState('');
  const [password, setPassword] = useState('');
  const [localError, setLocalError] = useState<string | null>(null);

  const handleSubmit = async (event: FormEvent) => {
    event.preventDefault();
    if (!username.trim() || !password || bridge.status === 'connecting') return;
    setLocalError(null);
    try {
      await bridge.login(username.trim(), password);
      setPassword('');
    } catch (cause) {
      setLocalError(cause instanceof Error ? cause.message : String(cause));
    }
  };

  return (
    <div className="inworld-login-screen">
      <aside>
        <div className="login-screen-brand"><Sparkles size={24} /><b>ChatOS Room</b></div>
        <div className="login-screen-copy">
          <span>PRIVATE AI WORKSPACE</span>
          <h2>回到你的书房</h2>
          <p>登录后，电脑桌面、项目书架、任务大屏和聊天记录会同步为当前账号的真实数据。</p>
        </div>
        <small>2.0.8 · Secure workspace</small>
      </aside>
      <main>
        <header>
          <i><LockKeyhole size={20} /></i>
          <div><b>登录 ChatOS</b><span>请输入现有账号</span></div>
        </header>
        <form onSubmit={handleSubmit}>
          <label>
            <span>用户名</span>
            <input value={username} onChange={(event) => setUsername(event.target.value)} autoComplete="username" autoFocus />
          </label>
          <label>
            <span>密码</span>
            <input type="password" value={password} onChange={(event) => setPassword(event.target.value)} autoComplete="current-password" />
          </label>
          {(localError || bridge.error) ? <div className="login-screen-error">{localError || bridge.error}</div> : null}
          <button type="submit" disabled={!username.trim() || !password || bridge.status === 'connecting'}>
            {bridge.status === 'connecting' ? <RefreshCw className="is-spinning" size={14} /> : <LogIn size={14} />}
            {bridge.status === 'connecting' ? '正在登录…' : '登录并进入桌面'}
          </button>
        </form>
        <button className="login-screen-demo" type="button" onClick={onDemo}>后端未启动？先进入演示桌面</button>
        <small>{bridge.apiBaseUrl}</small>
      </main>
    </div>
  );
}

function FocusDesktop({
  timeMode,
  onChat,
  onTerminal,
  onRemote,
  onLock,
}: {
  timeMode: TimeMode;
  onChat: () => void;
  onTerminal: () => void;
  onRemote: () => void;
  onLock: () => void;
}) {
  const greeting = timeMode === 'day' ? '上午好，指挥官' : timeMode === 'sunset' ? '傍晚好，指挥官' : '晚上好，指挥官';
  const displayTime = timeMode === 'day' ? '09:41' : timeMode === 'sunset' ? '18:26' : '23:18';

  return (
    <div className={`focus-desktop is-${timeMode}`}>
      <header>
        <strong>ChatOS</strong>
        <div><button type="button" aria-label="锁定电脑" onClick={onLock}><LockKeyhole size={11} /></button><span>{displayTime}</span></div>
      </header>
      <div className="focus-desktop__apps">
        <button type="button" onClick={onChat}><i className="is-chat"><Bot size={28} /></i><span>AI 聊天</span></button>
        <button type="button" onClick={onTerminal}><i className="is-terminal"><Terminal size={28} /></i><span>终端</span></button>
        <button type="button" onClick={onRemote}><i className="is-remote"><Server size={28} /></i><span>远程连接</span></button>
      </div>
      <div className="focus-desktop__greeting"><b>{greeting}</b><span>书房工作站</span></div>
      <div className="focus-desktop__dock"><i /><i /><i /></div>
    </div>
  );
}

function ComputerFocusLayer({ children }: { children: React.ReactNode }) {
  return (
    <div className="computer-focus-layer">
      <div className="computer-focus-stage">
        {children}
      </div>
    </div>
  );
}

const TERMINAL_WELCOME = [
  'ChatOS Local Terminal 2.0.8',
  '输入 help 查看可用演示命令。',
];

function InWorldTerminalScreen() {
  const [command, setCommand] = useState('');
  const [lines, setLines] = useState(TERMINAL_WELCOME);

  const runCommand = () => {
    const value = command.trim();
    if (!value) return;
    if (value === 'clear') {
      setLines([]);
      setCommand('');
      return;
    }
    const output = value === 'help'
      ? '可用命令：help、status、projects、whoami、clear'
      : value === 'status'
        ? 'room-renderer: healthy · websocket-bridge: demo mode'
        : value === 'projects'
          ? `${demoProjects.length} user projects indexed · bookshelf ready`
          : value === 'whoami'
            ? 'local-user@chatos-room'
            : `command not found: ${value}`;
    setLines((current) => [...current, `$ ${value}`, output].slice(-10));
    setCommand('');
  };

  return (
    <div className="inworld-terminal-screen">
      <header>
        <div><Terminal size={15} /><b>终端</b></div>
        <span><i /> LOCAL SESSION</span>
      </header>
      <main>
        <div className="terminal-lines">
          {lines.map((line, index) => <p key={`${line}-${index}`}>{line}</p>)}
        </div>
        <form onSubmit={(event) => { event.preventDefault(); runCommand(); }}>
          <span>chatos %</span>
          <input
            value={command}
            onChange={(event) => setCommand(event.target.value)}
            placeholder="输入命令，例如 help"
            onKeyDown={(event) => {
              if (event.key === 'Enter') {
                event.preventDefault();
                runCommand();
              }
            }}
          />
        </form>
      </main>
    </div>
  );
}

const REMOTE_HOSTS = [
  { id: 'studio-mac', name: 'Studio Mac', detail: '192.168.1.32', status: '在线' },
  { id: 'cloud-dev', name: 'Cloud Dev', detail: 'dev.chatos.local', status: '在线' },
  { id: 'home-nas', name: 'Home NAS', detail: '192.168.1.18', status: '休眠' },
];

function InWorldRemoteScreen() {
  const [selectedHost, setSelectedHost] = useState(REMOTE_HOSTS[0]);
  const [connected, setConnected] = useState(false);

  return (
    <div className="inworld-remote-screen">
      <header>
        <div><Wifi size={15} /><b>远程连接</b></div>
        <span>{connected ? 'SECURE SESSION' : 'DEVICE DISCOVERY'}</span>
      </header>
      <div className="remote-screen-body">
        <aside>
          <span>可用设备</span>
          {REMOTE_HOSTS.map((host) => (
            <button
              type="button"
              key={host.id}
              className={selectedHost.id === host.id ? 'is-active' : ''}
              onClick={() => {
                setSelectedHost(host);
                setConnected(false);
              }}
            >
              <Server size={14} />
              <div><b>{host.name}</b><small>{host.detail}</small></div>
              <i className={host.status === '在线' ? 'is-online' : ''} />
            </button>
          ))}
        </aside>
        <main>
          <div className="remote-device-icon"><Server size={34} /></div>
          <span>REMOTE DEVICE</span>
          <h2>{selectedHost.name}</h2>
          <p>{selectedHost.detail} · {selectedHost.status}</p>
          <div className="remote-safety"><Wifi size={13} />端到端加密演示连接</div>
          <button
            type="button"
            disabled={selectedHost.status !== '在线'}
            onClick={() => setConnected((current) => !current)}
          >
            {connected ? '断开连接' : '建立连接'}
          </button>
          {connected ? <strong className="remote-connected">已连接 · 24 ms</strong> : null}
        </main>
      </div>
    </div>
  );
}

type TaskHistoryFilter = 'all' | DemoTask['status'];

const TASK_HISTORY_FILTERS: Array<{ id: TaskHistoryFilter; label: string }> = [
  { id: 'all', label: '全部' },
  { id: 'doing', label: '执行中' },
  { id: 'todo', label: '待处理' },
  { id: 'done', label: '已完成' },
  { id: 'blocked', label: '异常' },
];

const taskMatchesHistoryFilter = (task: DemoTask, filter: TaskHistoryFilter) => (
  filter === 'all' || task.status === filter
);

const buildTaskGraphLayout = (graph: DemoTaskGraph) => {
  const nodeWidth = 250;
  const nodeHeight = 118;
  const horizontalGap = 34;
  const verticalGap = 78;
  const padding = 44;
  const nodeIds = new Set(graph.nodes.map((node) => node.id));
  const indegree = new Map(graph.nodes.map((node) => [node.id, 0]));
  const outgoing = new Map<string, string[]>();
  graph.edges.forEach((edge) => {
    if (!nodeIds.has(edge.source) || !nodeIds.has(edge.target)) return;
    indegree.set(edge.target, (indegree.get(edge.target) || 0) + 1);
    outgoing.set(edge.source, [...(outgoing.get(edge.source) || []), edge.target]);
  });
  const ranks = new Map(graph.nodes.map((node) => [node.id, 0]));
  const queue = graph.nodes.filter((node) => (indegree.get(node.id) || 0) === 0).map((node) => node.id);
  const processed = new Set<string>();
  while (queue.length > 0) {
    const source = queue.shift();
    if (!source || processed.has(source)) continue;
    processed.add(source);
    (outgoing.get(source) || []).forEach((target) => {
      ranks.set(target, Math.max(ranks.get(target) || 0, (ranks.get(source) || 0) + 1));
      const remaining = (indegree.get(target) || 0) - 1;
      indegree.set(target, remaining);
      if (remaining <= 0) queue.push(target);
    });
  }
  graph.nodes.forEach((node) => {
    if (!processed.has(node.id)) ranks.set(node.id, Math.max(0, node.depth));
  });
  const rows = new Map<number, typeof graph.nodes>();
  graph.nodes.forEach((node) => {
    const rank = ranks.get(node.id) || 0;
    rows.set(rank, [...(rows.get(rank) || []), node]);
  });
  const maxColumns = Math.max(1, ...Array.from(rows.values()).map((nodes) => nodes.length));
  const maxRank = Math.max(0, ...Array.from(rows.keys()));
  const contentWidth = padding * 2 + maxColumns * nodeWidth + Math.max(0, maxColumns - 1) * horizontalGap;
  const contentHeight = padding * 2 + (maxRank + 1) * nodeHeight + maxRank * verticalGap;
  const positions = new Map<string, { x: number; y: number }>();
  rows.forEach((nodes, rank) => {
    const rowWidth = nodes.length * nodeWidth + Math.max(0, nodes.length - 1) * horizontalGap;
    const startX = (contentWidth - rowWidth) / 2;
    nodes.forEach((node, index) => {
      positions.set(node.id, {
        x: startX + index * (nodeWidth + horizontalGap),
        y: padding + rank * (nodeHeight + verticalGap),
      });
    });
  });
  return { nodeWidth, nodeHeight, contentWidth, contentHeight, positions };
};

function TaskDependencyGraph({
  graph,
  loading,
  error,
}: {
  graph: DemoTaskGraph;
  loading: boolean;
  error: string | null;
}) {
  const [focusedNodeId, setFocusedNodeId] = useState<string | null>(null);
  const layout = useMemo(() => buildTaskGraphLayout(graph), [graph]);
  const focusedNode = graph.nodes.find((node) => node.id === focusedNodeId) || null;

  useEffect(() => {
    setFocusedNodeId(null);
  }, [graph.sourceSessionId, graph.sourceTurnId, graph.sourceUserMessageId]);

  if (loading) {
    return (
      <div className="task-graph-state is-loading">
        <RefreshCw size={30} />
        <b>正在读取真实任务依赖图</b>
        <span>同步旧版任务看板中的节点和前置关系…</span>
      </div>
    );
  }

  if (graph.nodes.length === 0) {
    return (
      <div className="task-graph-state">
        <CircleAlert size={32} />
        <b>这个任务暂时没有流程图记录</b>
        <span>{error || '旧版 Task Runner 没有为这个会话轮次保存依赖关系。'}</span>
      </div>
    );
  }

  return (
    <div className="task-dependency-graph">
      <div className="task-graph-legend">
        <span><i className="is-doing" />执行中</span>
        <span><i className="is-done" />已完成</span>
        <span><i className="is-blocked" />异常</span>
        <em>{graph.nodes.length} 个节点 · {graph.edges.length} 条依赖</em>
      </div>
      {focusedNode ? (
        <div className="task-graph-focus-card">
          <button type="button" aria-label="关闭节点详情" onClick={() => setFocusedNodeId(null)}><X size={13} /></button>
          <span>当前节点</span>
          <b>{focusedNode.title}</b>
          <small>{focusedNode.detail}</small>
        </div>
      ) : null}
      <div className="task-graph-scroll">
        <div className="task-graph-canvas" style={{ width: layout.contentWidth, height: layout.contentHeight }}>
          <svg width={layout.contentWidth} height={layout.contentHeight} aria-hidden>
            <defs>
              <marker id="workspace-task-arrow" markerWidth="10" markerHeight="10" refX="8" refY="5" orient="auto">
                <path d="M0,0 L10,5 L0,10 z" fill="context-stroke" />
              </marker>
            </defs>
            {graph.edges.map((edge) => {
              const source = layout.positions.get(edge.source);
              const target = layout.positions.get(edge.target);
              const sourceNode = graph.nodes.find((node) => node.id === edge.source);
              const targetNode = graph.nodes.find((node) => node.id === edge.target);
              if (!source || !target) return null;
              const startX = source.x + layout.nodeWidth / 2;
              const startY = source.y + layout.nodeHeight;
              const endX = target.x + layout.nodeWidth / 2;
              const endY = target.y;
              const middleY = startY + (endY - startY) / 2;
              const active = sourceNode?.status === 'doing' || targetNode?.status === 'doing';
              return (
                <path
                  key={edge.id}
                  className={active ? 'is-running' : ''}
                  d={`M ${startX} ${startY} C ${startX} ${middleY}, ${endX} ${middleY}, ${endX} ${endY}`}
                  fill="none"
                  markerEnd="url(#workspace-task-arrow)"
                />
              );
            })}
          </svg>
          {graph.nodes.map((node) => {
            const position = layout.positions.get(node.id);
            if (!position) return null;
            return (
              <button
                type="button"
                key={node.id}
                className={`task-graph-node is-${node.status}${focusedNodeId === node.id ? ' is-focused' : ''}${node.isCurrent ? ' is-current-message' : ''}`}
                style={{ left: position.x, top: position.y, width: layout.nodeWidth, height: layout.nodeHeight }}
                onClick={() => setFocusedNodeId((current) => current === node.id ? null : node.id)}
              >
                <span><i />{STATUS_LABELS[node.status]}{node.isRoot ? ' · 根任务' : ''}</span>
                <b>{node.title}</b>
                <small>{node.detail}</small>
                <em>{node.creatorName || 'Agent'} · {node.updatedAt || '时间未知'}</em>
              </button>
            );
          })}
        </div>
      </div>
    </div>
  );
}

function InWorldTaskWall({
  tasks,
  selectedTask,
  onSelect,
  timeMode,
  graph,
  graphLoading,
  graphError,
  onRefresh,
  onClose,
}: {
  tasks: DemoTask[];
  selectedTask: DemoTask | null;
  onSelect: (task: DemoTask) => void;
  timeMode: TimeMode;
  graph: DemoTaskGraph;
  graphLoading: boolean;
  graphError: string | null;
  onRefresh: () => void;
  onClose: () => void;
}) {
  const [filter, setFilter] = useState<TaskHistoryFilter>('all');
  const filteredTasks = tasks.filter((task) => taskMatchesHistoryFilter(task, filter));
  const activeTask = tasks.find((task) => task.id === selectedTask?.id) || filteredTasks[0] || tasks[0] || null;
  const counts = useMemo(() => ({
    all: tasks.length,
    doing: tasks.filter((task) => task.status === 'doing').length,
    todo: tasks.filter((task) => task.status === 'todo').length,
    done: tasks.filter((task) => task.status === 'done').length,
    blocked: tasks.filter((task) => task.status === 'blocked').length,
  }), [tasks]);

  return (
    <section className={`inworld-task-wall is-focus is-${timeMode}`}>
      <header className="task-center-header">
        <div>
          <span>WORKSPACE TASK CENTER</span>
          <h2>任务运行中心</h2>
          <p>正在执行与历史任务统一查看，流程图来自原 ChatOS Task Runner 依赖关系。</p>
        </div>
        <div className="task-center-header__actions">
          <div className="projection-live"><i /> {counts.doing} RUNNING</div>
          <button type="button" onClick={onRefresh}><RefreshCw size={16} /><span>刷新</span></button>
          <button type="button" onClick={onClose}><X size={17} /><span>返回房间</span></button>
        </div>
      </header>

      <div className="task-center-stats">
        <div><span>全部任务</span><b>{counts.all}</b><small>包含历史记录</small></div>
        <div className="is-doing"><span>正在执行</span><b>{counts.doing}</b><small>实时状态</small></div>
        <div><span>等待处理</span><b>{counts.todo}</b><small>尚未开始</small></div>
        <div className="is-done"><span>已经完成</span><b>{counts.done}</b><small>历史交付</small></div>
        <div className="is-blocked"><span>异常任务</span><b>{counts.blocked}</b><small>阻塞或失败</small></div>
      </div>

      <div className="task-center-body">
        <aside className="task-history-panel">
          <div className="task-history-heading">
            <div><span>TASK HISTORY</span><b>任务记录</b></div>
            <em>{filteredTasks.length} / {tasks.length}</em>
          </div>
          <nav aria-label="任务状态筛选">
            {TASK_HISTORY_FILTERS.map((item) => (
              <button
                type="button"
                key={item.id}
                className={filter === item.id ? 'is-active' : ''}
                onClick={() => {
                  setFilter(item.id);
                  const next = tasks.find((task) => taskMatchesHistoryFilter(task, item.id));
                  if (next) onSelect(next);
                }}
              >
                <span>{item.label}</span>
                <em>{counts[item.id]}</em>
              </button>
            ))}
          </nav>
          <div className="projection-task-list">
            {filteredTasks.map((task) => (
              <button
                type="button"
                key={task.id}
                className={`is-${task.status}${activeTask?.id === task.id ? ' is-active' : ''}`}
                onClick={() => onSelect(task)}
              >
                <i />
                <span>
                  <b>{task.title}</b>
                  <small>{task.detail}</small>
                  <small className="projection-task-meta">{task.conversationTitle || '当前会话'} · {task.updatedAt || task.completedAt || '时间未知'}</small>
                </span>
                <em>{STATUS_LABELS[task.status]}</em>
              </button>
            ))}
            {filteredTasks.length === 0 ? <div className="task-history-empty">这个分类下暂时没有任务</div> : null}
          </div>
        </aside>

        <main className="task-flow-workspace">
          {activeTask ? (
            <>
              <div className="task-flow-heading">
                <div>
                  <span>DEPENDENCY FLOW</span>
                  <h3>{activeTask.title}</h3>
                  <p>{activeTask.detail}</p>
                </div>
                <div className={`task-flow-status is-${activeTask.status}`}>
                  <span>{STATUS_LABELS[activeTask.status]}</span>
                  <b>{activeTask.progress}%</b>
                </div>
              </div>
              <div className="task-flow-meta">
                <span>会话：{activeTask.conversationTitle || '当前会话'}</span>
                <span>创建：{activeTask.createdAt || '时间未知'}</span>
                <span>更新：{activeTask.updatedAt || '时间未知'}</span>
                <span>轮次：{activeTask.conversationTurnId || '无关联轮次'}</span>
              </div>
              <TaskDependencyGraph graph={graph} loading={graphLoading} error={graphError} />
            </>
          ) : (
            <div className={`task-center-empty is-${timeMode}`}>
              <img src={TASK_EMPTY_IMAGES[timeMode]} alt="暂无任务时的默认风景" />
              <div><span>WORKSPACE STANDBY</span><h3>还没有任务记录</h3><p>新任务和已完成的历史任务都会出现在左侧。</p></div>
            </div>
          )}
        </main>
      </div>
    </section>
  );
}

function ProjectDossierFocusLayer({
  project,
  onClose,
}: {
  project: DemoProject;
  onClose: () => void;
}) {
  const contentItems = (project.planItems?.length
    ? project.planItems
    : project.files.map((title) => ({ title, status: null, kind: 'document' as const })))
    .slice(0, 6);
  const counts = project.workItemCounts;
  const sourceLabel = projectSourceLabel(project.sourceType);

  return (
    <section className="dossier-focus-layer" aria-label={`${project.name} 项目档案`}>
      <div className="dossier-focus-desk" />
      <article
        className="dossier-focus-book"
        style={{ '--dossier-accent': project.accent } as React.CSSProperties}
      >
        <i className="dossier-focus-book__spine" />
        <section className="dossier-focus-page is-left">
          <div className="dossier-focus-page__corner">CHATOS / USER PROJECT</div>
          <div className="dossier-focus-stamp">USER PROJECT</div>
          <span className="dossier-focus-index">NO. {project.id.slice(0, 18).toUpperCase()}</span>
          <h1>{project.name}</h1>
          <p className="dossier-focus-subtitle">{project.subtitle}</p>
          <div className="dossier-focus-rule" />
          <p className="dossier-focus-summary">{project.summary}</p>
          <dl className="dossier-focus-status-grid">
            <div><dt>当前状态</dt><dd>{PROJECT_STATUS_LABELS[project.status]}</dd></div>
            <div><dt>项目完成度</dt><dd>{project.progress}%</dd></div>
            <div><dt>工作区来源</dt><dd>{sourceLabel}</dd></div>
          </dl>
          <div className="dossier-focus-identity">
            <span>工作区资料</span>
            <dl>
              <div><dt>项目 ID</dt><dd title={project.id}>{project.id}</dd></div>
              <div><dt>真实路径</dt><dd title={project.rootPath || '未配置'}>{project.rootPath || '未配置'}</dd></div>
              <div><dt>Git 仓库</dt><dd title={project.gitUrl || '未配置'}>{project.gitUrl || '未配置'}</dd></div>
              <div><dt>导入状态</dt><dd>{projectImportLabel(project.importStatus)}</dd></div>
            </dl>
          </div>
          <div className="dossier-focus-dates">
            <div><span>创建时间</span><b>{project.createdAt || '未记录'}</b></div>
            <div><span>最近更新</span><b>{project.updatedAtExact || project.updatedAt}</b></div>
          </div>
          <small className="dossier-focus-page-number">01</small>
        </section>

        <section className="dossier-focus-page is-right">
          <div className="dossier-focus-page__corner">PROJECT CONTENTS</div>
          <span className="dossier-focus-section-title">计划与资料 · {contentItems.length} 条</span>
          <div className="dossier-focus-files">
            {contentItems.map((item, index) => (
              <div key={`${item.title}-${index}`}>
                <span>{String(index + 1).padStart(2, '0')}</span>
                <p>
                  <b title={item.title}>{item.title}</b>
                  <small>{projectItemKindLabel(item.kind)} · {projectItemStatusLabel(item.status)}</small>
                </p>
                <i />
              </div>
            ))}
          </div>
          <div className="dossier-focus-progress">
            <div><span>PROJECT COMPLETION</span><b>{project.progress}%</b></div>
            <div><i style={{ width: `${project.progress}%` }} /></div>
            <p>
              {counts && counts.total > 0
                ? `共 ${counts.total} 项 · ${counts.done} 项完成 · ${counts.running} 项执行中 · ${counts.blocked} 项阻塞`
                : `当前收录 ${contentItems.length} 条项目计划与资料`}
            </p>
          </div>
          <div className="dossier-focus-activity">
            <span>最近活动</span>
            <div><i /><p><b>项目建立</b><small>{project.createdAt || '创建时间未记录'}</small></p></div>
            <div><i className="is-current" /><p><b>资料同步</b><small>{project.updatedAtExact || project.updatedAt} · {projectImportLabel(project.importStatus)}</small></p></div>
            <div><i className="is-accent" /><p><b>当前阶段</b><small>{PROJECT_STATUS_LABELS[project.status]} · 完成度 {project.progress}%</small></p></div>
          </div>
          <small className="dossier-focus-page-number">02</small>
        </section>
      </article>

      <button className="dossier-focus-close" type="button" onClick={onClose}>
        <X size={18} />
        <span>放回书架</span>
      </button>
    </section>
  );
}

function SpatialModeHint({ view, projectName }: { view: ViewMode; projectName: string }) {
  const copy: Partial<Record<ViewMode, { title: string; detail: string }>> = {
    computer: { title: '电脑桌面已全屏', detail: '点击桌面图标打开应用，按 Esc 返回房间。' },
    chat: { title: 'AI 聊天已全屏', detail: '直接输入消息，按 Esc 返回电脑桌面。' },
    terminal: { title: '终端已全屏', detail: '输入命令，按 Esc 返回电脑桌面。' },
    remote: { title: '远程连接已全屏', detail: '选择设备并建立连接，按 Esc 返回电脑桌面。' },
    archive: { title: '左墙用户项目书架', detail: '每页 6 本项目档案册，超过 6 个可在书架铭牌上翻页。' },
    project: { title: `正在翻阅：${projectName}`, detail: '项目资料印在实体档案纸上，按 Esc 放回书架。' },
    projection: { title: '实时任务墙已聚焦', detail: '镜头已经靠近右侧任务画面，点击条目查看状态。' },
  };
  const current = copy[view];
  if (!current) return null;

  return (
    <div className="spatial-mode-hint">
      <b>{current.title}</b>
      <span>{current.detail}</span>
    </div>
  );
}

const DEMO_CHAT_SESSIONS: ChatSession[] = [
  { id: 'demo-general', title: '架构师 · 图灵', projectId: null, updatedAt: '刚刚', archived: false },
  { id: 'demo-room', title: '架构师 · 图灵', projectId: demoProjects[0].id, updatedAt: '刚刚', archived: false },
  { id: 'demo-project', title: '项目管家 · 小旅', projectId: demoProjects[1].id, updatedAt: '18 分钟前', archived: false },
  { id: 'demo-ideas', title: '内容助手 · 知秋', projectId: demoProjects[4].id, updatedAt: '昨天', archived: false },
];

const DEMO_SESSION_CONTACT_IDS: Record<string, string> = {
  'demo-general': 'contact-architect',
  'demo-room': 'contact-architect',
  'demo-project': 'contact-planner',
  'demo-ideas': 'contact-editor',
};

const DEMO_CHAT_CONTACTS: ChatContact[] = [
  { id: 'contact-architect', agentId: 'agent-architect', name: '架构师 · 图灵', description: '负责技术方案、代码实现与系统设计', sessionId: 'demo-general', projectId: null, lastActive: '刚刚' },
  { id: 'contact-planner', agentId: 'agent-planner', name: '项目管家 · 小旅', description: '负责项目拆解、计划推进与风险跟踪', sessionId: null, projectId: null, lastActive: '18 分钟前' },
  { id: 'contact-editor', agentId: 'agent-editor', name: '内容助手 · 知秋', description: '负责资料整理、写作和知识归档', sessionId: null, projectId: null, lastActive: '昨天' },
];

const DEMO_AVAILABLE_AGENTS: ChatAgentOption[] = [
  { id: 'agent-designer', name: '视觉设计师 · 澄空', description: '界面、视觉与交互设计', enabled: true },
  { id: 'agent-tester', name: '测试工程师 · 山雀', description: '自动化测试、质量检查与回归验证', enabled: true },
];

const DEMO_PROJECT_CONTACT_IDS: Record<string, string[]> = {
  [demoProjects[0].id]: ['contact-architect'],
  [demoProjects[1].id]: ['contact-planner'],
  [demoProjects[4].id]: ['contact-editor'],
};

const DEMO_CHAT_MODELS: ChatModelOption[] = [
  { id: 'demo-gpt', name: 'ChatOS 智能模型', modelName: 'chatos-demo', thinkingLevel: 'medium', supportsImages: true, supportsReasoning: true, enabled: true },
  { id: 'demo-fast', name: 'ChatOS 快速模型', modelName: 'chatos-fast-demo', thinkingLevel: 'low', supportsImages: true, supportsReasoning: true, enabled: true },
];

const DEMO_RUNTIME_SETTINGS: ChatRuntimeSettings = {
  selectedModelId: DEMO_CHAT_MODELS[0].id,
  selectedModelName: DEMO_CHAT_MODELS[0].modelName,
  selectedThinkingLevel: DEMO_CHAT_MODELS[0].thinkingLevel,
  reasoningEnabled: true,
  planModeEnabled: false,
};

const EMPTY_DEMO_TASK_GRAPH: DemoTaskGraph = {
  rootTaskIds: [],
  nodes: [],
  edges: [],
  sourceSessionId: null,
  sourceTurnId: null,
  sourceUserMessageId: null,
};

const DEMO_TASK_GRAPH: DemoTaskGraph = {
  rootTaskIds: ['task-fallback'],
  nodes: demoTasks.map((task, index) => ({
    id: task.id,
    title: task.title,
    detail: task.detail,
    status: task.status,
    progress: task.progress,
    depth: index,
    isRoot: task.id === 'task-fallback',
    isCurrent: task.status === 'doing',
    prerequisiteIds: task.id === 'task-assets'
      ? ['task-fallback']
      : task.id === 'task-scene'
        ? ['task-assets']
        : task.id === 'task-chat'
          ? ['task-scene']
          : [],
    creatorName: 'ChatOS Agent',
    updatedAt: task.updatedAt || '刚刚',
    resultSummary: task.status === 'done' ? task.detail : null,
  })),
  edges: [
    { id: 'demo-fallback-assets', source: 'task-fallback', target: 'task-assets' },
    { id: 'demo-assets-scene', source: 'task-assets', target: 'task-scene' },
    { id: 'demo-scene-chat', source: 'task-scene', target: 'task-chat' },
  ],
  sourceSessionId: 'demo-room',
  sourceTurnId: 'demo-task-flow',
  sourceUserMessageId: 'welcome-user',
};

const DEMO_SESSION_MESSAGES: Record<string, ChatMessage[]> = {
  'demo-general': initialMessages,
  'demo-room': initialMessages,
  'demo-project': [
    { id: 'demo-project-user', role: 'user', content: '把旅行项目里的路线和预算整理成一个清晰的执行计划。', time: '09:18' },
    { id: 'demo-project-ai', role: 'assistant', content: '可以。我会按下面顺序整理：\n\n1. 确认城市和日期\n2. 拆分每日路线\n3. 汇总交通、住宿与餐饮预算\n4. 标记需要预订的项目', time: '09:18' },
  ],
  'demo-ideas': [
    { id: 'demo-ideas-ai', role: 'assistant', content: '这里可以存放零散灵感、代码片段和待办。示例代码：\n\n```ts\nconst room = await createWorkspace({ mode: "3d" });\n```', time: '昨天' },
  ],
};

function App() {
  const bridge = useChatOSBridge();
  const [view, setView] = useState<ViewMode>('room');
  const [timeMode, setTimeMode] = useState<TimeMode>('day');
  const [selectedProject, setSelectedProject] = useState<DemoProject>(demoProjects[0]);
  const [selectedTask, setSelectedTask] = useState<DemoTask | null>(demoTasks.find((task) => task.status === 'doing') || null);
  const [demoSessions, setDemoSessions] = useState<ChatSession[]>(DEMO_CHAT_SESSIONS);
  const [demoSessionContactIds, setDemoSessionContactIds] = useState<Record<string, string>>(DEMO_SESSION_CONTACT_IDS);
  const [demoContacts, setDemoContacts] = useState<ChatContact[]>(DEMO_CHAT_CONTACTS);
  const [demoAvailableAgents, setDemoAvailableAgents] = useState<ChatAgentOption[]>(DEMO_AVAILABLE_AGENTS);
  const [demoProjectContactIds, setDemoProjectContactIds] = useState<Record<string, string[]>>(DEMO_PROJECT_CONTACT_IDS);
  const [demoActiveContactId, setDemoActiveContactId] = useState<string | null>('contact-architect');
  const [demoConversationId, setDemoConversationId] = useState<string | null>('demo-room');
  const [demoMessagesBySession, setDemoMessagesBySession] = useState<Record<string, ChatMessage[]>>(DEMO_SESSION_MESSAGES);
  const [demoRuntimeSettings, setDemoRuntimeSettings] = useState<ChatRuntimeSettings>(DEMO_RUNTIME_SETTINGS);
  const [demoActiveProjectId, setDemoActiveProjectId] = useState<string | null>(DEMO_CHAT_SESSIONS[0].projectId);
  const [demoThinking, setDemoThinking] = useState(false);
  const [demoDesktop, setDemoDesktop] = useState(false);
  const [catToast, setCatToast] = useState(false);
  const demoThinkingTimerRef = useRef<number | null>(null);
  const live = bridge.status === 'live';
  const projects = live ? bridge.projects : demoProjects;
  const tasks = live ? bridge.runningTasks : demoTasks;
  const runningTaskPreview = tasks.filter((task) => task.status === 'doing');
  const messages = live ? bridge.messages : (demoConversationId ? demoMessagesBySession[demoConversationId] || [] : []);
  const demoScopedContactIds = demoActiveProjectId ? (demoProjectContactIds[demoActiveProjectId] || []) : demoContacts.map((contact) => contact.id);
  const demoScopedContacts = demoContacts.filter((contact) => demoScopedContactIds.includes(contact.id)).map((contact) => {
    const session = demoSessions.find((item) => item.projectId === demoActiveProjectId && demoSessionContactIds[item.id] === contact.id);
    return { ...contact, projectId: demoActiveProjectId, sessionId: session?.id || null, lastActive: session?.updatedAt || contact.lastActive };
  });
  const chatContacts = live ? bridge.contacts : demoScopedContacts;
  const accountContacts = live ? bridge.accountContacts : demoContacts;
  const availableAgents = live ? bridge.availableAgents : demoAvailableAgents;
  const models = live ? bridge.models : DEMO_CHAT_MODELS;
  const runtimeSettings = live ? bridge.runtimeSettings : demoRuntimeSettings;
  const activeProjectId = live ? bridge.activeProjectId : demoActiveProjectId;
  const thinking = live ? bridge.thinking : demoThinking;
  const activeContactId = live ? bridge.activeContactId : demoActiveContactId;
  const conversationId = live ? bridge.conversationId : demoConversationId;
  const conversationTitle = live
    ? bridge.conversationTitle
    : demoContacts.find((contact) => contact.id === demoActiveContactId)?.name || null;
  const computerUnlocked = live || demoDesktop;

  useEffect(() => {
    if (projects.length === 0) return;
    setSelectedProject((current) => projects.find((project) => project.id === current.id) || projects[0]);
  }, [projects]);

  useEffect(() => {
    setSelectedTask((current) => current ? tasks.find((task) => task.id === current.id) || tasks[0] || null : tasks[0] || null);
  }, [tasks]);

  useEffect(() => () => {
    if (demoThinkingTimerRef.current !== null) window.clearTimeout(demoThinkingTimerRef.current);
  }, []);

  useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        setView((current) => {
          if (current === 'project') return 'archive';
          if (current === 'chat' || current === 'terminal' || current === 'remote') return 'computer';
          return 'room';
        });
        return;
      }
      if (view !== 'room') return;
      if (event.key === 'ArrowLeft') setView('archive');
      if (event.key === 'ArrowRight') setView('projection');
      if (event.key === 'Enter') setView('computer');
    };
    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [view]);

  const handleProjectSelect = (project: DemoProject) => {
    setSelectedProject(project);
    setView('project');
    if (bridge.status === 'live') void bridge.selectProject(project.id);
  };

  const handleSend = async (content: string, files: File[] = []) => {
    if (live) {
      await bridge.sendMessage(content, files);
      return;
    }
    if (!demoActiveContactId) throw new Error('请先选择联系人或项目负责人');
    let targetConversationId = demoConversationId;
    if (!targetConversationId) {
      targetConversationId = `demo-${Date.now()}`;
      const contact = demoContacts.find((item) => item.id === demoActiveContactId);
      const session: ChatSession = { id: targetConversationId, title: contact?.name || '联系人', projectId: demoActiveProjectId, updatedAt: '刚刚', archived: false };
      setDemoSessions((current) => [session, ...current]);
      setDemoSessionContactIds((current) => ({ ...current, [targetConversationId as string]: demoActiveContactId }));
      setDemoMessagesBySession((current) => ({ ...current, [targetConversationId as string]: [] }));
      setDemoConversationId(targetConversationId);
    }
    const activeConversationId = targetConversationId;
    const userMessage: ChatMessage = {
      id: `user-${Date.now()}`,
      role: 'user',
      content,
      time: formatTime(),
      attachments: files.map((file, index) => ({
        id: `demo-file-${Date.now()}-${index}`,
        name: file.name,
        mimeType: file.type || 'application/octet-stream',
        size: file.size,
        type: file.type.startsWith('image/') ? 'image' : file.type.startsWith('audio/') ? 'audio' : 'file',
      })),
    };
    setDemoMessagesBySession((current) => ({
      ...current,
      [activeConversationId]: [...(current[activeConversationId] || []), userMessage],
    }));
    setDemoSessions((current) => current.map((session) => session.id === activeConversationId ? { ...session, updatedAt: '刚刚' } : session));
    setDemoThinking(true);

    demoThinkingTimerRef.current = window.setTimeout(() => {
      const assistantMessage: ChatMessage = {
        id: `assistant-${Date.now()}`,
        role: 'assistant',
        content: files.length > 0
          ? `收到消息和 ${files.length} 个附件。演示模式已经完成附件预览与发送流程；登录真实 ChatOS 后会按原前端格式传给后端。`
          : '收到。联系人和项目负责人关系已经按原前端处理；首次发送消息时才会自动建立对应会话。',
        time: formatTime(),
      };
      setDemoMessagesBySession((current) => ({
        ...current,
        [activeConversationId]: [...(current[activeConversationId] || []), assistantMessage],
      }));
      setDemoThinking(false);
      demoThinkingTimerRef.current = null;
    }, 850);
  };

  const stopDemoMessage = () => {
    if (demoThinkingTimerRef.current !== null) window.clearTimeout(demoThinkingTimerRef.current);
    demoThinkingTimerRef.current = null;
    setDemoThinking(false);
  };

  const selectDemoContact = (contactId: string) => {
    setDemoActiveContactId(contactId);
    const session = demoSessions.find((item) => item.projectId === demoActiveProjectId && demoSessionContactIds[item.id] === contactId);
    setDemoConversationId(session?.id || null);
  };

  const addDemoContact = (agentId: string) => {
    const agent = demoAvailableAgents.find((item) => item.id === agentId);
    if (!agent) return;
    const contact: ChatContact = { id: `contact-${Date.now()}`, agentId: agent.id, name: agent.name, description: agent.description, sessionId: null, projectId: null, lastActive: '刚刚' };
    setDemoContacts((current) => [...current, contact]);
    setDemoAvailableAgents((current) => current.filter((item) => item.id !== agentId));
    setDemoActiveProjectId(null);
    setDemoActiveContactId(contact.id);
    setDemoConversationId(null);
  };

  const deleteDemoContact = (contactId: string) => {
    const remaining = demoContacts.filter((contact) => contact.id !== contactId);
    setDemoContacts(remaining);
    setDemoProjectContactIds((current) => Object.fromEntries(Object.entries(current).map(([projectId, ids]) => [projectId, ids.filter((id) => id !== contactId)])));
    if (demoActiveContactId === contactId) {
      setDemoActiveContactId(remaining[0]?.id || null);
      const session = remaining[0] ? demoSessions.find((item) => item.projectId === null && demoSessionContactIds[item.id] === remaining[0].id) : null;
      setDemoConversationId(session?.id || null);
    }
  };

  const assignDemoProjectContact = (contactId: string) => {
    if (!demoActiveProjectId) return;
    setDemoProjectContactIds((current) => ({ ...current, [demoActiveProjectId]: Array.from(new Set([...(current[demoActiveProjectId] || []), contactId])) }));
    setDemoActiveContactId(contactId);
    const session = demoSessions.find((item) => item.projectId === demoActiveProjectId && demoSessionContactIds[item.id] === contactId);
    setDemoConversationId(session?.id || null);
  };

  const removeDemoProjectContact = (contactId: string) => {
    if (!demoActiveProjectId) return;
    const remaining = (demoProjectContactIds[demoActiveProjectId] || []).filter((id) => id !== contactId);
    setDemoProjectContactIds((current) => ({ ...current, [demoActiveProjectId]: remaining }));
    if (demoActiveContactId === contactId) {
      const nextId = remaining[0] || null;
      setDemoActiveContactId(nextId);
      const session = nextId ? demoSessions.find((item) => item.projectId === demoActiveProjectId && demoSessionContactIds[item.id] === nextId) : null;
      setDemoConversationId(session?.id || null);
    }
  };

  const handleChatProjectChange = async (projectId: string | null) => {
    if (live) {
      if (projectId) await bridge.selectProject(projectId);
      else await bridge.selectPersonalContacts();
      return;
    }
    setDemoActiveProjectId(projectId);
    const contactId = projectId ? (demoProjectContactIds[projectId] || [])[0] || null : demoContacts[0]?.id || null;
    setDemoActiveContactId(contactId);
    const session = contactId ? demoSessions.find((item) => item.projectId === projectId && demoSessionContactIds[item.id] === contactId) : null;
    setDemoConversationId(session?.id || null);
  };

  const handleRuntimeChange = async (patch: Partial<ChatRuntimeSettings>) => {
    if (live) {
      await bridge.updateRuntimeSettings(patch);
      return;
    }
    setDemoRuntimeSettings((current) => ({ ...current, ...patch }));
  };

  const handleCatPet = () => {
    setCatToast(true);
    window.setTimeout(() => setCatToast(false), 2200);
  };

  useEffect(() => {
    if (tasks.length === 0) {
      setSelectedTask(null);
      return;
    }
    if (!selectedTask || !tasks.some((task) => task.id === selectedTask.id)) {
      setSelectedTask(tasks.find((task) => task.status === 'doing') || tasks[0]);
    }
  }, [selectedTask, tasks]);

  useEffect(() => {
    if (!live || view !== 'projection' || !selectedTask) return;
    void bridge.loadTaskGraph(selectedTask);
  }, [live, selectedTask, view]); // eslint-disable-line react-hooks/exhaustive-deps

  const handleTaskCenterRefresh = () => {
    if (!live) return;
    void bridge.refresh();
    if (selectedTask) void bridge.loadTaskGraph(selectedTask);
  };

  const computerImmersive = view === 'computer' || view === 'chat' || view === 'terminal' || view === 'remote';
  const projectImmersive = view === 'project';
  const taskImmersive = view === 'projection';
  const immersiveView = computerImmersive || projectImmersive || taskImmersive;
  const activeSelectedProject = projects.find((project) => project.id === selectedProject.id) || selectedProject;
  const activeTaskGraph = live
    ? bridge.taskGraphTaskId === selectedTask?.id ? bridge.taskGraph : EMPTY_DEMO_TASK_GRAPH
    : DEMO_TASK_GRAPH;
  const activeTaskGraphLoading = live && Boolean(selectedTask)
    ? bridge.taskGraphTaskId !== selectedTask?.id || bridge.taskGraphLoading
    : false;
  const activeTaskGraphError = live && bridge.taskGraphTaskId === selectedTask?.id ? bridge.taskGraphError : null;
  const exitImmersiveView = () => {
    setView((current) => current === 'chat' || current === 'terminal' || current === 'remote' ? 'computer' : 'room');
  };

  const lockComputer = () => {
    if (bridge.user) bridge.logout();
    setDemoDesktop(false);
    setView('computer');
  };

  return (
    <div className={`app-shell time-${timeMode}${projectImmersive ? ' is-project-reading' : ''}${taskImmersive ? ' is-task-center' : ''}`}>
      <div className={projectImmersive ? 'room-stage is-project-hidden' : taskImmersive ? 'room-stage is-task-hidden' : computerImmersive ? 'room-stage is-suspended' : 'room-stage'} aria-hidden={computerImmersive || projectImmersive || taskImmersive}>
        <Suspense fallback={<SceneLoading />}>
          <Canvas
          className="room-canvas"
          frameloop="demand"
          shadows="percentage"
          dpr={[1, 1.5]}
          camera={{ position: [0, 2.62, 6.7], fov: 47, near: 0.1, far: 60 }}
          gl={{ antialias: true, alpha: false, powerPreference: 'high-performance' }}
          onCreated={({ gl }) => {
            gl.toneMapping = ACESFilmicToneMapping;
            gl.toneMappingExposure = timeMode === 'night' ? 0.74 : timeMode === 'sunset' ? 0.79 : 0.82;
            gl.outputColorSpace = SRGBColorSpace;

            const canvas = gl.domElement;
            const handleContextLost = (event: Event) => {
              event.preventDefault();
            };
            const handleContextRestored = () => {
              gl.resetState();
            };
            canvas.addEventListener('webglcontextlost', handleContextLost, false);
            canvas.addEventListener('webglcontextrestored', handleContextRestored, false);
          }}
          onPointerMissed={() => {
            if (view === 'room') return;
          }}
          >
            <RealisticRoom
            view={view}
            timeMode={timeMode}
            projects={projects}
            tasks={runningTaskPreview}
            computerLocked={!computerUnlocked}
            computerScreenCovered={computerImmersive}
            computerContent={null}
            taskWallContent={null}
            onViewChange={setView}
            onComputerLock={lockComputer}
            onProjectSelect={handleProjectSelect}
            onCatPet={handleCatPet}
            />
          </Canvas>
        </Suspense>
      </div>

      <div className="vignette" />
      {computerImmersive ? (
        <ComputerFocusLayer>
          {view === 'computer' ? (
            computerUnlocked ? (
              <FocusDesktop
                timeMode={timeMode}
                onChat={() => setView('chat')}
                onTerminal={() => setView('terminal')}
                onRemote={() => setView('remote')}
                onLock={lockComputer}
              />
            ) : (
              <InWorldLoginScreen bridge={bridge} onDemo={() => setDemoDesktop(true)} />
            )
          ) : view === 'terminal' ? (
            <InWorldTerminalScreen />
          ) : view === 'remote' ? (
            <InWorldRemoteScreen />
          ) : (
            <InWorldChatScreen
              messages={messages}
              contacts={chatContacts}
              accountContacts={accountContacts}
              availableAgents={availableAgents}
              models={models}
              projects={projects}
              runtimeSettings={runtimeSettings}
              activeProjectId={activeProjectId}
              activeContactId={activeContactId}
              thinking={thinking}
              isStopping={live ? bridge.isStopping : false}
              loadingMessages={live ? bridge.loadingMessages : false}
              hasMoreMessages={live ? bridge.hasMoreMessages : false}
              sessionBusy={live ? bridge.sessionBusy : false}
              onSend={handleSend}
              onStop={live ? bridge.stopMessage : stopDemoMessage}
              live={live}
              webSocketStatus={live ? bridge.webSocketStatus : 'demo'}
              error={live ? bridge.error : null}
              conversationId={conversationId}
              conversationTitle={conversationTitle}
              onSelectContact={live ? bridge.selectContact : selectDemoContact}
              onAddContact={live ? bridge.addContact : addDemoContact}
              onDeleteContact={live ? bridge.deleteContact : deleteDemoContact}
              onAssignProjectContact={live ? bridge.assignProjectContact : assignDemoProjectContact}
              onRemoveProjectContact={live ? bridge.removeProjectContact : removeDemoProjectContact}
              onRefresh={live ? bridge.refresh : () => undefined}
              onLoadMore={live ? bridge.loadMoreMessages : () => undefined}
              onRuntimeChange={handleRuntimeChange}
              onProjectChange={handleChatProjectChange}
            />
          )}
        </ComputerFocusLayer>
      ) : null}
      {projectImmersive ? (
        <ProjectDossierFocusLayer
          project={activeSelectedProject}
          onClose={() => setView('archive')}
        />
      ) : null}
      {taskImmersive ? (
        <InWorldTaskWall
          tasks={tasks}
          selectedTask={selectedTask}
          onSelect={setSelectedTask}
          timeMode={timeMode}
          graph={activeTaskGraph}
          graphLoading={activeTaskGraphLoading}
          graphError={activeTaskGraphError}
          onRefresh={handleTaskCenterRefresh}
          onClose={() => setView('room')}
        />
      ) : null}
      {!immersiveView ? <TopBar view={view} timeMode={timeMode} onTimeModeChange={setTimeMode} /> : null}

      {view === 'room' ? <RoomHint /> : null}
      {!immersiveView ? <SpatialModeHint view={view} projectName={activeSelectedProject.name} /> : null}
      {computerImmersive ? (
        <button className={computerImmersive ? 'immersive-exit is-computer' : 'immersive-exit'} type="button" onClick={exitImmersiveView}>
          <X size={15} />
          <span>{view === 'chat' || view === 'terminal' || view === 'remote' ? '返回桌面' : '退出全屏'}</span>
        </button>
      ) : null}
      {view === 'phone' ? (
        <PhoneWorkspace
          timeMode={timeMode}
          onTimeModeChange={setTimeMode}
          onClose={() => setView('room')}
        />
      ) : null}

      {catToast ? (
        <div className="cat-toast">
          <CatIcon size={20} />
          <span>小猫发出了满意的呼噜声。</span>
        </div>
      ) : null}

      {!immersiveView ? <BottomNavigation view={view} onViewChange={setView} /> : null}

    </div>
  );
}

export default App;
