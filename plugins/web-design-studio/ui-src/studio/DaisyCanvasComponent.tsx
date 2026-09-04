import { useState, type PointerEvent as ReactPointerEvent, type ReactNode } from 'react';
import type { WebDesignComponent, WebDesignTokens } from '../../src/schema';
import { designStyleScopeProps } from './component-style';
import './daisyui-runtime.css';

type AnyProps = Record<string, any>;

function cx(...values: Array<string | false | null | undefined>) {
  return values.filter(Boolean).join(' ');
}

function strings(value: unknown, fallback: string[]): string[] {
  return Array.isArray(value) ? value.map(String) : fallback;
}

export function DaisyCanvasComponent({ component, preview, showcase = false, tokens, slotContent = {} }: {
  component: WebDesignComponent;
  preview: boolean;
  showcase?: boolean;
  tokens?: WebDesignTokens;
  slotContent?: Record<string, ReactNode>;
}) {
  const props = (component.library?.props ?? {}) as AnyProps;
  const mode = String(props.mode ?? 'standard');
  const className = String(props.className ?? '');
  const id = component.library?.component ?? 'Card';
  const scope = designStyleScopeProps(component.style);
  return <div
    className={cx('daisy-runtime', scope.className)}
    data-theme={mode === 'neutral' || mode === 'editorial' ? 'dark' : 'light'}
    style={{
      ...scope.style,
      '--color-primary': tokens?.colors.primary,
      '--color-base-100': tokens?.colors.surface,
      '--color-base-content': tokens?.colors.text
    } as React.CSSProperties}
  >
    <div className={cx('daisy-demo', `mode-${mode}`)}><DaisyRenderer id={id} component={component} props={props} mode={mode} className={className} preview={preview} showcase={showcase} slotContent={slotContent} /></div>
  </div>;
}

