// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { type CSSProperties, type FormEvent, useEffect, useMemo, useRef, useState } from 'react';
import {
  ArrowRight, BrainCircuit, Check, ChevronRight, Download, Laptop,
  Mail, MonitorDown, ShieldCheck, Sparkles, TerminalSquare, Workflow,
} from 'lucide-react';
import { BrandMark } from './BrandMark';

type SiteManifest = { product_name: string; tagline: string; app_url: string; registration_enabled: boolean; downloads_enabled: boolean };
type ClientArtifact = { platform: string; label: string; file_name: string; content_type: string; size_bytes: number; sha256: string; download_url: string };
type ClientRelease = { product: string; channel: string; version: string; published_at: string; artifacts: ClientArtifact[] };
type DownloadCatalog = { storage_configured: boolean; available: boolean; message: string; release?: ClientRelease | null };
type RegistrationForm = { email: string; displayName: string; inviteCode: string; verificationCode: string; password: string; confirmPassword: string };

const productName = '叽咕狸';
const heroVideos = [
  { src: '/showcase/videos/hero-work.mp4', poster: '/showcase/videos/hero-work-poster.webp' },
  { src: '/showcase/videos/hero-build.mp4', poster: '/showcase/videos/hero-build-poster.webp' },
  { src: '/showcase/videos/hero-team.mp4', poster: '/showcase/videos/hero-team-poster.webp' },
  { src: '/showcase/videos/hero-detail.mp4', poster: '/showcase/videos/hero-detail-poster.webp' },
];
const fallbackManifest: SiteManifest = { product_name: productName, tagline: '把每一个工具做好，陪你把每一件事做好。', app_url: '#download', registration_enabled: true, downloads_enabled: false };
const initialRegistration: RegistrationForm = { email: '', displayName: '', inviteCode: '', verificationCode: '', password: '', confirmPassword: '' };
const transitionText = '想要做好一件事，要学会让自己慢下来。';

function buildInitialHeroPath() {
  return 'M .08 0 L .31 0 L .31 .58 C .31 .78 .39 .87 .5 .87 C .61 .87 .69 .78 .69 .58 L .69 0 L .92 0 L .92 .58 C .92 .86 .74 1 .5 1 C .26 1 .08 .86 .08 .58 L .08 0 Z';
}

const capabilities = [
  { icon: BrainCircuit, en: 'Context', title: '资料放在对的位置', body: '项目背景、关键决定和工作记录各归其位，需要时找得到，也接得上。', pet: 'pet-thinking' },
  { icon: TerminalSquare, en: 'Tools', title: '每个工具都要顺手', body: '文件、终端、Git 与创作工具认真打磨，让每一步操作简单、直接、可用。', pet: 'pet-walking' },
  { icon: Workflow, en: 'Process', title: '过程始终看得见', body: '任务做到哪一步、用了什么工具、产生什么结果，都清楚地摆在你面前。', pet: 'pet-working' },
  { icon: ShieldCheck, en: 'Control', title: '决定始终交给人', body: '工作区单独授权，重要动作由你确认；工具负责协助，判断始终属于你。', pet: 'pet-calm' },
];

const steps = [
  { number: '01', title: '先把事情说清', body: '从一句自然语言开始，一起明确目标、约束和完成标准，不急着替你做决定。' },
  { number: '02', title: '用好每一个工具', body: '按需连接项目文件、终端、Git 与创作工具，让每一步都发生在正确的位置。' },
  { number: '03', title: '结果交给你确认', body: '过程随时看得见，关键动作由你把关；完成的结果回到项目里，继续为下一步服务。' },
];

const faqs = [
  { question: '叽咕狸和普通 AI 聊天工具有什么不同？', answer: '叽咕狸不把“更聪明”当作终点，而是从真实工作需要的每一个工具开始：项目、文件、终端、Git、创作与协作都放在清楚可控的工作台里，帮助你一步步把事情做好。' },
  { question: '必须把整个项目上传到云端吗？', answer: '不需要。项目目录由桌面客户端直接连接和管理；文件浏览、编辑、Git 与终端操作都围绕你在客户端中创建的本机项目进行。' },
  { question: '为什么注册需要邀请码？', answer: '目前仍处于邀请测试阶段，我们希望控制服务容量并认真处理每一条反馈。获得邀请码后，可在官网完成邮箱验证与注册。' },
  { question: '桌面客户端支持哪些系统？', answer: '当前官网优先提供 Windows 10/11 64 位安装包，macOS 原生客户端正在持续测试和完善。' },
];

function PetSprite({ className = '', label }: { className?: string; label?: string }) {
  return <span className={`pet-sprite ${className}`} role={label ? 'img' : undefined} aria-label={label} aria-hidden={label ? undefined : true} />;
}

function MockWindowBar({ title }: { title: string }) {
  return <div className="mock-windowbar"><span className="mock-traffic"><i /><i /><i /></span><strong>{title}</strong><span className="mock-window-actions">⌕　•••</span></div>;
}

