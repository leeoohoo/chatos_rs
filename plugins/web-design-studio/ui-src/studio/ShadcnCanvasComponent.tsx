import { useRef, useState, type ReactNode } from 'react';
import { Accordion, Checkbox, Collapsible, RadioGroup, Slider, Switch, Tabs, Toggle, ToggleGroup } from 'radix-ui';
import { Check, ChevronDown, ChevronLeft, ChevronRight, CircleAlert, MoreHorizontal, Search, X } from 'lucide-react';
import type { WebDesignComponent, WebDesignTokens } from '../../src/schema';
import { DesignOverlay, FloatingSurface, MiniCalendar, SimpleDataTable, SkeletonComposition, runtimeRecords, runtimeRows, runtimeStrings } from './LibraryRuntimePrimitives';

type AnyProps = Record<string, any>;

function cn(...values: Array<string | false | undefined>) { return values.filter(Boolean).join(' '); }

export function ShadcnCanvasComponent({ component, preview, tokens, slotContent = {} }: {
  component: WebDesignComponent;
  preview: boolean;
  tokens?: WebDesignTokens;
  slotContent?: Record<string, ReactNode>;
}) {
  return <div className={cn('shadcn-runtime', `density-${component.library?.props.density ?? 'default'}`, `tone-${component.library?.props.tone ?? 'neutral'}`)} style={{ '--shadcn-primary': tokens?.colors.primary ?? '#18181b', '--shadcn-radius': `${tokens?.radii.small ?? 8}px`, fontFamily: tokens?.typography.fontFamily } as React.CSSProperties}>
    <ShadcnCanvasRenderer component={component} preview={preview} slotContent={slotContent} />
  </div>;
}

function ShadcnButton({ variant = 'default', children, className = '', ...props }: AnyProps) {
  return <button className={cn('shadcn-button', `variant-${variant}`, className)} {...props}>{children}</button>;
}

