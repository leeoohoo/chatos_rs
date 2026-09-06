import { useEffect, useRef, useState, type ReactNode } from 'react';
import { Accordion, Checkbox, Collapsible, RadioGroup, Slider, Switch, Tabs, Toggle, ToggleGroup } from 'radix-ui';
import { Check, ChevronDown, ChevronLeft, ChevronRight, CircleAlert, MoreHorizontal, Search, X } from 'lucide-react';
import type { WebDesignComponent, WebDesignTokens } from '../../src/schema';
import { DesignOverlay, FloatingSurface, MiniCalendar, SimpleDataTable, SkeletonComposition, runtimeRecords, runtimeRows, runtimeStrings } from './LibraryRuntimePrimitives';
import { designStyleScopeProps } from './component-style';

type AnyProps = Record<string, any>;

function cn(...values: Array<string | false | undefined>) { return values.filter(Boolean).join(' '); }

export function ShadcnCanvasComponent({ component, preview, showcase = false, tokens, slotContent = {} }: {
  component: WebDesignComponent;
  preview: boolean;
  showcase?: boolean;
  tokens?: WebDesignTokens;
  slotContent?: Record<string, ReactNode>;
}) {
  const scope = designStyleScopeProps(component.style);
  return <div className={cn('shadcn-runtime', scope.className, `density-${component.library?.props.density ?? 'default'}`, `tone-${component.library?.props.tone ?? 'neutral'}`)} style={{ ...scope.style, '--shadcn-primary': tokens?.colors.primary ?? '#18181b', '--shadcn-radius': `${tokens?.radii.small ?? 8}px`, fontFamily: tokens?.typography.fontFamily } as React.CSSProperties}>
    <ShadcnCanvasRenderer component={component} preview={preview} showcase={showcase} slotContent={slotContent} />
  </div>;
}

function ShadcnButton({ variant = 'default', children, className = '', ...props }: AnyProps) {
  return <button className={cn('shadcn-button', `variant-${variant}`, className)} {...props}>{children}</button>;
}

function ShadcnToastBody({ props, onClose }: { props: AnyProps; onClose?: () => void }) {
  const icon = props.kind === 'error' ? '!' : props.kind === 'progress' ? '···' : '✓';
  return <div className={cn('shadcn-toast', `kind-${props.kind ?? 'success'}`)}><i>{icon}</i><div><strong>{String(props.title)}</strong><span>{String(props.description)}</span>{props.kind === 'progress' && <b><em /></b>}</div>{props.action && <button>{String(props.action)}</button>}{onClose && <button aria-label="关闭通知" onClick={onClose}><X size={14} /></button>}</div>;
}

function ShadcnHoverBody(props: AnyProps) {
  if (props.kind === 'project') return <div className="shadcn-hover-project"><header><i>W</i><span><strong>{String(props.title)}</strong><small>{String(props.description)}</small></span></header><div><b>72%</b><span>设计进度</span></div><footer><span>林</span><span>陈</span><span>AI</span><small>3 位协作者</small></footer></div>;
  if (props.kind === 'status') return <div className="shadcn-hover-status"><i><Check size={15} /></i><span><strong>{String(props.title)}</strong><small>{String(props.description)}</small></span></div>;
  return <div className="shadcn-hover-profile"><header><i>林</i><span><strong>{String(props.title)}</strong><small>{String(props.description)}</small></span></header><div><span>Product Design</span><span>Design System</span></div><footer>本周完成 18 项设计更新</footer></div>;
}

function ShadcnPopoverBody(props: AnyProps) {
  if (props.kind === 'form') return <div className="shadcn-popover-form"><label>邮箱<input defaultValue="designer@example.com" /></label><label>权限<select defaultValue="edit"><option value="edit">可编辑</option><option value="view">仅查看</option></select></label><ShadcnButton>发送邀请</ShadcnButton></div>;
  if (props.kind === 'command') return <div className="shadcn-popover-command">{[['复制组件', '⌘D'], ['创建组合', '⌘G'], ['锁定位置', '⇧⌘L']].map(([label, key]) => <button key={label}><span>{label}</span><kbd>{key}</kbd></button>)}</div>;
  return <div className="shadcn-popover-form"><label>宽度<input defaultValue="1200" /></label><label>高度<input defaultValue="Auto" /></label><label>约束<select defaultValue="stretch"><option value="stretch">左右拉伸</option><option value="center">居中</option></select></label></div>;
}

function ShadcnTooltipBody(props: AnyProps) {
  if (props.kind === 'rich') return <div className="shadcn-tooltip-rich"><strong>{String(props.title)}</strong><span>{String(props.content)}</span></div>;
  if (props.kind === 'shortcut') return <span className="shadcn-tooltip-shortcut">{String(props.content)}<kbd>{String(props.shortcut)}</kbd></span>;
  return String(props.content);
}

function ShadcnAlertDialogBody(props: AnyProps) {
  const icon = props.kind === 'publish' ? '↗' : props.kind === 'discard' ? '↶' : '!';
  return <div className={cn('shadcn-alert-dialog-body', `kind-${props.kind ?? 'delete'}`)}><i>{icon}</i><div><strong>{String(props.title)}</strong><p>{String(props.description)}</p></div></div>;
}