function ClientWorkspacePreview() {
  return <div className="client-mock client-mock-main" aria-label="叽咕狸桌面客户端项目工作区示意图">
    <MockWindowBar title="叽咕狸 · 官网项目" />
    <div className="mock-app-shell">
      <aside className="mock-app-sidebar">
        <b>叽咕狸</b>
        <small>联系人</small><span>◉ 产品讨论</span><span>◉ 设计搭档</span>
        <small>项目</small><span className="active">▣ 官网改版</span><span>▣ 桌面客户端</span>
        <small>工作台</small><span>▦ 应用</span><span>✦ AI 创作</span><span>◎ Agent</span>
      </aside>
      <div className="mock-project">
        <div className="mock-project-head"><div><strong>官网改版</strong><small><i /> 本地项目已连接</small></div><nav><span>项目目录</span><span className="active">用户消息</span><span>项目设置</span></nav></div>
        <div className="mock-project-body">
          <div className="mock-file-panel"><div className="mock-search">搜索项目文件</div><small>PROJECT</small><span>⌄ frontend</span><span>　⌄ src</span><span className="active">　　App.tsx</span><span>　　home.css</span><span>　　BrandMark.tsx</span><span>　⌄ public</span><span>　　brand</span><span>　　showcase</span><span>README.md</span><div className="mock-git"><b>3</b><small>个文件已修改</small></div></div>
          <div className="mock-chat-panel"><div className="mock-message user">把官网改成真正展示客户端的版本。</div><div className="mock-message ai"><b>叽咕狸</b><p>我会先核对当前客户端功能，再更新官网展示和文档。</p><div className="mock-task"><i /> 正在检查客户端界面 <span>运行中</span></div><div className="mock-steps"><span>✓ 读取项目结构</span><span>✓ 核对产品能力</span><span>✓ 更新三个客户端场景</span><span>↻ 检查响应式布局</span></div><div className="mock-delivery"><span><small>当前任务</small><b>官网客户端展示</b></span><span><small>进度</small><b>78%</b></span><i><em /></i></div></div><div className="mock-activity-grid"><section><small>运行终端</small><b>npm run build</b><code>✓ 1782 modules transformed<br />✓ built in 591ms</code></section><section><small>Git 变更</small><b>3 个文件待提交</b><span>App.tsx　+42</span><span>home.css　+68</span></section></div><div className="mock-composer">继续告诉我你的想法… <b>↑</b></div></div>
        </div>
      </div>
    </div>
  </div>;
}

function CreationStudioPreview() {
  return <div className="client-mock compact-mock creation-mock" aria-label="叽咕狸客户端 AI 创作工作台示意图">
    <MockWindowBar title="叽咕狸 · AI 创作" />
    <div className="mock-studio-head"><b>✦ AI 创作</b><span>图片</span><span>视频</span><span className="active">剧情模式</span><span>记录</span></div>
    <div className="mock-studio-body"><aside><small>剧情时间线</small><b>01　清晨的工作室</b><b className="active">02　灵感变成画面</b><b>03　狐狸进入镜头</b><b>04　团队开始协作</b><b>05　交付完成</b><div className="studio-count"><span>5 个分段</span><span>总时长 00:28</span></div><button>＋ 添加分段</button></aside><div className="mock-frame"><div className="frame-toolbar"><span>第二段 · 预览</span><span>16:9　1080P　5 秒</span></div><div className="frame-picture"><span>JIGULI STORY　/　SCENE 02</span><strong>让想法<br />开始流动</strong><i>▶</i><small>首帧</small></div><div className="frame-status"><span>首帧已确认</span><span>尾帧已确认</span><span>运镜 · 缓慢推进</span><b>生成本段视频 →</b></div><div className="frame-progress"><div><span>生成队列</span><b>场景 02 正在渲染</b></div><strong>68%</strong><i><em /></i></div></div></div>
  </div>;
}

function AgentWorkspacePreview() {
  return <div className="client-mock compact-mock agent-mock" aria-label="叽咕狸客户端 Agent 团队工作台示意图">
    <MockWindowBar title="叽咕狸 · Agent" />
    <div className="mock-agent-body"><aside><b>Agent</b><small>私聊与项目团队</small><span>▦ Agent 管理</span><small>团队</small><span className="active">◉ 官网发布团队　3</span><span>◉ 客户端研发　5</span><span>◉ 内容创作　2</span><small>私聊</small><span>设计 Agent</span><span>开发 Agent</span><span>研究 Agent</span><div className="agent-online"><i /><span>8 个 Agent 在线</span></div></aside><div className="mock-agent-chat"><div className="agent-title"><div><b>官网发布团队</b><small>3 位 Agent 正在协作 · 4 项任务</small></div><span>＋ 添加成员</span></div><div className="agent-summary"><span><b>4</b><small>全部任务</small></span><span><b>2</b><small>进行中</small></span><span><b>1</b><small>待确认</small></span><span><b>76%</b><small>总进度</small></span></div><div className="agent-note orange"><i>D</i><p><b>设计 Agent</b><small>新版客户端展示已经完成，正在检查移动端布局。</small></p><em>刚刚</em></div><div className="agent-note blue"><i>C</i><p><b>开发 Agent</b><small>官网构建通过，历史后台素材与引用已移除。</small></p><em>2 分钟</em></div><div className="agent-note purple"><i>R</i><p><b>研究 Agent</b><small>已整理同类产品的动效节奏与信息层级。</small></p><em>5 分钟</em></div><div className="agent-note green"><i>✓</i><p><b>项目经理</b><small>3 / 4 已完成 · 等待最终验收</small></p><em>现在</em></div><div className="agent-board"><div><small>NEXT UP</small><b>待办与交付</b></div><span><i className="done">✓</i><b>更新客户端场景</b><p>三组场景结构与动效节奏已完成，等待最终视觉确认。</p><small>3 / 3 检查项完成</small><em>设计 Agent</em></span><span><i>↻</i><b>响应式视觉检查</b><p>正在验证超宽屏、桌面与移动端的布局稳定性。</p><small>6 / 8 视口通过</small><em>开发 Agent</em></span><span><i>→</i><b>整理上线说明</b><p>汇总本轮更新内容，准备版本说明与发布清单。</p><small>预计 10 分钟</small><em>项目经理</em></span></div></div></div>
  </div>;
}

