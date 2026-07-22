import { Archive, CloudSun, Home, Laptop, PanelRightOpen, Smartphone, Sparkles } from 'lucide-react';
import type { TimeMode, ViewMode } from '../types';
import { TIME_MODES, VIEW_LABELS } from './constants';

export function SceneLoading() {
  return (
    <div className="scene-loading">
      <Sparkles size={20} />
      <span>正在进入你的写实 3D 书房…</span>
    </div>
  );
}

export function TopBar({
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

export function BottomNavigation({
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

export function RoomHint() {
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