function ShadcnCanvasRenderer({ component, preview, slotContent }: { component: WebDesignComponent; preview: boolean; slotContent: Record<string, ReactNode> }) {
  const binding = component.library!;
  const p = binding.props as AnyProps;
  const name = binding.component;
  const items = runtimeRecords(p.items);
  const triggerRef = useRef<HTMLButtonElement | null>(null);
  const hoverRef = useRef<HTMLButtonElement | null>(null);
  const contextRef = useRef<HTMLDivElement | null>(null);
  const [open, setOpen] = useState(false);
  const [page, setPage] = useState(Number(p.defaultPage ?? 2));
  const [toastOpen, setToastOpen] = useState(false);
  const [slide, setSlide] = useState(0);
  const [selectedOptions, setSelectedOptions] = useState<string[]>([]);

  if (name === 'AspectRatio') return <div className="shadcn-aspect-ratio" style={{ aspectRatio: String(p.ratio ?? 1.7778) }}>{slotContent.content ?? <div className="shadcn-media-placeholder"><span>16:9</span><small>媒体内容区域</small></div>}</div>;
  if (name === 'ButtonGroup') return <div className={cn('shadcn-button-group', p.orientation === 'vertical' && 'vertical')}>{slotContent.content ?? <><ShadcnButton variant="outline">上一页</ShadcnButton><ShadcnButton variant="outline">下一页</ShadcnButton><ShadcnButton>保存</ShadcnButton></>}</div>;
  if (name === 'Resizable') return <div className="shadcn-resizable"><div>{slotContent['panel-1'] ?? '面板一'}</div><button aria-label="调整宽度"><span /></button><div>{slotContent['panel-2'] ?? '面板二'}</div></div>;
  if (name === 'ScrollArea') return <div className="shadcn-scroll-area">{slotContent.content ?? Array.from({ length: 10 }, (_, index) => <div className="shadcn-scroll-item" key={index}><strong>组件更新 {index + 1}</strong><span>可以向下滚动查看全部内容。</span></div>)}</div>;
  if (name === 'Separator') return <div className={cn('shadcn-separator', p.orientation === 'vertical' && 'vertical')} />;
  if (name === 'Sidebar') return <div className="shadcn-sidebar">{slotContent.content ?? <><div className="shadcn-sidebar-brand"><i>W</i><strong>Workspace</strong></div><nav>{items.map((item, index) => <button className={index === 0 ? 'active' : ''} key={String(item.key)}><span>{index === 0 ? '⌂' : index === 1 ? '▦' : '⚙'}</span>{String(item.label)}</button>)}</nav><div className="shadcn-sidebar-user"><i>AI</i><span><strong>AI Designer</strong><small>designer@example.com</small></span></div></>}</div>;
  if (name === 'Direction') return <div className="shadcn-direction" dir={p.direction === 'auto' ? undefined : p.direction}><span>{String(p.locale ?? 'Auto')}</span><p>{component.content}</p></div>;

  if (name === 'Typography') {
    if (p.scale === 'lead') return <p className="shadcn-typography lead">{component.content}</p>;
    return <p className="shadcn-typography">{component.content}</p>;
  }
  if (name === 'Label') return <label className="shadcn-label">{component.content}</label>;
  if (name === 'Kbd') return <kbd className="shadcn-kbd">{component.content}</kbd>;
  if (name === 'Marker') return <mark className={cn('shadcn-marker', `kind-${p.kind ?? 'highlight'}`, `color-${p.color ?? 'yellow'}`)}>{component.content}</mark>;

  if (name === 'Button') return <ShadcnButton className="fill" variant={p.variant}>{component.content}</ShadcnButton>;
  if (name === 'Toggle') return <Toggle.Root className="shadcn-toggle" defaultPressed={Boolean(p.pressed)}>{component.content}</Toggle.Root>;
  if (name === 'ToggleGroup') return <ToggleGroup.Root className="shadcn-toggle-group" type={p.type ?? 'single'} defaultValue={p.defaultValue}>{items.map((item) => <ToggleGroup.Item key={String(item.key)} value={String(item.key)}>{String(item.label)}</ToggleGroup.Item>)}</ToggleGroup.Root>;

  if (name === 'Input') return <input className={cn('shadcn-input', p.invalid && 'invalid')} placeholder={component.content} disabled={Boolean(p.disabled)} />;
  if (name === 'InputGroup') return <div className="shadcn-input-group"><span>{String(p.prefix ?? '')}</span><input placeholder={component.content} /><span>{String(p.suffix ?? '')}</span></div>;
  if (name === 'InputOTP') return <div className="shadcn-input-otp">{Array.from({ length: Number(p.length ?? 6) }, (_, index) => <input key={index} maxLength={1} defaultValue={String(p.defaultValue ?? '')[index] ?? ''} />)}</div>;
  if (name === 'Textarea') return <textarea className="shadcn-textarea" placeholder={component.content} rows={Number(p.rows ?? 4)} />;
  if (name === 'NativeSelect') return <select className="shadcn-select-native" defaultValue={String(runtimeRecords(p.options)[0]?.value ?? '')}>{runtimeRecords(p.options).map((option) => <option key={String(option.value)} value={String(option.value)}>{String(option.label)}</option>)}</select>;
  if (name === 'Select' || name === 'Combobox') return <><button ref={triggerRef} className="shadcn-select-trigger" onClick={() => preview && setOpen(!open)}><span>{component.content}</span><ChevronDown size={15} /></button><FloatingSurface anchorRef={triggerRef} open={open} className="shadcn-select-content">{name === 'Combobox' && <label><Search size={14} /><input placeholder="搜索…" /></label>}{runtimeRecords(p.options).map((option, index) => <button key={String(option.value)} onClick={() => setOpen(false)}><Check size={14} className={index === 0 ? '' : 'hidden'} />{String(option.label)}</button>)}</FloatingSurface></>;
  if (name === 'Checkbox') return <label className="shadcn-checkbox-label"><Checkbox.Root className="shadcn-checkbox" defaultChecked={Boolean(p.defaultChecked)}><Checkbox.Indicator><Check size={13} strokeWidth={3} /></Checkbox.Indicator></Checkbox.Root><span>{component.content}</span></label>;
  if (name === 'Switch') return <label className="shadcn-switch-label"><Switch.Root className="shadcn-switch" defaultChecked={Boolean(p.defaultChecked)}><Switch.Thumb /></Switch.Root><span>{component.content}</span></label>;
  if (name === 'RadioGroup') return <RadioGroup.Root className="shadcn-radio-group" defaultValue={String(p.defaultValue ?? '')}>{runtimeRecords(p.options).map((option) => <label key={String(option.value)}><RadioGroup.Item value={String(option.value)}><RadioGroup.Indicator /></RadioGroup.Item><span>{String(option.label)}</span></label>)}</RadioGroup.Root>;
  if (name === 'Slider') return <Slider.Root className="shadcn-slider" defaultValue={Array.isArray(p.defaultValue) ? p.defaultValue.map(Number) : [Number(p.defaultValue ?? 50)]} min={Number(p.min ?? 0)} max={Number(p.max ?? 100)} step={Number(p.step ?? 1)}><Slider.Track><Slider.Range /></Slider.Track><Slider.Thumb /></Slider.Root>;
  if (name === 'Calendar') return <MiniCalendar selectedDay={Number(p.selectedDay ?? 3)} className="shadcn-calendar" />;
  if (name === 'DatePicker') return <><button ref={triggerRef} className="shadcn-date-picker" onClick={() => preview && setOpen(!open)}><span>▣</span>{String(p.placeholder ?? component.content ?? '选择日期')}<ChevronDown size={15} /></button><FloatingSurface anchorRef={triggerRef} open={open} className="shadcn-date-picker-popover"><MiniCalendar selectedDay={Number(p.selectedDay ?? 3)} className="shadcn-calendar" />{p.mode === 'range' && <small>结束日期：{String(p.endDay ?? 9)} 日</small>}</FloatingSurface></>;
  if (name === 'Field') return <fieldset className="shadcn-field"><label>{String(p.label ?? '字段')}</label>{slotContent.content ?? <input className={cn('shadcn-input', p.error && 'invalid')} placeholder="请输入内容" />}<small className={p.error ? 'error' : ''}>{String(p.error || p.description || '')}</small></fieldset>;
  if (name === 'Questionnaire') {
    const options = runtimeStrings(p.options);
    return <section className="shadcn-questionnaire"><strong>{String(p.question ?? '请选择')}</strong><div>{options.map((option) => {
      const active = selectedOptions.includes(option);
      return <button className={active ? 'active' : ''} key={option} onClick={() => {
        if (!preview) return;
        setSelectedOptions((current) => p.type === 'multiple' ? current.includes(option) ? current.filter((item) => item !== option) : [...current, option] : [option]);
      }}>{p.type === 'rating' ? '★' : active ? '✓' : '○'}<span>{option}</span></button>;
    })}</div></section>;
  }

  if (name === 'Accordion') {
    const type = p.type === 'multiple' ? 'multiple' : 'single';
    return type === 'multiple'
      ? <Accordion.Root className="shadcn-accordion" type="multiple" defaultValue={Array.isArray(p.defaultValue) ? p.defaultValue.map(String) : []}>{items.map((item) => <ShadcnAccordionItem key={String(item.key)} item={item} slotContent={slotContent} />)}</Accordion.Root>
      : <Accordion.Root className="shadcn-accordion" type="single" collapsible={p.collapsible !== false} defaultValue={String(p.defaultValue ?? '')}>{items.map((item) => <ShadcnAccordionItem key={String(item.key)} item={item} slotContent={slotContent} />)}</Accordion.Root>;
  }
  if (name === 'Breadcrumb') return <nav className="shadcn-breadcrumb">{items.map((item, index) => <span key={String(item.key)}><a>{String(item.label)}</a>{index < items.length - 1 && <ChevronRight size={14} />}</span>)}</nav>;
  if (name === 'Collapsible') return <Collapsible.Root className="shadcn-collapsible" defaultOpen={Boolean(p.defaultOpen)}><Collapsible.Trigger><span>{component.content}</span><ChevronDown size={15} /></Collapsible.Trigger><Collapsible.Content>{slotContent.content ?? <p>这是可以继续设计的折叠内容区域。</p>}</Collapsible.Content></Collapsible.Root>;
  if (name === 'Command') return <div className="shadcn-command"><label><Search size={15} /><input placeholder={String(p.placeholder ?? '输入命令或搜索…')} /></label>{runtimeRecords(p.groups).map((group, index) => <section key={`${String(group.label)}-${index}`}><small>{String(group.label)}</small>{runtimeStrings(group.items).map((entry, itemIndex) => <button key={entry}><span>{itemIndex === 0 ? '⌘' : itemIndex === 1 ? '↗' : '•'}</span>{entry}<kbd>{itemIndex + 1}</kbd></button>)}</section>)}</div>;
  if (name === 'Menubar') return <div className="shadcn-menubar">{runtimeRecords(p.menus).map((menu) => <button key={String(menu.key)}>{String(menu.label)}</button>)}</div>;
  if (name === 'NavigationMenu') return <nav className="shadcn-navigation-menu">{items.map((item) => <button key={String(item.key)}>{String(item.label)}<ChevronDown size={13} /></button>)}</nav>;
  if (name === 'Pagination') return <nav className="shadcn-pagination"><button onClick={() => preview && setPage(Math.max(1, page - 1))}><ChevronLeft size={15} />上一页</button>{Array.from({ length: Math.min(5, Number(p.totalPages ?? 8)) }, (_, index) => index + 1).map((value) => <button className={page === value ? 'active' : ''} onClick={() => preview && setPage(value)} key={value}>{value}</button>)}<button onClick={() => preview && setPage(Math.min(Number(p.totalPages ?? 8), page + 1))}>下一页<ChevronRight size={15} /></button></nav>;
  if (name === 'Tabs') return <Tabs.Root className={cn('shadcn-tabs', p.orientation === 'vertical' && 'vertical')} orientation={p.orientation ?? 'horizontal'} defaultValue={String(p.defaultValue ?? items[0]?.key ?? '')}><Tabs.List>{items.map((item) => <Tabs.Trigger key={String(item.key)} value={String(item.key)}>{String(item.label)}</Tabs.Trigger>)}</Tabs.List>{items.map((item) => <Tabs.Content key={String(item.key)} value={String(item.key)}>{slotContent[`tab-${String(item.key)}`] ?? <p>{String(item.label)}设置内容。</p>}</Tabs.Content>)}</Tabs.Root>;

  if (name === 'Avatar') return <div className="shadcn-avatar">{p.src ? <img src={String(p.src)} alt="" /> : <span>{String(p.fallback ?? component.content)}</span>}</div>;
  if (name === 'Attachment') return <div className={cn('shadcn-attachment', `status-${p.status ?? 'uploading'}`)}><span>⌁</span><div><strong>{String(p.fileName ?? 'attachment.fig')}</strong><small>{String(p.fileSize ?? '')} · {p.status === 'complete' ? '已完成' : p.status === 'error' ? '上传失败' : `${Number(p.progress ?? 0)}%`}</small><i><b style={{ width: `${Math.min(100, Math.max(0, Number(p.progress ?? 0)))}%` }} /></i></div><button>×</button></div>;
  if (name === 'Badge') return <span className={cn('shadcn-badge', `variant-${p.variant ?? 'default'}`)}>{component.content}</span>;
  if (name === 'Bubble') return <div className={cn('shadcn-bubble', p.role === 'user' && 'user', p.thinking && 'thinking')}><i>{String(p.avatar ?? 'AI')}</i><div><p>{component.content}</p><small>{String(p.timestamp ?? '')}</small></div></div>;
  if (name === 'Card') return <article className={cn('shadcn-card', p.featured && 'featured', p.density === 'compact' && 'compact')}><header><h3>{String(p.title ?? '卡片')}</h3><p>{String(p.description ?? '')}</p></header><div className="shadcn-card-content">{slotContent.content ?? <p>{component.content}</p>}</div></article>;
  if (name === 'Carousel') {
    const labels = runtimeStrings(p.labels);
    const activeLabel = labels[slide] ?? `轮播项 ${slide + 1}`;
    return <div className="shadcn-carousel"><div className="shadcn-carousel-track">{slotContent[`slide-${slide + 1}`] ?? <><span>{String(slide + 1).padStart(2, '0')}</span><strong>{activeLabel}</strong><small>使用左右按钮切换内容</small></>}</div><button aria-label="上一张" onClick={() => preview && setSlide((current) => current <= 0 ? (p.loop ? Math.max(0, labels.length - 1) : 0) : current - 1)}><ChevronLeft size={16} /></button><button aria-label="下一张" onClick={() => preview && setSlide((current) => current >= labels.length - 1 ? (p.loop ? 0 : current) : current + 1)}><ChevronRight size={16} /></button><nav>{labels.map((label, index) => <i className={index === slide ? 'active' : ''} key={label} />)}</nav></div>;
  }
  if (name === 'Chart') return <div className="shadcn-chart"><div className="shadcn-chart-heading"><span>访问趋势</span><strong>+18.2%</strong></div><div className="shadcn-chart-bars">{runtimeStrings(p.labels).map((label, index) => <div key={label}><i style={{ height: `${Math.max(12, Number((p.values as any[])?.[index] ?? 0))}%` }} /><small>{label}</small></div>)}</div></div>;
  if (name === 'DataTable' || name === 'Table') return <SimpleDataTable className="shadcn-data-table" columns={runtimeStrings(p.columns)} rows={runtimeRows(p.rows)} striped={Boolean(p.striped)} />;
  if (name === 'Empty') return <div className="shadcn-empty"><span>◇</span><h3>{String(p.title)}</h3><p>{String(p.description)}</p><ShadcnButton>{String(p.actionLabel)}</ShadcnButton></div>;
  if (name === 'Item') return <div className="shadcn-item"><i>{String(p.icon ?? '◈')}</i><div><strong>{String(p.title ?? '内容条目')}</strong><span>{String(p.description ?? '')}</span></div>{p.action && <button>{String(p.action)}</button>}</div>;
  if (name === 'Message') return <div className={cn('shadcn-message', `role-${p.role ?? 'assistant'}`)}><i>{String(p.avatar ?? 'AI')}</i><div><header><strong>{String(p.sender ?? 'AI')}</strong><small>{String(p.time ?? '')}</small></header><p>{component.content}</p></div></div>;
  if (name === 'MessageScroller') return <div className="shadcn-message-scroller">{Array.from({ length: Number(p.messageCount ?? 6) }, (_, index) => <div className={index % 3 === 2 ? 'user' : ''} key={index}><i>{index % 3 === 2 ? '你' : 'AI'}</i><p>{p.kind === 'activity' ? `设计活动 ${index + 1} 已记录` : index % 3 === 2 ? '继续调整这个区域的视觉层级。' : '已完成组件分析并生成修改建议。'}</p></div>)}</div>;

  if (name === 'Alert') return <div className={cn('shadcn-alert', p.variant === 'destructive' && 'destructive')}><CircleAlert size={18} /><div><h4>{String(p.title ?? '提示')}</h4><p>{component.content}</p></div></div>;
  if (name === 'Progress') return <div className="shadcn-progress"><i style={{ width: `${Math.min(100, Math.max(0, Number(p.value ?? 0)))}%` }} /></div>;
  if (name === 'Skeleton') return <SkeletonComposition kind={String(p.kind ?? 'text')} lines={Number(p.lines ?? 3)} className="shadcn-skeleton-composition" />;
  if (name === 'Spinner') return <div className="shadcn-spinner" style={{ width: Number(p.size ?? 28), height: Number(p.size ?? 28) }} />;
  if (name === 'Toast') return <><ShadcnButton ref={triggerRef} className="fill" onClick={() => { if (!preview) return; setToastOpen(true); window.setTimeout(() => setToastOpen(false), 2500); }}>{component.content}</ShadcnButton><FloatingSurface anchorRef={triggerRef} open={toastOpen} className="shadcn-toast"><Check size={17} /><div><strong>{String(p.title)}</strong><span>{String(p.description)}</span></div><button onClick={() => setToastOpen(false)}><X size={14} /></button></FloatingSurface></>;

  if (name === 'Dialog' || name === 'AlertDialog' || name === 'Drawer' || name === 'Sheet') {
    const side = name === 'Dialog' || name === 'AlertDialog' ? 'center' : String(p.side ?? 'right');
    return <><ShadcnButton ref={triggerRef} className="fill" variant={name === 'AlertDialog' ? 'destructive' : 'default'} onClick={() => preview && setOpen(true)}>{component.content}</ShadcnButton><DesignOverlay anchorRef={triggerRef} open={open} side={side} title={String(p.title ?? name)} className="shadcn-overlay" onClose={() => setOpen(false)} footer={<><ShadcnButton variant="outline" onClick={() => setOpen(false)}>取消</ShadcnButton><ShadcnButton variant={name === 'AlertDialog' ? 'destructive' : 'default'} onClick={() => setOpen(false)}>{name === 'AlertDialog' ? '确认删除' : '保存修改'}</ShadcnButton></>}>{slotContent.content ?? <div className="shadcn-overlay-default"><p>{String(p.description ?? '这是可交互的 shadcn/ui 浮层，可以继续放入表单或展示组件。')}</p><input className="shadcn-input" placeholder="输入内容" /></div>}</DesignOverlay></>;
  }
  if (name === 'DropdownMenu') return <><ShadcnButton ref={triggerRef} className="fill" variant="outline" onClick={() => preview && setOpen(!open)}>{component.content}<ChevronDown size={15} /></ShadcnButton><FloatingSurface anchorRef={triggerRef} open={open} className="shadcn-dropdown">{items.map((item) => <button key={String(item.key)} onClick={() => setOpen(false)}>{String(item.label)}{item.key === 'profile' && <span>⇧⌘P</span>}</button>)}</FloatingSurface></>;
  if (name === 'ContextMenu') return <><div ref={contextRef} className="shadcn-context-target" onContextMenu={(event) => { event.preventDefault(); if (preview) setOpen(true); }}>{component.content}<MoreHorizontal size={18} /></div><FloatingSurface anchorRef={contextRef} open={open} className="shadcn-dropdown">{items.map((item) => <button key={String(item.key)} onClick={() => setOpen(false)}>{String(item.label)}</button>)}</FloatingSurface></>;
  if (name === 'HoverCard' || name === 'Tooltip') return <><ShadcnButton ref={hoverRef} className="fill" variant="outline" onMouseEnter={() => preview && setOpen(true)} onMouseLeave={() => setOpen(false)}>{component.content}</ShadcnButton><FloatingSurface anchorRef={hoverRef} open={open} className={name === 'Tooltip' ? 'shadcn-tooltip' : 'shadcn-hover-card'}>{name === 'Tooltip' ? String(p.content) : <><strong>{String(p.title)}</strong><p>{String(p.description)}</p></>}</FloatingSurface></>;
  if (name === 'Popover') return <><ShadcnButton ref={triggerRef} className="fill" variant="outline" onClick={() => preview && setOpen(!open)}>{component.content}</ShadcnButton><FloatingSurface anchorRef={triggerRef} open={open} className="shadcn-popover"><h4>{String(p.title)}</h4>{slotContent.popup ?? <div className="shadcn-popover-form"><label>宽度<input defaultValue="100%" /></label><label>高度<input defaultValue="自动" /></label></div>}</FloatingSurface></>;

  return <div className="shadcn-fallback"><span>shadcn/ui</span><strong>{name}</strong></div>;
}

function ShadcnAccordionItem({ item, slotContent }: { item: Record<string, any>; slotContent: Record<string, ReactNode> }) {
  return <Accordion.Item value={String(item.key)}><Accordion.Header><Accordion.Trigger>{String(item.label)}<ChevronDown size={16} /></Accordion.Trigger></Accordion.Header><Accordion.Content>{slotContent[`panel-${String(item.key)}`] ?? <p>{String(item.label)}的详细内容。</p>}</Accordion.Content></Accordion.Item>;
}