function ShadcnCanvasRenderer({ component, preview, showcase, slotContent }: { component: WebDesignComponent; preview: boolean; showcase: boolean; slotContent: Record<string, ReactNode> }) {
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
  const [checkedMenuItems, setCheckedMenuItems] = useState<string[]>(() => items.filter((item) => item.checked).map((item) => String(item.key)));
  const [selectedSelectValues, setSelectedSelectValues] = useState<string[]>(() => runtimeStrings(p.defaultValue));
  const [comboboxQuery, setComboboxQuery] = useState('');
  const [selectedTableRows, setSelectedTableRows] = useState<number[]>([0]);
  useEffect(() => {
    if (showcase && ['Select', 'Combobox', 'DatePicker', 'Dialog', 'AlertDialog', 'Drawer', 'Sheet', 'DropdownMenu', 'ContextMenu', 'HoverCard', 'Popover', 'Tooltip'].includes(name)) setOpen(true);
  }, [showcase, name]);
  useEffect(() => {
    if (showcase && name === 'Toast') setToastOpen(true);
  }, [showcase, name]);
  useEffect(() => {
    setSelectedSelectValues(runtimeStrings(p.defaultValue));
    setComboboxQuery('');
    setCheckedMenuItems(items.filter((item) => item.checked).map((item) => String(item.key)));
  }, [component.id, binding.variant]);

  if (name === 'AspectRatio') return <div className={cn('shadcn-aspect-ratio', `kind-${p.kind ?? 'video'}`)} style={{ aspectRatio: String(p.ratio ?? 1.7778) }}>{slotContent.content ?? <div className="shadcn-media-placeholder"><span>{String(p.label ?? '16:9')}</span><strong>{String(p.title ?? '媒体内容区域')}</strong><small>{p.kind === 'video' ? '▶ 点击播放' : p.kind === 'portrait' ? '移动端素材' : '方形视觉资源'}</small></div>}</div>;
  if (name === 'ButtonGroup') return <div className={cn('shadcn-button-group', p.orientation === 'vertical' && 'vertical')}>{slotContent.content ?? (p.kind === 'split' ? <><ShadcnButton>发布设计</ShadcnButton><ShadcnButton aria-label="更多发布选项">⌄</ShadcnButton></> : p.kind === 'tools' ? <><ShadcnButton variant="outline">选择</ShadcnButton><ShadcnButton variant="outline">画框</ShadcnButton><ShadcnButton variant="outline">文字</ShadcnButton></> : <><ShadcnButton variant="outline">上一页</ShadcnButton><ShadcnButton variant="outline">1 / 8</ShadcnButton><ShadcnButton variant="outline">下一页</ShadcnButton></>)}</div>;
  if (name === 'Resizable') return <div className={cn('shadcn-resizable', p.direction === 'vertical' && 'vertical')}><div>{slotContent['panel-1'] ?? (p.panelKind === 'layers' ? <><strong>页面图层</strong><p>导航栏<br />主视觉<br />功能卡片</p></> : p.panelKind === 'code' ? <><strong>JSX</strong><pre>{'<Hero title="Design" />'}</pre></> : <><strong>页面预览</strong><p>1200 × 900</p></>)}</div><button aria-label={p.direction === 'vertical' ? '调整高度' : '调整宽度'}><span /></button><div>{slotContent['panel-2'] ?? (p.panelKind === 'layers' ? <><strong>属性面板</strong><p>宽度 1200<br />高度 Auto</p></> : p.panelKind === 'code' ? <><strong>实时预览</strong><p>Design with AI</p></> : <><strong>交互时间线</strong><p>滚动 · 悬浮 · 点击</p></>)}</div></div>;
  if (name === 'ScrollArea') return <div className={cn('shadcn-scroll-area', p.kind === 'horizontal' && 'horizontal')}>{slotContent.content ?? Array.from({ length: Number(p.itemCount ?? 10) }, (_, index) => p.kind === 'horizontal' ? <div className="shadcn-scroll-card" key={index}><i>{String(index + 1).padStart(2, '0')}</i><strong>{['Hero', 'Logo Wall', 'Features', 'Gallery', 'Pricing', 'Footer'][index % 6]}</strong></div> : <div className="shadcn-scroll-item" key={index}><strong>{p.kind === 'notifications' ? ['新批注', '设计已保存', '成员加入'][index % 3] : `组件更新 ${index + 1}`}</strong><span>{p.kind === 'notifications' ? `${index + 2} 分钟前 · 点击查看详情` : '可以向下滚动查看全部内容。'}</span></div>)}</div>;
  if (name === 'Separator') return p.label ? <div className="shadcn-separator-labeled"><div className="shadcn-separator" /><span>{String(p.label)}</span><div className="shadcn-separator" /></div> : <div className={cn('shadcn-separator', p.orientation === 'vertical' && 'vertical')} />;
  if (name === 'Sidebar') return <div className={cn('shadcn-sidebar', `kind-${p.kind ?? 'workspace'}`)}>{slotContent.content ?? <><div className="shadcn-sidebar-brand"><i>W</i>{p.kind !== 'rail' && <strong>{p.kind === 'settings' ? '设置中心' : 'Workspace'}</strong>}</div><nav>{items.map((item, index) => <div key={String(item.key)}>{p.kind === 'settings' && (index === 0 || items[index - 1]?.group !== item.group) && <small>{String(item.group)}</small>}<button title={String(item.label)} className={index === 0 ? 'active' : ''}><span>{['⌂', '▦', '◇', '⚙'][index % 4]}</span>{p.kind !== 'rail' && String(item.label)}</button></div>)}</nav>{p.kind !== 'rail' && <div className="shadcn-sidebar-user"><i>AI</i><span><strong>AI Designer</strong><small>designer@example.com</small></span></div>}</>}</div>;
  if (name === 'Direction') return <div className="shadcn-direction" dir={p.direction === 'auto' ? undefined : p.direction}><span>{String(p.locale ?? 'Auto')}</span><p>{component.content}</p></div>;

  if (name === 'Typography') {
    if (p.scale === 'lead') return <p className="shadcn-typography lead">{component.content}</p>;
    if (p.scale === 'heading') return <h2 className="shadcn-typography heading">{component.content}</h2>;
    if (p.scale === 'quote') return <blockquote className="shadcn-typography quote">{component.content}<footer>— Design Systems Team</footer></blockquote>;
    return <p className="shadcn-typography">{component.content}</p>;
  }
  if (name === 'Label') return <label className="shadcn-label">{component.content}{p.required && <b>*</b>}{p.helper && <small>{String(p.helper)}</small>}</label>;
  if (name === 'Kbd') return <span className="shadcn-kbd-group">{runtimeStrings(p.keys).map((key, index) => <span key={`${key}-${index}`}><kbd className="shadcn-kbd">{key}</kbd>{index < runtimeStrings(p.keys).length - 1 && <i>{binding.variant === 'sequence' ? 'then' : '+'}</i>}</span>)}</span>;
  if (name === 'Marker') return <mark className={cn('shadcn-marker', `kind-${p.kind ?? 'highlight'}`, `color-${p.color ?? 'yellow'}`)}>{component.content}</mark>;

  if (name === 'Button') return <ShadcnButton className="fill" variant={p.variant}>{component.content}</ShadcnButton>;
  if (name === 'Toggle') return <Toggle.Root className="shadcn-toggle" defaultPressed={Boolean(p.pressed)} disabled={Boolean(p.disabled)}><strong>{String(p.icon ?? '')}</strong><span>{component.content}</span></Toggle.Root>;
  if (name === 'ToggleGroup') return <ToggleGroup.Root className={cn('shadcn-toggle-group', p.orientation === 'vertical' && 'vertical')} type={p.type ?? 'single'} defaultValue={p.defaultValue}>{items.map((item) => <ToggleGroup.Item key={String(item.key)} value={String(item.key)}>{String(item.label)}</ToggleGroup.Item>)}</ToggleGroup.Root>;

  if (name === 'Input') return <input className={cn('shadcn-input', p.invalid && 'invalid')} placeholder={component.content} disabled={Boolean(p.disabled)} />;
  if (name === 'InputGroup') return <div className={cn('shadcn-input-group', `kind-${p.kind ?? 'email'}`)}><span>{String(p.prefix ?? '')}</span><input defaultValue={String(p.defaultValue ?? '')} placeholder={component.content} /><span>{String(p.suffix ?? '')}</span></div>;
  if (name === 'InputOTP') return <div className="shadcn-input-otp">{Array.from({ length: Number(p.length ?? 6) }, (_, index) => <input className={Number(p.groupAt) === index ? 'group-start' : ''} key={index} maxLength={1} defaultValue={String(p.defaultValue ?? '')[index] ?? ''} />)}</div>;
  if (name === 'Textarea') return <div className={cn('shadcn-textarea-field', p.invalid && 'invalid')}><textarea className="shadcn-textarea" defaultValue={String(p.defaultValue ?? '')} placeholder={component.content} rows={Number(p.rows ?? 4)} maxLength={p.maxLength ? Number(p.maxLength) : undefined} disabled={Boolean(p.disabled)} />{p.showCount && <small>{String(p.defaultValue ?? '').length} / {Number(p.maxLength ?? 200)}</small>}{p.errorText && <small className="error">{String(p.errorText)}</small>}</div>;
  if (name === 'NativeSelect') return <select className="shadcn-select-native" defaultValue={String(p.defaultValue ?? runtimeRecords(p.options)[0]?.value ?? '')} disabled={Boolean(p.disabled)}>{runtimeRecords(p.options).map((option) => <option key={String(option.value)} value={String(option.value)}>{String(option.label)}</option>)}</select>;
  if (name === 'Select' || name === 'Combobox') {
    const options = runtimeRecords(p.options);
    const visibleOptions = name === 'Combobox' && comboboxQuery ? options.filter((option) => String(option.label).toLowerCase().includes(comboboxQuery.toLowerCase())) : options;
    const selectedLabels = selectedSelectValues.map((value) => String(options.find((option) => String(option.value) === value)?.label ?? value));
    const menu = <>{name === 'Combobox' && <label><Search size={14} /><input value={comboboxQuery} onChange={(event) => setComboboxQuery(event.target.value)} placeholder={p.kind === 'people' ? '搜索姓名…' : '搜索…'} /></label>}{visibleOptions.length ? visibleOptions.map((option) => { const value = String(option.value); const active = selectedSelectValues.includes(value); return <button className={active ? 'active' : ''} key={value} onClick={() => { if (p.multiple) setSelectedSelectValues((current) => current.includes(value) ? current.filter((item) => item !== value) : [...current, value]); else { setSelectedSelectValues([value]); setOpen(false); } }}><Check size={14} className={active ? '' : 'hidden'} />{String(option.label)}</button>; }) : <p className="shadcn-select-empty">没有匹配结果</p>}</>;
    return <div className={cn('shadcn-select-shell', p.multiple && 'multiple', p.disabled && 'disabled')}><button ref={triggerRef} className="shadcn-select-trigger" disabled={Boolean(p.disabled)} onClick={() => preview && !p.disabled && setOpen(!open)}><span>{selectedLabels.length ? p.multiple ? selectedLabels.map((label) => <i key={label}>{label}</i>) : selectedLabels[0] : component.content}</span><ChevronDown size={15} /></button>{showcase && !p.disabled ? <div className="shadcn-select-content inline">{menu}</div> : <FloatingSurface anchorRef={triggerRef} open={p.disabled ? false : open} className="shadcn-select-content">{menu}</FloatingSurface>}</div>;
  }
  if (name === 'Checkbox') return <label className={cn('shadcn-checkbox-label', p.disabled && 'disabled')}><Checkbox.Root className="shadcn-checkbox" defaultChecked={Boolean(p.defaultChecked)} disabled={Boolean(p.disabled)}><Checkbox.Indicator><Check size={13} strokeWidth={3} /></Checkbox.Indicator></Checkbox.Root><span>{component.content}</span></label>;
  if (name === 'Switch') return <label className={cn('shadcn-switch-label', p.disabled && 'disabled')}><Switch.Root className="shadcn-switch" defaultChecked={Boolean(p.defaultChecked)} disabled={Boolean(p.disabled)}><Switch.Thumb /></Switch.Root><span>{component.content}</span></label>;
  if (name === 'RadioGroup') return <RadioGroup.Root className={cn('shadcn-radio-group', p.orientation === 'vertical' && 'vertical', p.kind === 'cards' && 'cards')} defaultValue={String(p.defaultValue ?? '')}>{runtimeRecords(p.options).map((option) => <label key={String(option.value)}><RadioGroup.Item value={String(option.value)}><RadioGroup.Indicator /></RadioGroup.Item><span>{String(option.label)}</span>{p.kind === 'cards' && <small>{option.value === 'pro' ? '推荐' : option.value === 'team' ? '多人协作' : '基础能力'}</small>}</label>)}</RadioGroup.Root>;
  if (name === 'Slider') {
    const values = Array.isArray(p.defaultValue) ? p.defaultValue.map(Number) : [Number(p.defaultValue ?? 50)];
    return <div className={cn('shadcn-slider-shell', `kind-${p.kind ?? 'value'}`)}><div><span>{p.kind === 'range' ? `${values[0]}% – ${values[1]}%` : p.kind === 'steps' ? `等级 ${values[0]}` : `${values[0]}%`}</span>{p.kind === 'range' && <small>响应式宽度范围</small>}</div><Slider.Root className="shadcn-slider" defaultValue={values} min={Number(p.min ?? 0)} max={Number(p.max ?? 100)} step={Number(p.step ?? 1)}><Slider.Track><Slider.Range /></Slider.Track>{values.map((_, index) => <Slider.Thumb key={index} />)}</Slider.Root>{p.kind === 'steps' && <nav>{runtimeStrings(p.marks).map((mark) => <small key={mark}>{mark}</small>)}</nav>}</div>;
  }
  if (name === 'Calendar') return <div className={cn('shadcn-calendar-shell', `kind-${p.kind ?? 'single'}`)}><MiniCalendar selectedDay={Number(p.selectedDay ?? 3)} className="shadcn-calendar" />{p.kind === 'range' && <div className="shadcn-calendar-range"><span>9月 {Number(p.selectedDay)} 日</span><b>→</b><span>9月 {Number(p.endDay)} 日</span></div>}{p.kind === 'events' && <div className="shadcn-calendar-events"><strong>9月 {Number(p.selectedDay)} 日 · {Number(p.eventCount)} 项安排</strong>{['设计评审 · 10:00', '响应式验收 · 14:30', '发布预览 · 17:00'].map((event) => <span key={event}><i />{event}</span>)}</div>}</div>;
  if (name === 'DatePicker') return <><button ref={triggerRef} className="shadcn-date-picker" onClick={() => preview && setOpen(!open)}><span>▣</span>{String(p.placeholder ?? component.content ?? '选择日期')}<ChevronDown size={15} /></button><FloatingSurface anchorRef={triggerRef} open={open} className="shadcn-date-picker-popover"><MiniCalendar selectedDay={Number(p.selectedDay ?? 3)} className="shadcn-calendar" />{p.mode === 'range' && <small>结束日期：{String(p.endDay ?? 9)} 日</small>}</FloatingSurface></>;
  if (name === 'Field') return <fieldset className="shadcn-field"><label>{String(p.label ?? '字段')}</label>{slotContent.content ?? <input className={cn('shadcn-input', p.error && 'invalid')} placeholder={component.content || '请输入内容'} />}<small className={p.error ? 'error' : ''}>{String(p.error || p.description || '')}</small></fieldset>;
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
  if (name === 'Breadcrumb') return <nav className={cn('shadcn-breadcrumb', `kind-${p.kind ?? 'basic'}`)}>{items.map((item, index) => <span key={String(item.key)}><a>{String(item.label)}</a>{index < items.length - 1 && <ChevronRight size={14} />}</span>)}</nav>;
  if (name === 'Collapsible') return <Collapsible.Root className={cn('shadcn-collapsible', `kind-${p.kind ?? 'details'}`)} defaultOpen={Boolean(p.defaultOpen)}><Collapsible.Trigger><span>{component.content}</span><ChevronDown size={15} /></Collapsible.Trigger><Collapsible.Content>{slotContent.content ?? (p.kind === 'code' ? <pre>{`<HeroSection\n  title="Design with AI"\n  responsive\n/>`}</pre> : p.kind === 'filters' ? <div className="shadcn-collapsible-filters"><button>组件库：全部</button><button>状态：可用</button></div> : <div className="shadcn-collapsible-details"><span>尺寸 <b>1200 × 640</b></span><span>约束 <b>左右拉伸</b></span><span>状态 <b>可编辑</b></span></div>)}</Collapsible.Content></Collapsible.Root>;
  if (name === 'Command') return <div className="shadcn-command"><label><Search size={15} /><input placeholder={String(p.placeholder ?? '输入命令或搜索…')} /></label>{runtimeRecords(p.groups).map((group, index) => <section key={`${String(group.label)}-${index}`}><small>{String(group.label)}</small>{runtimeStrings(group.items).map((entry, itemIndex) => <button key={entry}><span>{itemIndex === 0 ? '⌘' : itemIndex === 1 ? '↗' : '•'}</span>{entry}<kbd>{itemIndex + 1}</kbd></button>)}</section>)}</div>;
  if (name === 'Menubar') return <div className={cn('shadcn-menubar', `kind-${p.kind ?? 'editor'}`)}>{runtimeRecords(p.menus).map((menu, index) => <button className={p.kind === 'media' && index === 0 ? 'active' : ''} key={String(menu.key)}>{String(menu.label)}</button>)}</div>;
  if (name === 'NavigationMenu') return <nav className={cn('shadcn-navigation-menu', `kind-${p.kind ?? 'product'}`)}><div>{items.map((item) => <button key={String(item.key)}>{String(item.label)}{p.kind !== 'account' && <ChevronDown size={13} />}</button>)}</div>{p.kind === 'mega' && <section><div><strong>设计平台</strong><span>AI 网站设计</span><span>可视化编辑器</span></div><div><strong>开发工具</strong><span>组件库</span><span>设计令牌</span></div><aside><b>NEW</b><strong>多人实时协作</strong><span>邀请团队一起完善网站</span></aside></section>}{p.kind === 'account' && <i>AI</i>}</nav>;
  if (name === 'Pagination') return p.kind === 'load-more' ? <nav className="shadcn-pagination load-more"><button onClick={() => preview && setPage(Math.min(Number(p.totalPages), page + 1))}>加载更多 · {page}/{Number(p.totalPages)}</button></nav> : <nav className={cn('shadcn-pagination', p.kind === 'compact' && 'compact')}><button aria-label="上一页" onClick={() => preview && setPage(Math.max(1, page - 1))}><ChevronLeft size={15} />{p.kind !== 'compact' && '上一页'}</button>{p.kind === 'compact' ? <span>{page} / {Number(p.totalPages)}</span> : Array.from({ length: Math.min(5, Number(p.totalPages ?? 8)) }, (_, index) => index + 1).map((value) => <button className={page === value ? 'active' : ''} onClick={() => preview && setPage(value)} key={value}>{value}</button>)}<button aria-label="下一页" onClick={() => preview && setPage(Math.min(Number(p.totalPages ?? 8), page + 1))}>{p.kind !== 'compact' && '下一页'}<ChevronRight size={15} /></button></nav>;
  if (name === 'Tabs') return <Tabs.Root className={cn('shadcn-tabs', p.orientation === 'vertical' && 'vertical')} orientation={p.orientation ?? 'horizontal'} defaultValue={String(p.defaultValue ?? items[0]?.key ?? '')}><Tabs.List>{items.map((item) => <Tabs.Trigger key={String(item.key)} value={String(item.key)}>{String(item.label)}</Tabs.Trigger>)}</Tabs.List>{items.map((item) => <Tabs.Content key={String(item.key)} value={String(item.key)}>{slotContent[`tab-${String(item.key)}`] ?? <p>{String(item.label)}设置内容。</p>}</Tabs.Content>)}</Tabs.Root>;

  if (name === 'Avatar') return <div className={cn('shadcn-avatar-shell', p.status && `status-${p.status}`)}><div className="shadcn-avatar">{p.src ? <img src={String(p.src)} alt="成员头像" /> : <span>{String(p.fallback ?? component.content)}</span>}</div>{p.status && <i />}</div>;
  if (name === 'Attachment') return <div className={cn('shadcn-attachment', `status-${p.status ?? 'uploading'}`)}><span>⌁</span><div><strong>{String(p.fileName ?? 'attachment.fig')}</strong><small>{String(p.fileSize ?? '')} · {p.status === 'complete' ? '已完成' : p.status === 'error' ? '上传失败' : `${Number(p.progress ?? 0)}%`}</small><i><b style={{ width: `${Math.min(100, Math.max(0, Number(p.progress ?? 0)))}%` }} /></i></div><button>×</button></div>;
  if (name === 'Badge') return <span className={cn('shadcn-badge', `variant-${p.variant ?? 'default'}`)}>{component.content}</span>;
  if (name === 'Bubble') return <div className={cn('shadcn-bubble', p.role === 'user' && 'user', p.thinking && 'thinking')}><i>{String(p.avatar ?? 'AI')}</i><div><p>{component.content}</p><small>{String(p.timestamp ?? '')}</small></div></div>;
  if (name === 'Card') {
    if (p.kind === 'activity') return <article className="shadcn-card kind-activity"><header><div><h3>{String(p.title)}</h3><p>{String(p.description)}</p></div><button>查看全部</button></header><div className="shadcn-card-activity-list">{[['林设计师', '更新了首页主视觉', '10:24'], ['AI Designer', '生成了移动端布局', '10:18'], ['陈产品', '添加了 3 条批注', '09:56']].map(([person, action, time], index) => <div key={person}><i>{person.slice(0, 1)}</i><span><strong>{person}</strong><small>{action}</small></span><time>{time}</time>{index < 2 && <b />}</div>)}</div></article>;
    if (p.kind === 'metric') return <article className="shadcn-card kind-metric featured"><header><p>{String(p.title)}</p><span>↗ 18.2%</span></header><div className="shadcn-card-metric"><strong>{slotContent.content ?? component.content}</strong><small>{String(p.description)}</small><div>{[32, 48, 42, 68, 57, 76, 92].map((height, index) => <i key={index} style={{ height: `${height}%` }} />)}</div></div></article>;
    return <article className="shadcn-card kind-project"><header><div className="shadcn-card-project-icon">W</div><div><h3>{String(p.title ?? '卡片')}</h3><p>{String(p.description ?? '')}</p></div><button>•••</button></header><div className="shadcn-card-content"><p>{slotContent.content ?? component.content}</p><div className="shadcn-card-project-footer"><span><i />设计中</span><div><b>林</b><b>陈</b><b>AI</b></div></div></div></article>;
  }
  if (name === 'Carousel') {
    const labels = runtimeStrings(p.labels);
    const activeLabel = labels[slide] ?? `轮播项 ${slide + 1}`;
    return <div className="shadcn-carousel"><div className="shadcn-carousel-track">{slotContent[`slide-${slide + 1}`] ?? <><span>{String(slide + 1).padStart(2, '0')}</span><strong>{activeLabel}</strong><small>使用左右按钮切换内容</small></>}</div><button aria-label="上一张" onClick={() => preview && setSlide((current) => current <= 0 ? (p.loop ? Math.max(0, labels.length - 1) : 0) : current - 1)}><ChevronLeft size={16} /></button><button aria-label="下一张" onClick={() => preview && setSlide((current) => current >= labels.length - 1 ? (p.loop ? 0 : current) : current + 1)}><ChevronRight size={16} /></button><nav>{labels.map((label, index) => <i className={index === slide ? 'active' : ''} key={label} />)}</nav></div>;
  }
  if (name === 'Chart') {
    const values = Array.isArray(p.values) ? p.values.map(Number) : [];
    if (p.kind === 'donut') return <div className="shadcn-chart kind-donut"><div className="shadcn-chart-heading"><span>{String(p.title)}</span><strong>{String(p.change)}</strong></div><div className="shadcn-chart-donut"><i style={{ background: `conic-gradient(#18181b 0 ${values[0]}%,#71717a ${values[0]}% ${values[0] + values[1]}%,#d4d4d8 ${values[0] + values[1]}% 100%)` }}><b>{values.reduce((sum, value) => sum + value, 0)}%</b></i><div>{runtimeStrings(p.labels).map((label, index) => <span key={label}><em style={{ background: ['#18181b', '#71717a', '#d4d4d8'][index] }} />{label}<b>{values[index]}%</b></span>)}</div></div></div>;
    if (p.kind === 'line') return <div className="shadcn-chart kind-line"><div className="shadcn-chart-heading"><span>{String(p.title)}</span><strong>{String(p.change)}</strong></div><svg viewBox="0 0 420 150" preserveAspectRatio="none"><polyline points={values.map((value, index) => `${index * (420 / Math.max(1, values.length - 1))},${150 - value * 1.35}`).join(' ')} /><path d={`M0 150 L${values.map((value, index) => `${index * (420 / Math.max(1, values.length - 1))} ${150 - value * 1.35}`).join(' L')} L420 150 Z`} /></svg><footer>{runtimeStrings(p.labels).map((label) => <small key={label}>{label}</small>)}</footer></div>;
    return <div className="shadcn-chart"><div className="shadcn-chart-heading"><span>{String(p.title)}</span><strong>{String(p.change)}</strong></div><div className="shadcn-chart-bars">{runtimeStrings(p.labels).map((label, index) => <div key={label}><i style={{ height: `${Math.max(12, values[index] ?? 0)}%` }} /><small>{label}</small></div>)}</div></div>;
  }
  if (name === 'DataTable' || name === 'Table') {
    const table = p.selectable ? <table className="shadcn-selectable-table"><thead><tr><th><input type="checkbox" checked={selectedTableRows.length === runtimeRows(p.rows).length} onChange={() => preview && setSelectedTableRows((current) => current.length ? [] : runtimeRows(p.rows).map((_, index) => index))} /></th>{runtimeStrings(p.columns).map((column) => <th key={column}>{column}</th>)}</tr></thead><tbody>{runtimeRows(p.rows).map((row, rowIndex) => <tr key={rowIndex}><td><input type="checkbox" checked={selectedTableRows.includes(rowIndex)} onChange={() => preview && setSelectedTableRows((current) => current.includes(rowIndex) ? current.filter((value) => value !== rowIndex) : [...current, rowIndex])} /></td>{row.map((cell, cellIndex) => <td key={cellIndex}>{cell}</td>)}</tr>)}</tbody></table> : <SimpleDataTable className="shadcn-data-table" columns={runtimeStrings(p.columns)} rows={runtimeRows(p.rows)} striped={Boolean(p.striped)} />;
    return <div className={cn('shadcn-table-shell', `kind-${p.kind ?? 'basic'}`)}>{p.kind === 'toolbar' && <header><label><Search size={14} /><input placeholder="搜索资源…" /></label><button>筛选</button><button>导出</button></header>}{p.selectable && <header><span>{selectedTableRows.length} 项已选择</span><button>批量检查</button></header>}{table}</div>;
  }
  if (name === 'Empty') return <div className={cn('shadcn-empty', `kind-${p.kind ?? 'projects'}`)}><span>{p.kind === 'search' ? '⌕' : p.kind === 'offline' ? '!' : '◇'}</span><h3>{String(p.title)}</h3><p>{String(p.description)}</p><ShadcnButton variant={p.kind === 'offline' ? 'outline' : 'default'}>{String(p.actionLabel)}</ShadcnButton></div>;
  if (name === 'Item') return <div className="shadcn-item"><i>{String(p.icon ?? '◈')}</i><div><strong>{String(p.title ?? '内容条目')}</strong><span>{String(p.description ?? '')}</span></div>{p.action && <button>{String(p.action)}</button>}</div>;
  if (name === 'Message') return <div className={cn('shadcn-message', `role-${p.role ?? 'assistant'}`)}><i>{String(p.avatar ?? 'AI')}</i><div><header><strong>{String(p.sender ?? 'AI')}</strong><small>{String(p.time ?? '')}</small></header><p>{component.content}</p></div></div>;
  if (name === 'MessageScroller') return <div className="shadcn-message-scroller">{Array.from({ length: Number(p.messageCount ?? 6) }, (_, index) => <div className={index % 3 === 2 ? 'user' : ''} key={index}><i>{index % 3 === 2 ? '你' : 'AI'}</i><p>{p.kind === 'activity' ? `设计活动 ${index + 1} 已记录` : index % 3 === 2 ? '继续调整这个区域的视觉层级。' : '已完成组件分析并生成修改建议。'}</p></div>)}</div>;

  if (name === 'Alert') return <div className={cn('shadcn-alert', p.variant === 'destructive' && 'destructive')}><CircleAlert size={18} /><div><h4>{String(p.title ?? '提示')}</h4><p>{component.content}</p></div></div>;
  if (name === 'Progress') return <div className="shadcn-progress"><i style={{ width: `${Math.min(100, Math.max(0, Number(p.value ?? 0)))}%` }} /></div>;
  if (name === 'Skeleton') return <SkeletonComposition kind={String(p.kind ?? 'text')} lines={Number(p.lines ?? 3)} className="shadcn-skeleton-composition" />;
  if (name === 'Spinner') return p.kind === 'dots' ? <div className="shadcn-loading-dots"><span><i /><i /><i /></span><strong>{String(p.label)}</strong></div> : <div className={cn('shadcn-spinner-shell', p.kind === 'label' && 'with-label')}><div className="shadcn-spinner" style={{ width: Number(p.size ?? 28), height: Number(p.size ?? 28) }} />{p.label && <span>{String(p.label)}</span>}</div>;
  if (name === 'Toast') return showcase ? <div className="shadcn-toast-showcase"><ShadcnButton ref={triggerRef} onClick={() => setToastOpen(!toastOpen)}>{component.content}</ShadcnButton><ShadcnToastBody props={p} /></div> : <><ShadcnButton ref={triggerRef} className="fill" onClick={() => { if (!preview) return; setToastOpen(true); window.setTimeout(() => setToastOpen(false), 2500); }}>{component.content}</ShadcnButton><FloatingSurface anchorRef={triggerRef} open={toastOpen} className="shadcn-toast-surface"><ShadcnToastBody props={p} onClose={() => setToastOpen(false)} /></FloatingSurface></>;

  if (name === 'AlertDialog') {
    const body = ShadcnAlertDialogBody(p);
    if (showcase) return <div className="shadcn-alert-dialog-showcase"><ShadcnButton ref={triggerRef} variant={p.kind === 'delete' ? 'destructive' : 'outline'} onClick={() => setOpen(!open)}>{component.content}</ShadcnButton><section>{body}<footer><ShadcnButton variant="outline">取消</ShadcnButton><ShadcnButton variant={p.kind === 'delete' ? 'destructive' : 'default'}>{p.kind === 'publish' ? '确认发布' : p.kind === 'discard' ? '放弃修改' : '确认删除'}</ShadcnButton></footer></section></div>;
    return <><ShadcnButton ref={triggerRef} className="fill" variant={p.kind === 'delete' ? 'destructive' : 'default'} onClick={() => preview && setOpen(true)}>{component.content}</ShadcnButton><DesignOverlay anchorRef={triggerRef} open={open} side="center" size={String(p.size ?? 'sm')} title={String(p.title)} className="shadcn-overlay" onClose={() => setOpen(false)} footer={<><ShadcnButton variant="outline" onClick={() => setOpen(false)}>取消</ShadcnButton><ShadcnButton variant={p.kind === 'delete' ? 'destructive' : 'default'} onClick={() => setOpen(false)}>{p.kind === 'publish' ? '确认发布' : p.kind === 'discard' ? '放弃修改' : '确认删除'}</ShadcnButton></>}>{slotContent.content ?? body}</DesignOverlay></>;
  }
  if (name === 'Dialog' || name === 'Drawer' || name === 'Sheet') {
    const side = name === 'Dialog' ? 'center' : String(p.side ?? 'right');
    return <><ShadcnButton ref={triggerRef} className="fill" onClick={() => preview && setOpen(true)}>{component.content}</ShadcnButton><DesignOverlay anchorRef={triggerRef} open={open} side={side} size={String(p.size ?? 'md')} title={String(p.title ?? name)} className="shadcn-overlay" onClose={() => setOpen(false)} footer={<><ShadcnButton variant="outline" onClick={() => setOpen(false)}>取消</ShadcnButton><ShadcnButton onClick={() => setOpen(false)}>保存修改</ShadcnButton></>}>{slotContent.content ?? <div className="shadcn-overlay-default"><p>{String(p.description ?? '这是可交互的 shadcn/ui 浮层，可以继续放入表单或展示组件。')}</p><input className="shadcn-input" placeholder="输入内容" /></div>}</DesignOverlay></>;
  }
  if (name === 'DropdownMenu') {
    const menu = <div className={cn('shadcn-dropdown', `kind-${p.kind ?? 'account'}`)}>{p.kind === 'account' && <header><i>AI</i><span><strong>AI Designer</strong><small>designer@example.com</small></span></header>}{items.map((item, index) => { const key = String(item.key); const checked = checkedMenuItems.includes(key); return <button className={item.danger ? 'danger' : ''} key={key} onClick={() => { if (p.kind === 'checkbox') setCheckedMenuItems((current) => current.includes(key) ? current.filter((value) => value !== key) : [...current, key]); else setOpen(false); }}>{p.kind === 'checkbox' && <i>{checked ? '✓' : ''}</i>}{String(item.label)}{item.shortcut && <span>{String(item.shortcut)}</span>}{p.kind === 'actions' && index === 1 && <b />}</button>; })}</div>;
    return showcase ? <div className="shadcn-floating-showcase"><ShadcnButton ref={triggerRef} variant="outline">{component.content}<ChevronDown size={15} /></ShadcnButton>{menu}</div> : <><ShadcnButton ref={triggerRef} className="fill" variant="outline" onClick={() => preview && setOpen(!open)}>{component.content}<ChevronDown size={15} /></ShadcnButton><FloatingSurface anchorRef={triggerRef} open={open} className="shadcn-floating-reset">{menu}</FloatingSurface></>;
  }
  if (name === 'ContextMenu') {
    const menu = <div className="shadcn-dropdown">{items.map((item) => <button key={String(item.key)} onClick={() => setOpen(false)}>{String(item.label)}</button>)}</div>;
    return showcase ? <div className="shadcn-floating-showcase"><div ref={contextRef} className="shadcn-context-target">{component.content}<MoreHorizontal size={18} /></div>{menu}</div> : <><div ref={contextRef} className="shadcn-context-target" onContextMenu={(event) => { event.preventDefault(); if (preview) setOpen(true); }}>{component.content}<MoreHorizontal size={18} /></div><FloatingSurface anchorRef={contextRef} open={open} className="shadcn-floating-reset">{menu}</FloatingSurface></>;
  }
  if (name === 'HoverCard') {
    const content = slotContent.popup ?? ShadcnHoverBody(p);
    return showcase ? <div className="shadcn-floating-showcase"><ShadcnButton ref={hoverRef} variant="outline">{component.content}</ShadcnButton><div className="shadcn-hover-card">{content}</div></div> : <><ShadcnButton ref={hoverRef} className="fill" variant="outline" onMouseEnter={() => preview && setOpen(true)} onMouseLeave={() => setOpen(false)}>{component.content}</ShadcnButton><FloatingSurface anchorRef={hoverRef} open={open} className="shadcn-hover-card">{content}</FloatingSurface></>;
  }
  if (name === 'Tooltip') {
    const content = ShadcnTooltipBody(p);
    return showcase ? <div className="shadcn-tooltip-showcase"><div className={cn('shadcn-tooltip', p.kind === 'rich' && 'rich')}>{content}</div><ShadcnButton ref={hoverRef} variant="outline">{component.content}</ShadcnButton></div> : <><ShadcnButton ref={hoverRef} className="fill" variant="outline" onMouseEnter={() => preview && setOpen(true)} onMouseLeave={() => setOpen(false)}>{component.content}</ShadcnButton><FloatingSurface anchorRef={hoverRef} open={open} className={cn('shadcn-tooltip', p.kind === 'rich' && 'rich')}>{content}</FloatingSurface></>;
  }
  if (name === 'Popover') {
    const content = <div className="shadcn-popover"><h4>{String(p.title)}</h4>{slotContent.popup ?? ShadcnPopoverBody(p)}</div>;
    return showcase ? <div className="shadcn-floating-showcase"><ShadcnButton ref={triggerRef} variant="outline">{component.content}</ShadcnButton>{content}</div> : <><ShadcnButton ref={triggerRef} className="fill" variant="outline" onClick={() => preview && setOpen(!open)}>{component.content}</ShadcnButton><FloatingSurface anchorRef={triggerRef} open={open} className="shadcn-floating-reset">{content}</FloatingSurface></>;
  }

  return <div className="shadcn-fallback"><span>shadcn/ui</span><strong>{name}</strong></div>;
}

function ShadcnAccordionItem({ item, slotContent }: { item: Record<string, any>; slotContent: Record<string, ReactNode> }) {
  return <Accordion.Item value={String(item.key)}><Accordion.Header><Accordion.Trigger>{String(item.label)}<ChevronDown size={16} /></Accordion.Trigger></Accordion.Header><Accordion.Content>{slotContent[`panel-${String(item.key)}`] ?? <p>{String(item.label)}的详细内容。</p>}</Accordion.Content></Accordion.Item>;
}