function DaisyRenderer({ id, component, props, mode, className, preview, showcase, slotContent }: {
  id: string;
  component: WebDesignComponent;
  props: AnyProps;
  mode: string;
  className: string;
  preview: boolean;
  showcase: boolean;
  slotContent: Record<string, ReactNode>;
}) {
  const [active, setActive] = useState(false);
  const [selected, setSelected] = useState(0);
  const [checked, setChecked] = useState(mode !== 'standard' && mode !== 'primary');
  const [range, setRange] = useState(Number(props.value ?? 58));
  const [files, setFiles] = useState<string[]>([]);
  const title = String(props.title ?? component.content);
  const items = strings(props.items, ['设计', '开发', '发布']);

  if (id === 'Accordion') return <div className="daisy-fill daisy-grid">{items.map((item, index) => <div key={item} className={cx('collapse bg-base-100 border border-base-300', className)}><input type="radio" name={`accordion-${component.id}`} defaultChecked={index === 0} /><div className="collapse-title font-semibold">{item}</div><div className="collapse-content text-sm">{index === 0 ? '整理内容结构与视觉层级。' : '这里可以继续放入自由组件。'}</div></div>)}</div>;

  if (id === 'Alert') return <div role="alert" className={cx('alert daisy-fill', className)}><span>{mode === 'success' ? '✓' : mode === 'warning' ? '!' : mode === 'error' ? '×' : 'i'}</span><div><strong>{title}</strong><div>{mode === 'error' ? '保存失败，请检查输入。' : mode === 'success' ? '设计已经安全保存。' : '组件内容和状态都可继续编辑。'}</div></div>{mode === 'warning' && <button className="btn btn-sm">处理</button>}</div>;

  if (id === 'Aura') return <div className={cx('aura daisy-aura-card', mode === 'secondary' ? 'aura-secondary' : 'aura-primary')}><div><small>DAISYUI 5</small><h3>{title}</h3><button className="btn btn-primary btn-sm">Explore</button></div></div>;

  if (id === 'Avatar') {
    const avatar = (label: string, online = false) => <div className={cx('avatar', online && 'avatar-online')}><div className={cx('w-16 rounded-full', mode === 'ring' && 'ring ring-primary ring-offset-base-100 ring-offset-2')}><div className="daisy-avatar-art">{label}</div></div></div>;
    return mode === 'group' ? <div className="avatar-group -space-x-6">{['林', '陈', 'AI'].map((item) => <span key={item}>{avatar(item, item === 'AI')}</span>)}<div className="avatar avatar-placeholder"><div className="bg-neutral text-neutral-content w-16 rounded-full"><span>+9</span></div></div></div> : mode === 'placeholder' ? <div className="avatar avatar-placeholder"><div className="bg-neutral text-neutral-content w-20 rounded-full"><span className="text-xl">UI</span></div></div> : avatar('AI', mode === 'ring');
  }

  if (id === 'Badge') return <div className="daisy-actions"><span className={cx('badge', className)}>{title}</span><span className={cx('badge', className)}>v5.7</span>{mode === 'dash' && <span className="badge badge-success badge-dash">Live</span>}</div>;

  if (id === 'Breadcrumbs') return <div className="breadcrumbs text-sm daisy-fill"><ul>{(mode === 'compact' ? ['首页', '编辑器'] : ['项目', '网站设计', '响应式画布']).map((item) => <li key={item}><a>{item}</a></li>)}</ul></div>;

  if (id === 'Button') return <div className="daisy-actions"><button className={cx('btn', className)} onClick={() => preview && setActive(!active)}>{active ? '✓ 已执行' : title}{mode !== 'wide' && <span>→</span>}</button>{mode === 'soft' && <button className="btn btn-ghost">取消</button>}</div>;

  if (id === 'Calendar') return <div className="card bg-base-100 shadow-sm daisy-fill"><div className="card-body p-4"><div className="flex justify-between"><button className="btn btn-ghost btn-sm">‹</button><strong>September 2026</strong><button className="btn btn-ghost btn-sm">›</button></div><div className="grid grid-cols-7 gap-1 text-center text-xs">{['S','M','T','W','T','F','S'].map((d,i)=><b key={`${d}-${i}`} className="opacity-50">{d}</b>)}{Array.from({length:35},(_,i)=>{const day=i-1;return <button key={i} className={cx('btn btn-xs',day===Number(props.selectedDay??18)?'btn-primary':'btn-ghost',day<1||day>30?'invisible':'')}>{day>0&&day<=30?day:''}</button>})}</div></div></div>;

  if (id === 'Card') return <article className={cx('card bg-base-100 shadow-xl daisy-fill', className)}>{mode === 'image' && <figure className="daisy-placeholder-art image-full-art"><span /></figure>}{mode === 'side' && <figure className="daisy-placeholder-art w-2/5"><strong>01</strong></figure>}<div className="card-body">{slotContent.content ?? <><div className="badge badge-primary badge-soft">PRODUCT</div><h2 className="card-title">{title}</h2><p>用官方结构组合图片、正文、状态和操作。</p><div className="card-actions justify-end"><button className="btn btn-primary btn-sm">立即查看</button></div></>}</div></article>;

  if (id === 'Carousel') return <div className={cx('carousel daisy-fill', mode === 'center' && 'carousel-center', mode === 'full' && 'rounded-none')}>{items.concat('品牌').map((item,index)=><div key={`${item}-${index}`} className="carousel-item daisy-gallery-card"><strong>{item}</strong><span>{String(index+1).padStart(2,'0')}</span></div>)}</div>;

  if (id === 'Chat') return <div className="daisy-fill"><div className="chat chat-start"><div className="chat-image avatar"><div className="w-10 rounded-full"><div className="daisy-avatar-art">AI</div></div></div><div className="chat-header">AI Designer <time className="text-xs opacity-50">10:24</time></div><div className="chat-bubble">我已经把首屏调整为响应式布局。</div></div><div className="chat chat-end"><div className="chat-bubble chat-bubble-primary">很好，再加强一点视觉层次。</div></div></div>;

  if (id === 'Checkbox') return <div className={cx('daisy-grid', mode === 'list' && 'daisy-fill')}>{(mode === 'list' ? items : ['保持响应式布局']).map((item,index)=><label className="daisy-inline-label" key={item}><input type="checkbox" className={cx('checkbox',className)} checked={index===0?checked:index<2} onChange={() => index===0&&preview&&setChecked(!checked)} /><span>{item}</span></label>)}</div>;

  if (id === 'Collapse') return <div className={cx('collapse bg-base-100 border border-base-300 daisy-fill', active && 'collapse-open')}><button className="collapse-title font-semibold text-left" onClick={() => preview&&setActive(!active)}>{title}<span className="float-right">{active?'−':'+'}</span></button><div className="collapse-content">{slotContent.content ?? <p>展开后可以继续设计任意内容与操作。</p>}</div></div>;

  if (id === 'Countdown') return <div className="daisy-actions text-center"><div><span className="countdown font-mono text-5xl"><span style={{'--value':10} as React.CSSProperties} /></span><small className="block">days</small></div><span className="text-3xl">:</span><div><span className="countdown font-mono text-5xl"><span style={{'--value':24} as React.CSSProperties} /></span><small className="block">hours</small></div><span className="text-3xl">:</span><div><span className="countdown font-mono text-5xl"><span style={{'--value':36} as React.CSSProperties} /></span><small className="block">min</small></div></div>;

  if (id === 'Diff') return mode === 'split' ? <div className="grid grid-cols-2 daisy-fill gap-2"><div className="daisy-placeholder-art rounded-box opacity-60"><strong>Before</strong></div><div className="daisy-placeholder-art rounded-box"><strong>After</strong></div></div> : mode === 'stacked' ? <div className="daisy-grid daisy-fill"><div className="alert">− 固定画布</div><div className="alert alert-success">+ 响应式约束</div><div className="alert alert-info">+ 可编辑状态</div></div> : <div className="diff aspect-video daisy-fill"><div className="diff-item-1"><div className="daisy-placeholder-art bg-primary text-primary-content"><strong>After</strong></div></div><div className="diff-item-2"><div className="daisy-placeholder-art"><strong>Before</strong></div></div><div className="diff-resizer" /></div>;

  if (id === 'Divider') return mode === 'vertical' ? <div className="flex h-20 daisy-fill"><div className="grid grow place-items-center">Design</div><div className="divider divider-horizontal">OR</div><div className="grid grow place-items-center">Code</div></div> : <div className="daisy-fill text-center">视觉设计<div className="divider">{mode==='labeled'?'AND':''}</div>交互体验</div>;

  if (id === 'Dock') return <div className="dock dock-sm relative daisy-fill rounded-box border border-base-300">{['⌂','⌕','＋','♡','☻'].map((icon,index)=><button key={icon} className={selected===index?'dock-active':''} onClick={()=>preview&&setSelected(index)}><span>{icon}</span><span className="dock-label">{['首页','搜索','创建','收藏','我的'][index]}</span></button>)}</div>;

  if (id === 'Drawer') return <div className={cx('drawer daisy-drawer-preview',className)}><input className="drawer-toggle" type="checkbox" checked={active||showcase} readOnly /><div className="drawer-content p-5"><button className="btn btn-primary btn-sm" onClick={()=>preview&&setActive(true)}>打开抽屉</button><p className="mt-4">页面内容保持在原设计中。</p></div>{(active||showcase)&&<div className="drawer-side"><button className="drawer-overlay" aria-label="关闭抽屉" onClick={()=>setActive(false)} /><aside className="menu bg-base-200 min-h-full w-64 p-4">{slotContent.content ?? <><li><strong>{mode==='navigation'?'工作区导航':'抽屉内容'}</strong></li>{items.map(i=><li key={i}><a>{i}</a></li>)}</>}</aside></div>}</div>;

  if (id === 'Dropdown') return <div className={cx('dropdown',className,(active||showcase)&&'dropdown-open')}><button tabIndex={0} className="btn" onClick={()=>preview&&setActive(!active)}>打开菜单 ⌄</button>{(active||showcase)&&<ul className="dropdown-content menu bg-base-100 rounded-box z-10 w-52 p-2 shadow-sm">{items.map(i=><li key={i}><button onClick={()=>setActive(false)}>{i}</button></li>)}</ul>}</div>;

  if (id === 'Fab') return <div className={cx('fab relative',(active||showcase)&&'fab-open',mode==='flower'&&'fab-flower')}><button className="btn btn-lg btn-circle btn-primary" onClick={()=>preview&&setActive(!active)}>＋</button>{(active||showcase)&&<div>{['✎','↗','⌁'].map(icon=><button key={icon} className="btn btn-circle">{icon}</button>)}</div>}</div>;

  if (id === 'Fieldset') return <fieldset className="fieldset bg-base-200 border-base-300 rounded-box w-full border p-4"><legend className="fieldset-legend">项目设置</legend>{slotContent.content ?? <><label className="label">网站名称</label><input className="input daisy-fill" defaultValue="Design Studio" /><label className="label">团队</label><select className="select daisy-fill"><option>Product Design</option><option>Engineering</option></select><button className="btn btn-primary mt-3">保存设置</button></>}</fieldset>;

  if (id === 'FileInput') return <label className="form-control daisy-fill"><span className="label mb-2">上传项目素材</span><input type="file" multiple className={cx('file-input daisy-fill',className)} onChange={e=>setFiles(Array.from(e.target.files??[]).map(f=>f.name))}/><small className="mt-2 opacity-60">{files.length?files.join(' · '):'支持图片、PDF 与设计文件'}</small></label>;

  if (id === 'Filter') return <form className={cx('filter',mode==='vertical'&&'filter-vertical')}><input className="btn btn-square" type="reset" value="×" />{items.map((item,index)=><input key={item} className="btn" type="radio" name={`filter-${component.id}`} aria-label={item} defaultChecked={index===selected} onClick={()=>preview&&setSelected(index)}/>)}</form>;

  if (id === 'Footer') return <footer className={cx('footer bg-neutral text-neutral-content rounded-box daisy-fill',mode==='compact'?'footer-horizontal items-center p-5':'p-8')}>{slotContent.content ?? <><aside><strong className="text-lg">Design Studio</strong><p>AI 与人共同设计网站。</p></aside><nav><h6 className="footer-title">Product</h6><a className="link link-hover">Editor</a><a className="link link-hover">Components</a><a className="link link-hover">Publish</a></nav><nav><h6 className="footer-title">Company</h6><a className="link link-hover">About</a><a className="link link-hover">Contact</a></nav></>}</footer>;

  if (id === 'Hero') return <section className="hero bg-base-200 rounded-box daisy-fill"><div className={cx('hero-content',mode==='editorial'?'justify-start text-left':'text-center')} >{slotContent.content ?? <div className="max-w-md"><span className="badge badge-primary badge-soft">NEW</span><h1 className="text-4xl font-bold mt-3">{title}</h1><p className="py-5">用可编辑组件与 AI 快速设计真正好看的网站。</p><button className="btn btn-primary">开始设计</button></div>}</div></section>;

  if (id === 'Hover3D') return <div className="hover-3d" onPointerMove={(event)=>setTilt(event)}><div className="hover-3d-content daisy-hover3d-card"><div className="badge badge-secondary">{mode.toUpperCase()}</div><h3>{title}</h3><p>移动指针查看真实 3D 倾斜反馈。</p><div className="card-actions mt-5"><button className="btn btn-primary btn-sm">Open</button></div></div></div>;

  if (id === 'HoverGallery') return <div className="hover-gallery daisy-fill">{['Design','Motion','Brand','Launch'].map((item,index)=><div key={item} className="daisy-gallery-card"><strong>{item}</strong><span>0{index+1}</span></div>)}</div>;

  if (id === 'Indicator') return <div className="indicator"><span className="indicator-item badge badge-primary">{mode==='compact'?'3':'NEW'}</span><div className="card bg-base-100 border border-base-300 w-56"><div className="card-body"><h3 className="card-title">{title}</h3><p>新组件已经可用</p></div></div></div>;

  if (id === 'Input' || id === 'Label') return <label className="form-control daisy-fill"><span className="label mb-2">{mode==='search'?'搜索组件':title}</span><input className={cx('input daisy-fill',className)} placeholder={String(props.placeholder??'输入内容…')} defaultValue={mode==='error'?'invalid value':''}/>{mode==='error'&&<span className="validator-hint text-error mt-1">请输入有效内容</span>}</label>;

  if (id === 'Join') return <div className={cx('join',mode==='editorial'&&'join-vertical')}><input className="input join-item" placeholder="搜索组件"/><select className="select join-item"><option>全部</option><option>交互</option></select><button className="btn btn-primary join-item">搜索</button></div>;

  if (id === 'Kbd') return <div className="daisy-actions"><kbd className="kbd kbd-lg">⌘</kbd><span>+</span><kbd className="kbd kbd-lg">K</kbd><span className="opacity-60">打开命令面板</span></div>;

  if (id === 'Link') return <div className="daisy-grid"><a className={cx('link',mode==='primary'?'link-primary':mode==='neutral'?'link-neutral':'link-hover')}>{title} ↗</a>{mode==='outline'&&<a className="link link-secondary">查看组件文档</a>}</div>;

  if (id === 'List') return <ul className="list bg-base-100 rounded-box shadow-md daisy-fill">{items.map((item,index)=><li key={item} className="list-row"><div className="avatar avatar-placeholder"><div className="bg-primary text-primary-content size-10 rounded-box"><span>{index+1}</span></div></div><div><strong>{item}</strong><div className="text-xs opacity-60">{['今天 10:24','昨天 18:40','星期二 09:18'][index]}</div></div><button className="btn btn-square btn-ghost">›</button></li>)}</ul>;

  if (id === 'Loading') return <div className="daisy-actions"><span className={cx('loading loading-lg',className)} /><strong>{mode==='dots'?'正在生成页面…':'加载设计系统'}</strong></div>;

  if (id === 'Mask') return <div className={cx('mask size-40 daisy-placeholder-art',className)}><strong>{mode==='heart'?'♥':mode==='star'?'★':'AI'}</strong></div>;

  if (id === 'Megamenu') return <div className="daisy-fill"><nav className="navbar bg-base-100 rounded-box shadow-sm"><strong className="px-3">Studio</strong><div className="megamenu">{items.map((item,index)=><button key={item} onMouseEnter={()=>preview&&setSelected(index)}>{item}</button>)}</div></nav><div className="mt-2 grid grid-cols-3 gap-2 rounded-box bg-base-100 p-4 shadow"><div><strong>{items[selected]}</strong><p className="text-sm opacity-60">完整产品能力</p></div><a className="link">开始使用</a><a className="link">阅读文档</a></div></div>;

  if (id === 'Menu') return <ul className={cx('menu bg-base-200 rounded-box daisy-fill',mode==='compact'&&'menu-sm',mode==='editorial'&&'menu-horizontal')}>{items.concat('设置').map((item,index)=><li key={item}><button className={selected===index?'menu-active':''} onClick={()=>preview&&setSelected(index)}><span>{['⌂','✦','↗','⚙'][index]}</span>{item}</button></li>)}</ul>;

  if (id.startsWith('Mockup')) return <Mockup id={id} mode={mode} title={title} />;

  if (id === 'Modal') return <div className="daisy-fill"><button className="btn btn-primary" onClick={()=>preview&&setActive(true)}>打开模态框</button>{(active||showcase)&&<div className={cx('modal modal-open daisy-modal-inline',className)}><div className={cx('modal-box',mode==='bottom'&&'daisy-modal-bottom',mode==='sheet'&&'daisy-modal-sheet')}>{slotContent.content ?? (mode==='sheet'?<><div className="grid grid-cols-2 gap-4"><div><span className="badge badge-primary badge-soft">SETTINGS PANEL</span><h3 className="text-xl font-bold mt-3">{title}</h3><p className="mt-2 opacity-60">全宽面板适合较复杂的配置与详情。</p></div><div className="daisy-grid"><label className="label">项目名称</label><input className="input daisy-fill" defaultValue="Launch project"/><label className="label">发布环境</label><select className="select daisy-fill"><option>Production</option><option>Preview</option></select></div></div><div className="modal-action"><button className="btn" onClick={()=>setActive(false)}>取消</button><button className="btn btn-primary">保存设置</button></div></>:<><div className={mode==='bottom'?'daisy-modal-handle':''}/><h3 className="text-lg font-bold">{title}</h3><p className="py-4">{mode==='bottom'?'从页面底部打开，适合移动端操作。':'在当前页面预览范围内编辑表单和展示内容。'}</p><input className="input daisy-fill" defaultValue="Launch project"/><div className="modal-action"><button className="btn" onClick={()=>setActive(false)}>取消</button><button className="btn btn-primary">确认</button></div></>)}</div></div>}</div>;

  if (id === 'Navbar') return <nav className="navbar bg-base-100 rounded-box shadow-sm daisy-fill">{slotContent.content ?? <><div className="navbar-start"><button className="btn btn-ghost text-xl">Studio</button></div><div className="navbar-center hidden lg:flex"><ul className="menu menu-horizontal"><li><a>产品</a></li><li><a>组件</a></li><li><a>案例</a></li></ul></div><div className="navbar-end"><button className="btn btn-primary btn-sm">开始设计</button></div></>}</nav>;

  if (id === 'Otp') { const count=mode==='four'?4:6; return <div className="join">{Array.from({length:count},(_,index)=><input key={index} className="input join-item w-12 text-center" maxLength={1} defaultValue={mode==='masked'&&index<3?'•':index<2?String(index+2):''}/>)}</div>; }

  if (id === 'Pagination') return <div className="join">{['«','1','2','3','»'].map((item,index)=><button key={item} className={cx('join-item btn',selected===index&&'btn-active')} onClick={()=>preview&&setSelected(index)}>{item}</button>)}</div>;

  if (id === 'Progress') return mode==='steps'?<div className="daisy-fill"><div className="flex justify-between text-xs mb-2"><span>Brief</span><span>Design</span><span>Launch</span></div><div className="grid grid-cols-3 gap-1">{[true,true,false].map((on,i)=><span key={i} className={cx('h-3 rounded-full',on?'bg-primary':'bg-base-300')}/>)}</div></div>:<div className="daisy-fill"><div className="flex justify-between mb-2"><strong>{title}</strong><span>{range}%</span></div><progress className={cx('progress daisy-fill',className)} value={range} max="100" /></div>;

  if (id === 'RadialProgress') return <div className={cx('radial-progress text-primary',mode==='thick'&&'border-8 border-primary border-opacity-10')} style={{'--value':range,'--size':mode==='metric'?'9rem':'7rem','--thickness':mode==='thick'?'.8rem':'.45rem'} as React.CSSProperties} role="progressbar"><span className="text-xl font-bold">{range}%</span></div>;

  if (id === 'Radio') return <div className="daisy-grid">{['设计团队','开发团队','市场团队'].map((item,index)=><label key={item} className="daisy-inline-label"><input type="radio" name={`radio-${component.id}`} className={cx('radio',mode==='primary'?'radio-primary':mode==='neutral'?'radio-neutral':'radio-secondary')} defaultChecked={index===selected} onClick={()=>preview&&setSelected(index)}/><span>{item}</span></label>)}</div>;

  if (id === 'Range') return <div className="daisy-fill"><div className="flex justify-between mb-3"><strong>{title}</strong><span className="badge badge-primary">{range}</span></div><input type="range" min="0" max="100" value={range} className={cx('range',className)} onChange={e=>preview&&setRange(Number(e.target.value))}/>{mode==='steps'&&<div className="flex justify-between px-2.5 mt-2 text-xs"><span>0</span><span>25</span><span>50</span><span>75</span><span>100</span></div>}</div>;

  if (id === 'Rating') return <div className={cx('rating rating-lg',mode==='half'&&'rating-half')}>{Array.from({length:5},(_,index)=><input key={index} type="radio" name={`rating-${component.id}`} className={cx('mask',mode==='hearts'?'mask-heart bg-secondary':'mask-star-2 bg-orange-400')} aria-label={`${index+1} star`} defaultChecked={index===3}/>)}</div>;

  if (id === 'Select') return <label className="form-control daisy-fill"><span className="label mb-2">{title}</span><select className={cx('select daisy-fill',className)}>{mode==='grouped'?<><optgroup label="团队"><option>Design</option><option>Engineering</option></optgroup><optgroup label="业务"><option>Marketing</option></optgroup></>:strings(props.options,['Design','Engineering']).map(item=><option key={item}>{item}</option>)}</select>{mode==='error'&&<small className="text-error mt-1">请选择一个有效团队</small>}</label>;

  if (id === 'Skeleton') return <div className="daisy-fill flex gap-4"><div className="skeleton h-16 w-16 shrink-0 rounded-full"/><div className="flex-1 space-y-3"><div className="skeleton h-4 w-2/3"/><div className="skeleton h-4 w-full"/><div className="skeleton h-24 w-full"/></div></div>;

  if (id === 'Stack') return <div className="stack w-56">{['bg-primary text-primary-content','bg-secondary text-secondary-content','bg-accent text-accent-content'].map((tone,index)=><div key={tone} className={cx('card shadow-md',tone)}><div className="card-body"><strong>{['当前设计','上一版本','模板来源'][index]}</strong><span>v{3-index}</span></div></div>)}</div>;

  if (id === 'Stat') return <div className={cx('stats shadow daisy-stat-grid',mode==='compact'&&'mode-compact')}>{[['24.8K','访客','↗ 18%'],['98%','完成度','+12%'],['42ms','响应','−8ms']].map(row=><div className="stat" key={row[1]}><div className="stat-title">{row[1]}</div><div className="stat-value text-primary">{row[0]}</div><div className="stat-desc">{row[2]}</div></div>)}</div>;

  if (id === 'Status') return <div className="daisy-actions">{[['status-success','服务正常'],['status-warning','构建中'],['status-error','需处理']].map(([tone,label],index)=><span key={tone} className="daisy-inline-label"><span className={cx('status',mode==='neutral'?'status-neutral':tone,index===0&&mode==='primary'&&'status-primary')}/>{label}</span>)}</div>;

  if (id === 'Steps') return <ul className={cx('steps daisy-fill',className)}>{items.map((item,index)=><li key={item} className={cx('step',index<=selected&&'step-primary')} onClick={()=>preview&&setSelected(index)}>{item}</li>)}</ul>;

  if (id === 'Swap') return <label className="swap swap-rotate"><input type="checkbox" checked={checked} onChange={()=>preview&&setChecked(!checked)}/><div className="swap-on text-5xl">☀</div><div className="swap-off text-5xl">☾</div></label>;

  if (id === 'Tab') return <div className="daisy-fill"><div role="tablist" className={cx('tabs',className)}>{items.map((item,index)=><button role="tab" key={item} className={cx('tab',selected===index&&'tab-active')} onClick={()=>preview&&setSelected(index)}>{item}</button>)}</div><section className="mt-4 rounded-box bg-base-200 p-5">{slotContent[`tab-${items[selected]}`]??<><strong>{items[selected]}</strong><p className="mt-2 opacity-70">标签页内容支持独立设计和保存。</p></>}</section></div>;

  if (id === 'Table') return <div className="overflow-x-auto daisy-fill"><table className={cx('table',className)}><thead><tr><th>项目</th><th>负责人</th><th>状态</th><th>进度</th></tr></thead><tbody>{[['响应式官网','Lin','进行中','72%'],['组件系统','AI','已完成','100%'],['发布检查','Maya','待处理','28%']].map(row=><tr key={row[0]}><td><strong>{row[0]}</strong></td><td>{row[1]}</td><td><span className="badge badge-soft">{row[2]}</span></td><td>{row[3]}</td></tr>)}</tbody></table></div>;

  if (id === 'TextRotate') return <div className={cx('daisy-text-rotate',mode==='hero'&&'text-4xl')}><span>网站设计</span><span className="text-rotate"><span>{items.join('\n')}</span></span></div>;

  if (id === 'Textarea') return <label className="form-control daisy-fill"><span className="label mb-2">设计说明</span><textarea className="textarea textarea-bordered daisy-fill" rows={mode==='compact'?3:5} defaultValue="请描述页面目标、用户和你喜欢的视觉感觉。"/><span className="label text-xs opacity-60">AI 会使用这些信息完善页面。</span></label>;

  if (id === 'ThemeController') return mode==='cards'?<div className="grid grid-cols-3 gap-2 daisy-fill">{['light','dark','cupcake'].map((theme,index)=><button key={theme} className={cx('card border p-3 text-left',selected===index&&'border-primary')} data-theme={theme} onClick={()=>preview&&setSelected(index)}><span className="h-12 rounded-box bg-base-200 mb-2"/><strong>{theme}</strong></button>)}</div>:mode==='buttons'?<div className="join"><button className="btn join-item btn-active">Light</button><button className="btn join-item">Dark</button><button className="btn join-item">System</button></div>:<label className="swap swap-rotate"><input type="checkbox" className="theme-controller" value="dark" checked={checked} onChange={()=>preview&&setChecked(!checked)}/><span className="swap-on text-4xl">☾</span><span className="swap-off text-4xl">☀</span></label>;

  if (id === 'Timeline') return mode==='compact'?<div className="daisy-timeline-compact">{items.map((item,index)=><div key={item}><span className={cx('status',index<2?'status-success':'status-neutral')}/><strong>{item}</strong><small>Sep {12+index}</small></div>)}</div>:<ul className={cx('timeline daisy-fill',className)}>{items.map((item,index)=><li key={item}>{index>0&&<hr className={index<=1?'bg-primary':''}/>}<div className={index%2?'timeline-end timeline-box':'timeline-start timeline-box'}>{item}</div><div className="timeline-middle">{index<2?'✓':'○'}</div>{index<items.length-1&&<hr className={index<1?'bg-primary':''}/>}</li>)}</ul>;

  if (id === 'Toast') return <div className={cx('toast relative',className)}><div className="alert alert-info"><span>设计已保存</span></div>{mode==='stack'&&<><div className="alert alert-success"><span>组件已同步</span></div><div className="alert alert-warning"><span>还有 2 项待检查</span></div></>}</div>;

  if (id === 'Toggle') return <label className="daisy-inline-label"><input type="checkbox" className={cx('toggle',mode==='primary'?'toggle-primary':mode==='neutral'?'toggle-neutral':'toggle-secondary')} checked={checked} onChange={()=>preview&&setChecked(!checked)}/><strong>{checked?'已启用':'已关闭'}</strong></label>;

  if (id === 'Tooltip') return <div className={cx('tooltip',className)} data-tip={mode==='open'?'常显的完整提示内容':'查看组件说明'}><button className="btn" onClick={()=>preview&&setActive(!active)}>{title}</button></div>;

  if (id === 'Validator') return <label className="form-control daisy-fill"><span className="label mb-2">{mode==='password'?'账户密码':'网站域名'}</span><input className={cx('input validator daisy-fill',mode==='error'?'input-error':mode==='success'?'input-success':'')} required pattern={mode==='password'?'(?=.*\\d).{8,}':'[a-z0-9-]+'} defaultValue={mode==='success'?'design-studio':'Invalid value!'}/><p className={cx('validator-hint',mode==='success'&&'text-success')}>{mode==='password'?'至少 8 位并包含数字':mode==='success'?'✓ 域名可用':'只能使用小写字母、数字和连字符'}</p></label>;

  return <div className="alert"><span>i</span><strong>{title}</strong><span>{id}</span></div>;
}