function App() {
  const heroRef = useRef<HTMLElement>(null);
  const sceneRef = useRef<HTMLElement>(null);
  const heroClipPathRef = useRef<SVGPathElement>(null);
  const [manifest, setManifest] = useState<SiteManifest>(fallbackManifest);
  const [downloads, setDownloads] = useState<DownloadCatalog | null>(null);
  const [registration, setRegistration] = useState<RegistrationForm>(initialRegistration);
  const [registrationState, setRegistrationState] = useState<'idle' | 'submitting' | 'success'>('idle');
  const [codeSending, setCodeSending] = useState(false);
  const [codeCountdown, setCodeCountdown] = useState(0);
  const [formMessage, setFormMessage] = useState<string | null>(null);
  const [activeVideo, setActiveVideo] = useState(0);

  useEffect(() => {
    document.title = '叽咕狸 | 把每一个工具做好，陪你把每一件事做好';
    let cancelled = false;
    Promise.all([
      fetch('/api/site/manifest').then((response) => response.ok ? response.json() as Promise<SiteManifest> : fallbackManifest),
      fetch('/api/site/downloads').then((response) => response.ok ? response.json() as Promise<DownloadCatalog> : null),
    ]).then(([manifestPayload, downloadPayload]) => {
      if (!cancelled) { setManifest({ ...manifestPayload, product_name: productName }); setDownloads(downloadPayload); }
    }).catch(() => { if (!cancelled) setManifest(fallbackManifest); });
    return () => { cancelled = true; };
  }, []);

  useEffect(() => {
    const timer = window.setInterval(() => setActiveVideo((current) => (current + 1) % heroVideos.length), 4200);
    return () => window.clearInterval(timer);
  }, []);

  useEffect(() => {
    const hero = heroRef.current;
    if (!hero) return undefined;
    let animationFrame = 0;
    const clamp01 = (value: number) => Math.min(1, Math.max(0, value));
    const smoothstep = (from: number, to: number, value: number) => {
      const amount = clamp01((value - from) / (to - from));
      return amount * amount * (3 - 2 * amount);
    };
    const uPathValues = [
      .08, 0, .31, 0, .31, .58,
      .31, .78, .39, .87, .5, .87,
      .61, .87, .69, .78, .69, .58,
      .69, 0, .92, 0, .92, .58,
      .92, .86, .74, 1, .5, 1,
      .26, 1, .08, .86, .08, .58,
      .08, 0,
    ];
    const capsulePathValues = [
      .06, 0, .2, 0, .36, 0,
      .4, 0, .45, 0, .5, 0,
      .55, 0, .6, 0, .64, 0,
      .8, 0, .94, 0, 1, .5,
      1, .82, .76, 1, .5, 1,
      .24, 1, 0, .82, 0, .5,
      .06, 0,
    ];
    const buildMorphPath = (phase: number) => {
      const value = (index: number) => {
        const result = uPathValues[index] + (capsulePathValues[index] - uPathValues[index]) * phase;
        return result.toFixed(4);
      };
      return `M ${value(0)} ${value(1)} L ${value(2)} ${value(3)} L ${value(4)} ${value(5)} C ${value(6)} ${value(7)} ${value(8)} ${value(9)} ${value(10)} ${value(11)} C ${value(12)} ${value(13)} ${value(14)} ${value(15)} ${value(16)} ${value(17)} L ${value(18)} ${value(19)} L ${value(20)} ${value(21)} L ${value(22)} ${value(23)} C ${value(24)} ${value(25)} ${value(26)} ${value(27)} ${value(28)} ${value(29)} C ${value(30)} ${value(31)} ${value(32)} ${value(33)} ${value(34)} ${value(35)} L ${value(36)} ${value(37)} Z`;
    };
    const updateHero = () => {
      animationFrame = 0;
      const rect = hero.getBoundingClientRect();
      const scrollRange = Math.max(hero.offsetHeight - window.innerHeight, 1);
      const progress = Math.min(1, Math.max(0, -rect.top / scrollRange));
      const compact = window.innerWidth <= 820;
      const brandInset = compact ? 28 : 72;
      const wordmarkSize = compact
        ? Math.max(70, Math.min(138, (window.innerWidth - 42) / 3.35))
        : Math.max(132, Math.min(390, (window.innerWidth - brandInset) / 3.05));
      const uWidth = wordmarkSize * (compact ? 0.72 : 0.76);
      const uHeight = wordmarkSize * 0.98;
      const capsuleWidth = compact ? window.innerWidth * 0.9 : Math.min(window.innerWidth * 0.62, 1400);
      const capsuleHeight = compact ? Math.min(window.innerHeight * 0.7, 600) : Math.max(340, Math.min(window.innerHeight * 0.46, 400));
      const capsuleRadius = compact ? 90 : capsuleHeight / 2;
      const finalWidth = compact ? window.innerWidth * 0.74 : Math.min(window.innerWidth * 0.32, 540);
      const finalHeight = compact ? window.innerHeight * 0.62 : window.innerHeight * 0.75;
      let width: number;
      let height: number;
      let radius: number;
      let shift: number;
      let clipPath: string;
      if (progress <= 0.24) {
        const phase = smoothstep(0, 0.24, progress);
        width = uWidth + (capsuleWidth - uWidth) * phase;
        height = uHeight + (capsuleHeight - uHeight) * phase;
        radius = capsuleRadius * phase;
        shift = 0;
        if (heroClipPathRef.current) heroClipPathRef.current.setAttribute('d', buildMorphPath(phase));
        clipPath = 'url(#heroMorphClip)';
      } else if (progress <= 0.58) {
        const phase = (progress - 0.24) / 0.34;
        const eased = 1 - Math.pow(1 - phase, 3);
        width = capsuleWidth + (window.innerWidth - capsuleWidth) * eased;
        height = capsuleHeight + (window.innerHeight - capsuleHeight) * eased;
        radius = capsuleRadius * (1 - eased);
        shift = 0;
        clipPath = `inset(0 round ${radius}px)`;
      } else {
        const phase = (progress - 0.58) / 0.42;
        const eased = phase * phase * (3 - 2 * phase);
        width = window.innerWidth - (window.innerWidth - finalWidth) * eased;
        height = window.innerHeight - (window.innerHeight - finalHeight) * eased;
        radius = 190 * eased;
        shift = (compact ? 0 : window.innerWidth * 0.28) * eased;
        clipPath = `inset(0 round ${radius}px)`;
      }
      const leftEdge = window.innerWidth / 2 + shift - width / 2;
      const rightEdge = window.innerWidth / 2 + shift + width / 2;
      const wordmarkGap = Math.max(10, Math.min(24, wordmarkSize * 0.055));
      hero.style.setProperty('--hero-width', `${width}px`);
      hero.style.setProperty('--hero-height', `${height}px`);
      hero.style.setProperty('--hero-radius', `${radius}px`);
      hero.style.setProperty('--hero-clip', clipPath);
      hero.style.setProperty('--hero-shift', `${shift}px`);
      hero.style.setProperty('--hero-left-edge', `${leftEdge}px`);
      hero.style.setProperty('--hero-right-edge', `${rightEdge}px`);
      hero.style.setProperty('--hero-word-size', `${wordmarkSize}px`);
      hero.style.setProperty('--hero-word-gap', `${wordmarkGap}px`);
      const copyOpacity = smoothstep(0.21, 0.3, progress) * (1 - smoothstep(0.43, 0.56, progress));
      hero.style.setProperty('--hero-copy-opacity', `${copyOpacity}`);
      hero.style.setProperty('--hero-word-opacity', `${1 - smoothstep(0.035, 0.16, progress)}`);
      hero.style.setProperty('--hero-intro-opacity', `${1 - smoothstep(0.27, 0.48, progress)}`);
      hero.style.setProperty('--hero-story-opacity', `${smoothstep(0.65, 0.82, progress)}`);
      hero.style.setProperty('--hero-video-scale', `${1.04 + progress * 0.12}`);
    };
    const requestUpdate = () => {
      if (!animationFrame) animationFrame = window.requestAnimationFrame(updateHero);
    };
    updateHero();
    window.addEventListener('scroll', requestUpdate, { passive: true });
    window.addEventListener('resize', requestUpdate);
    return () => {
      if (animationFrame) window.cancelAnimationFrame(animationFrame);
      window.removeEventListener('scroll', requestUpdate);
      window.removeEventListener('resize', requestUpdate);
    };
  }, []);

  useEffect(() => {
    if (codeCountdown <= 0) return undefined;
    const timer = window.setInterval(() => setCodeCountdown((value) => Math.max(0, value - 1)), 1000);
    return () => window.clearInterval(timer);
  }, [codeCountdown]);

  useEffect(() => {
    const scene = sceneRef.current;
    if (!scene) return undefined;
    let animationFrame = 0;
    let lastProgress = 0;
    let foxDirection = 1;
    const clamp01 = (value: number) => Math.min(1, Math.max(0, value));
    const smoothstep = (from: number, to: number, value: number) => {
      const amount = clamp01((value - from) / (to - from));
      return amount * amount * (3 - 2 * amount);
    };
    const updateScene = () => {
      animationFrame = 0;
      const rect = scene.getBoundingClientRect();
      const range = Math.max(scene.offsetHeight - window.innerHeight, 1);
      const progress = clamp01(-rect.top / range);
      if (Math.abs(progress - lastProgress) > .0002) foxDirection = progress > lastProgress ? 1 : -1;
      lastProgress = progress;
      const projectOut = smoothstep(.15, .22, progress);
      const creationIn = smoothstep(.15, .22, progress);
      const creationOut = smoothstep(.3, .37, progress);
      const agentsIn = smoothstep(.3, .37, progress);
      const showcaseOut = smoothstep(.48, .54, progress);
      const transition = clamp01((progress - .5) / .5);
      const diveShift = .72 * clamp01((transition - .04) / .92);
      const holeProgress = smoothstep(.08, .9, transition);
      const mobile = window.innerWidth <= 820;
      const orbStart = mobile ? .49 : .43;
      const orbEnd = mobile ? .62 : .66;
      const orbTravel = smoothstep(.05, .46, progress);
      const startRadius = Math.hypot(window.innerWidth, window.innerHeight) * 1.08;
      const endRadius = mobile ? 58 : 80;
      const holeRadius = startRadius + (endRadius - startRadius) * holeProgress;
      const holeX = window.innerWidth * (1.035 - .239 * smoothstep(.58, .98, transition));
      const textInner = scene.querySelector<HTMLElement>('.outro-text-inner');
      const textWidth = textInner?.scrollWidth || window.innerWidth * 1.9;
      const foxEnter = smoothstep(.73, .98, transition);
      const foxRunX = textWidth * (.17 + .42 * diveShift);
      const foxRunY = window.innerHeight * (.29 + Math.sin(transition * Math.PI * 2) * .018);
      const foxX = foxRunX + (holeX - foxRunX) * foxEnter;
      const foxY = foxRunY + (window.innerHeight * .5 - foxRunY) * foxEnter;
      const foxScale = (.95 + Math.sin(transition * Math.PI * 3) * .045) * (1 - .44 * foxEnter);

      scene.style.setProperty('--scene-platform-opacity', `${1 - showcaseOut}`);
      scene.style.setProperty('--scene-project-active', `${1 - projectOut}`);
      scene.style.setProperty('--scene-creation-active', `${creationIn * (1 - creationOut)}`);
      scene.style.setProperty('--scene-agents-active', `${agentsIn * (1 - showcaseOut)}`);
      scene.style.setProperty('--scene-orb-y', `${(orbStart + (orbEnd - orbStart) * orbTravel) * window.innerHeight}px`);
      scene.style.setProperty('--scene-orb-turn', `${-5 + 10 * orbTravel}deg`);
      scene.style.setProperty('--scene-outro-opacity', `${smoothstep(.48, .52, progress)}`);
      scene.style.setProperty('--scene-hole-x', `${holeX}px`);
      scene.style.setProperty('--scene-hole-r', `${holeRadius}px`);
      scene.style.setProperty('--scene-dive-shift', `${diveShift}`);
      scene.style.setProperty('--scene-text-width', `${textWidth}px`);
      scene.style.setProperty('--scene-fox-x', `${foxX}px`);
      scene.style.setProperty('--scene-fox-y', `${foxY}px`);
      scene.style.setProperty('--scene-fox-opacity', `${smoothstep(.54, .59, progress)}`);
      scene.style.setProperty('--scene-fox-scale', `${foxScale}`);
      scene.style.setProperty('--scene-fox-flip', `${foxDirection}`);
    };
    const requestUpdate = () => { if (!animationFrame) animationFrame = window.requestAnimationFrame(updateScene); };
    updateScene();
    window.addEventListener('scroll', requestUpdate, { passive: true });
    window.addEventListener('resize', requestUpdate);
    return () => {
      if (animationFrame) window.cancelAnimationFrame(animationFrame);
      window.removeEventListener('scroll', requestUpdate);
      window.removeEventListener('resize', requestUpdate);
    };
  }, []);

  const windowsArtifact = useMemo(() => downloads?.release?.artifacts.find((artifact) => artifact.platform === 'windows-x64') ?? null, [downloads]);
  const updateRegistration = (field: keyof RegistrationForm, value: string) => { setRegistration((current) => ({ ...current, [field]: value })); setFormMessage(null); };

  const sendCode = async () => {
    if (!registration.email.trim()) { setFormMessage('请先填写邮箱地址。'); return; }
    if (!registration.inviteCode.trim()) { setFormMessage('请先填写邀请码。'); return; }
    setCodeSending(true); setFormMessage(null);
    try {
      const response = await fetch('/api/site/auth/register/send-code', { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify({ email: registration.email.trim(), invite_code: registration.inviteCode.trim() }) });
      const payload = await response.json() as { error?: string; resend_after_seconds?: number };
      if (!response.ok) throw new Error(payload.error || '验证码发送失败');
      setCodeCountdown(payload.resend_after_seconds ?? 60); setFormMessage('验证码已发送，请查看邮箱。');
    } catch (error) { setFormMessage(toFriendlyRegistrationError(error)); } finally { setCodeSending(false); }
  };

  const submitRegistration = async (event: FormEvent) => {
    event.preventDefault(); setFormMessage(null);
    if (registration.password.length < 6) { setFormMessage('密码至少需要 6 个字符。'); return; }
    if (registration.password !== registration.confirmPassword) { setFormMessage('两次输入的密码不一致。'); return; }
    setRegistrationState('submitting');
    try {
      const response = await fetch('/api/site/auth/register', { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify({ email: registration.email.trim(), display_name: registration.displayName.trim() || null, password: registration.password, invite_code: registration.inviteCode.trim(), verification_code: registration.verificationCode.trim() }) });
      const payload = await response.json() as { error?: string };
      if (!response.ok) throw new Error(payload.error || '注册失败');
      setRegistrationState('success'); setRegistration(initialRegistration); setFormMessage(null);
    } catch (error) { setRegistrationState('idle'); setFormMessage(toFriendlyRegistrationError(error)); }
  };

  return (
    <main className="jiguli-home" id="top">
      <header className="jiguli-header">
        <a className="jiguli-brand" href="#top" aria-label="叽咕狸首页"><BrandMark /><span className="brand-word"><b>叽咕狸</b><small>JIGULI</small></span></a>
        <nav className="jiguli-nav" aria-label="主导航"><a href="#capability">产品原则</a><a href="#scene">使用场景</a><a href="#download">客户端下载</a><a href="#register">开始使用</a></nav>
        <div className="header-actions"><a className="header-pill dark" href={manifest.app_url}>进入应用</a><a className="header-pill" href="#register"><span>+</span> 加入测试</a></div>
      </header>

      <section className="jiguli-hero" ref={heroRef}>
        <svg className="hero-clip-defs" aria-hidden="true" focusable="false">
          <defs><clipPath id="heroMorphClip" clipPathUnits="objectBoundingBox"><path ref={heroClipPathRef} d={buildInitialHeroPath()} /></clipPath></defs>
        </svg>
        <div className="hero-sticky">
          <div className="hero-index">TOOLS FOR REAL WORK · 001</div>
          <div className="hero-wordmark" aria-hidden="true"><span>JIG</span><span className="wordmark-u">U</span><span>LI</span></div>
          <div className="hero-capsule">
            <div className="hero-video-reel" aria-label="叽咕狸工作场景视频">
              {heroVideos.map((video, index) => <video key={video.src} className={index === activeVideo ? 'active' : ''} src={video.src} poster={video.poster} autoPlay muted loop playsInline preload={index === 0 ? 'auto' : 'metadata'} />)}
            </div>
            <div className="video-wash" />
            <div className="capsule-copy"><span className="capsule-kicker"><Sparkles size={14} /> 你好，我是叽咕狸</span><h1>把工具做好，<br />把每件事<br /><span>一起做好。</span></h1><p>从项目、文件、终端到创作与协作，先认真做好每一个工具，再在你需要时稳稳接上。</p><div className="capsule-actions"><a className="j-button primary" href="#register">现在开始 <ArrowRight size={17} /></a><a className="j-button ghost" href="#capability">我们的原则</a></div></div>
            <div className="video-counter"><span>{String(activeVideo + 1).padStart(2, '0')}</span><i /> <span>{String(heroVideos.length).padStart(2, '0')}</span></div>
            <PetSprite className="hero-pet" label="抱着枫叶的叽咕狸狐狸形象" /><div className="pet-message">嗨，今天想做点什么？</div>
          </div>
          <div className="hero-scroll-story"><small>GOOD TOOLS, BETTER WORK</small><h2>先把每一个<br /><span>工具做好。</span><br />再陪你把事情做好。</h2><p>工具负责把过程变简单，把信息放清楚；<br />目标、判断和最后的决定，始终属于你。</p></div>
          <div className="hero-footerline"><p>为认真做事的人，认真做好工具</p><span>一件件打磨，一步步协助，<br />让每次使用都真正解决问题。</span><a href="#statement">SCROLL <i>↓</i></a></div>
        </div>
      </section>

      <section className="brand-statement" id="statement">
        <span className="statement-label">OUR POINT OF VIEW</span><p>真正好用的产品，不靠一句<br /><span>“更聪明”</span>证明自己。</p><p>它要先把每一个工具认真做好，尊重人的判断，<br />再陪人把每一件事情稳稳做好。</p>
        <div className="statement-sign"><PetSprite className="pet-peek" /><strong>叽咕狸</strong><small>为每一件真实的事</small></div>
      </section>

      <section className="capability-section" id="capability">
        <div className="editorial-heading"><div><span>01</span><small>OUR PRINCIPLES</small></div><h2>把每一个工具，<br />都认真做好。</h2><p>好找、好用、看得懂，也由你掌控。<br />这是协助人做好事情的起点。</p></div>
        <div className="capability-grid">{capabilities.map((item, index) => { const Icon = item.icon; return <article className="capability-card" key={item.en}><div className="card-top"><span>0{index + 1}</span><Icon size={22} /></div><PetSprite className={`card-pet ${item.pet}`} /><small>{item.en}</small><h3>{item.title}</h3><p>{item.body}</p><span className="card-arrow">↗</span></article>; })}</div>
      </section>

      <section className="scene-section" id="scene" ref={sceneRef}>
        <div className="scene-sticky">
          <div className="platform-showcase">
            <header className="platform-heading"><span>02　JIGULI DESKTOP</span><h2>Inside Jiguli</h2><p>不是把功能堆在一起，<br />而是让每一步自然接上。</p></header>
            <div className="platform-list">
              <article className="platform-item project-item"><i>✦</i><div><h3>项目工作台</h3><p>文件、对话、终端与 Git，围绕同一个项目展开。</p></div></article>
              <article className="platform-item creation-item"><i>♥</i><div><h3>AI 创作</h3><p>图片、视频与剧情分段，在一处连续完成。</p></div></article>
              <article className="platform-item agents-item"><i>●</i><div><h3>Agent 团队</h3><p>清楚分工，看见过程，也看见每一份交付。</p></div></article>
            </div>
            <div className="platform-orb" aria-label="随页面滚动轮换的叽咕狸客户端界面">
              <div className="orb-ring" aria-hidden="true" />
              <div className="orb-preview project-preview"><ClientWorkspacePreview /></div>
              <div className="orb-preview creation-preview"><CreationStudioPreview /></div>
              <div className="orb-preview agents-preview"><AgentWorkspacePreview /></div>
              <span className="orb-caption"><b>SCROLL</b><i /><em>01 — 03</em></span>
            </div>
          </div>
          <div className="platform-outro" aria-label="想要做好一件事，要学会让自己慢下来。">
            <span className="outro-blue-face" aria-hidden="true" />
            <div className="outro-text-layer outro-text-dark" aria-hidden="true"><div className="outro-text-inner"><span className="outro-line">{Array.from(transitionText).map((char, index) => <i className={index < 7 ? 'outro-char-highlight' : ''} key={`${char}-${index}`} style={{ '--char-start': index * .0125 } as CSSProperties}>{char}</i>)}</span></div></div>
            <div className="outro-text-layer outro-text-white" aria-hidden="true"><div className="outro-text-inner"><span className="outro-line">{Array.from(transitionText).map((char, index) => <i key={`${char}-${index}`} style={{ '--char-start': index * .0125 } as CSSProperties}>{char}</i>)}</span></div></div>
            <span className="outro-fox" aria-hidden="true" />
          </div>
        </div>
      </section>

      <section className="process-section">
        <div className="process-intro"><div className="editorial-heading inverse"><div><span>03</span><small>HOW IT WORKS</small></div><h2>从手边一件事，<br />一步步做好。</h2></div><PetSprite className="process-pet" /><p>把事情说清，把工具接好，把过程摊开，最后由你确认结果。</p></div>
        <div className="process-steps">{steps.map((step) => <article key={step.number}><span>{step.number}</span><h3>{step.title}</h3><p>{step.body}</p><i>↘</i></article>)}</div>
      </section>

      <section className="access-section" id="download">
        <div className="access-copy"><span className="access-label">JIGULI DESKTOP APP</span><h2>把顺手的工具，<br />装进你的电脑。</h2><p>项目、对话、文件、Git、终端、Agent 协作与 AI 创作，不是功能清单，而是一组为真实工作认真打磨的桌面工具。</p><div className="access-points"><span><Laptop size={18} /> 原生桌面体验，打开就能使用</span><span><Workflow size={18} /> 项目、消息与任务各归其位</span><span><Sparkles size={18} /> 创作与协作过程随时看得见</span></div></div>
        <div className="download-panel"><div className="download-title"><span className="platform-icon"><MonitorDown size={26} /></span><div><small>DESKTOP APP</small><strong>Windows 客户端</strong><em>Windows 10 / 11 · 64 位</em></div></div>
          {windowsArtifact ? <><a className="j-button primary wide" href={windowsArtifact.download_url}><Download size={18} /> 下载 {downloads?.release?.version}</a><div className="release-meta"><span>{formatBytes(windowsArtifact.size_bytes)}</span><span>SHA-256 {windowsArtifact.sha256.slice(0, 12)}…</span></div></> : <><button className="j-button disabled wide" type="button" disabled><Download size={18} /> {downloads?.message ?? '正在读取最新版本'}</button><div className="release-meta"><span>Windows 版本即将开放下载</span></div></>}
          <ol className="install-list"><li><span>1</span>下载并安装桌面客户端</li><li><span>2</span>登录叽咕狸账号</li><li><span>3</span>创建或打开本机项目</li></ol><p className="coming-soon">macOS 原生客户端正在持续测试中</p>
        </div>
      </section>

      <section className="join-section" id="register">
        <div className="join-copy"><span className="access-label">START WITH ONE THING</span><h2>从一件小事，<br />开始一起做好。</h2><p>当前处于邀请测试阶段。创建账号，打开桌面端，把手边第一件真实的事情交给叽咕狸一起协助。</p><PetSprite className="join-pet" /></div>
        <div className="register-panel">{registrationState === 'success' ? <div className="registration-success"><span><Check size={28} /></span><h3>账号创建成功</h3><p>现在可以打开叽咕狸，使用邮箱和密码登录。</p><a className="j-button primary" href={manifest.app_url}>打开叽咕狸 <ArrowRight size={17} /></a></div> :
          <form onSubmit={submitRegistration}><div className="form-heading"><Mail size={21} /><div><strong>创建叽咕狸账号</strong><small>邀请测试 · 邮箱验证</small></div></div>
            <label>邮箱<input type="email" value={registration.email} onChange={(event) => updateRegistration('email', event.target.value)} placeholder="you@example.com" autoComplete="email" required /></label>
            <label>昵称（选填）<input value={registration.displayName} onChange={(event) => updateRegistration('displayName', event.target.value)} placeholder="希望我们怎么称呼你" autoComplete="name" /></label>
            <label>邀请码<input value={registration.inviteCode} onChange={(event) => updateRegistration('inviteCode', event.target.value)} placeholder="输入邀请测试码" required /></label>
            <label>邮箱验证码<span className="code-field"><input inputMode="numeric" value={registration.verificationCode} onChange={(event) => updateRegistration('verificationCode', event.target.value)} placeholder="6 位验证码" required /><button type="button" onClick={() => void sendCode()} disabled={codeSending || codeCountdown > 0}>{codeSending ? '发送中' : codeCountdown > 0 ? `${codeCountdown}s` : '发送验证码'}</button></span></label>
            <div className="password-row"><label>密码<input type="password" value={registration.password} onChange={(event) => updateRegistration('password', event.target.value)} placeholder="至少 6 个字符" autoComplete="new-password" required /></label><label>确认密码<input type="password" value={registration.confirmPassword} onChange={(event) => updateRegistration('confirmPassword', event.target.value)} placeholder="再次输入密码" autoComplete="new-password" required /></label></div>
            {formMessage && <div className="form-message">{formMessage}</div>}<button className="j-button primary wide" type="submit" disabled={registrationState === 'submitting' || !manifest.registration_enabled}>{registrationState === 'submitting' ? '正在创建账号…' : '创建账号'} <ArrowRight size={17} /></button><p className="form-legal">注册即表示你同意在邀请测试期间遵守平台使用规则与隐私约定。</p>
          </form>}
        </div>
      </section>

      <section className="faq-section-new"><div className="editorial-heading"><div><span>04</span><small>FAQ</small></div><h2>开始之前，<br />你可能还想知道。</h2></div><div className="faq-list-new">{faqs.map((item, index) => <details key={item.question}><summary><span>0{index + 1}</span>{item.question}<ChevronRight size={20} /></summary><p>{item.answer}</p></details>)}</div></section>

      <footer className="jiguli-footer"><div className="footer-main"><div className="footer-name"><PetSprite className="footer-pet" /><h2>叽咕狸</h2><span>JIGULI</span></div><p>把每一个工具做好，<br />陪你把每一件事做好。</p><a className="j-button light" href="#register">开始使用 <ArrowRight size={17} /></a></div><div className="footer-bottom"><span>© 2025–2026 叽咕狸</span><nav><a href="#capability">产品原则</a><a href="#download">客户端下载</a><a href="#register">注册</a><a href={manifest.app_url}>登录</a><a href="/privacy/browser-bridge">隐私政策</a></nav><a href="#top">BACK TO TOP ↑</a></div></footer>
    </main>
  );
}

function formatBytes(value: number) { if (!Number.isFinite(value) || value <= 0) return '大小未知'; const units = ['B', 'KB', 'MB', 'GB']; let size = value; let unit = 0; while (size >= 1024 && unit < units.length - 1) { size /= 1024; unit += 1; } return `${size.toFixed(unit === 0 ? 0 : 1)} ${units[unit]}`; }

function toFriendlyRegistrationError(error: unknown) {
  const message = error instanceof Error ? error.message : String(error);
  const translations: Array<[string, string]> = [['email already registered', '这个邮箱已经注册，可以直接登录。'], ['invite code is invalid', '邀请码无效或已经失效。'], ['verification code is invalid or expired', '邮箱验证码错误或已经过期。'], ['verification code was sent recently', '验证码刚刚发送，请稍后再试。'], ['too many verification emails', '验证码发送次数过多，请稍后再试。'], ['email format is invalid', '请输入有效的邮箱地址。'], ['registration service is temporarily unavailable', '注册服务暂时不可用，请稍后再试。']];
  return translations.find(([source]) => message.toLowerCase().includes(source))?.[1] ?? message;
}

export default App;
