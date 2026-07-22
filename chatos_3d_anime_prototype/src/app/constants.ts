import { MoonStar, Sun, Sunset } from 'lucide-react';
import type { DemoProject, DemoTask, TimeMode, ViewMode } from '../types';

export const TIME_MODES: Array<{ mode: TimeMode; label: string; icon: typeof Sun }> = [
  { mode: 'day', label: '白天', icon: Sun },
  { mode: 'sunset', label: '黄昏', icon: Sunset },
  { mode: 'night', label: '夜晚', icon: MoonStar },
];

export const VIEW_LABELS: Record<ViewMode, string> = {
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

export const STATUS_LABELS: Record<DemoTask['status'], string> = {
  doing: '进行中',
  todo: '待处理',
  blocked: '阻塞',
  done: '已完成',
};

export const TASK_EMPTY_IMAGES: Record<TimeMode, string> = {
  day: '/assets/window-day.jpg',
  sunset: '/assets/window-sunset.jpg',
  night: '/assets/window-night.jpg',
};

export const PROJECT_STATUS_LABELS: Record<DemoProject['status'], string> = {
  running: '运行中',
  planning: '规划中',
  idle: '空闲',
};