function Mockup({ id, mode, title }: { id: string; mode: string; title: string }) {
  if (id === 'MockupCode') return <div className="mockup-code daisy-fill"><pre data-prefix="$"><code>{mode==='install'?'npm install daisyui':'npm run design'}</code></pre><pre data-prefix="✓" className="text-success"><code>components ready</code></pre><pre data-prefix=">"><code>{mode==='diff'?'+ responsive layout':'opening studio…'}</code></pre></div>;
  if (id === 'MockupPhone') return <div className="mockup-phone"><div className="mockup-phone-camera"/><div className="mockup-phone-display"><div className="daisy-phone-screen"><small>{mode.toUpperCase()} APP</small><h3 className="text-3xl font-bold">{title}</h3><button className="btn btn-primary btn-sm">Explore</button></div></div></div>;
  if (id === 'MockupWindow') return <div className="mockup-window border border-base-300 daisy-fill"><div className="daisy-browser-page"><span className="badge badge-primary badge-soft">{mode}</span><h3>{title}</h3><p>完整可编辑的应用窗口。</p><div className="skeleton h-20 w-full"/></div></div>;
  return <div className="mockup-browser border border-base-300 daisy-fill"><div className="mockup-browser-toolbar"><div className="input">https://design.local</div></div><div className="daisy-browser-page"><span className="badge badge-secondary badge-soft">{mode}</span><h3>{title}</h3><p>真正的网站内容，而不是静态截图。</p><button className="btn btn-primary btn-sm">Start designing</button></div></div>;
}

function setTilt(event: ReactPointerEvent<HTMLElement>) {
  const rect = event.currentTarget.getBoundingClientRect();
  event.currentTarget.style.setProperty('--rotate-x', `${((event.clientY - rect.top) / rect.height - .5) * -12}deg`);
  event.currentTarget.style.setProperty('--rotate-y', `${((event.clientX - rect.left) / rect.width - .5) * 12}deg`);
}
