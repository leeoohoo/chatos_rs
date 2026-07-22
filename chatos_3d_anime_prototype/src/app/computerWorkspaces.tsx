import {
  Bot,
  Cat as CatIcon,
  CheckCircle2,
  Home,
  LockKeyhole,
  LogIn,
  RefreshCw,
  Server,
  Smartphone,
  Sparkles,
  Terminal,
  Wifi,
  X,
} from 'lucide-react';
import { FormEvent, useState, type ReactNode } from 'react';
import { demoProjects } from '../demoData';
import type { TimeMode } from '../types';
import { useChatOSBridge } from '../useChatOSBridge';
import { TIME_MODES } from './constants';

const formatTime = () => new Intl.DateTimeFormat('zh-CN', {
  hour: '2-digit',
  minute: '2-digit',
  hour12: false,
}).format(new Date());

const formatPhoneDate = () => {
  const now = new Date();
  const date = new Intl.DateTimeFormat('zh-CN', { month: 'long', day: 'numeric' }).format(now);
  const weekday = new Intl.DateTimeFormat('zh-CN', { weekday: 'short' }).format(now);
  return `${date} · ${weekday}`;
};

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

export function PhoneWorkspace({ timeMode, onTimeModeChange, onClose }: {
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

export function InWorldLoginScreen({
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

export function FocusDesktop({
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

export function ComputerFocusLayer({ children }: { children: ReactNode }) {
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

export function InWorldTerminalScreen() {
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

export function InWorldRemoteScreen() {
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
