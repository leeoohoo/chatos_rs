import { useMemo, useRef, useState, type PointerEvent, type ReactNode } from 'react';
import type { WebDesignComponent, WebDesignTokens } from '../../src/schema';
import { designStyleScopeProps } from './component-style';
import { INSPIRA_BACKGROUND_SLUGS } from '../../src/inspira-library';
import { LibraryRuntimeComponent } from './LibraryRuntimeComponent';

type AnyProps = Record<string, any>;

function cx(...values: Array<string | false | null | undefined>) {
  return values.filter(Boolean).join(' ');
}

function strings(value: unknown, fallback: string[] = []): string[] {
  return Array.isArray(value) ? value.map(String) : fallback;
}

function dots(count: number) {
  return Array.from({ length: count }, (_, index) => <i key={index} style={{ '--dot-index': index } as React.CSSProperties} />);
}

export function CreativeCanvasComponent({ component, preview, showcase = false, tokens, slotContent = {} }: {
  component: WebDesignComponent;
  preview: boolean;
  showcase?: boolean;
  tokens?: WebDesignTokens;
  slotContent?: Record<string, ReactNode>;
}) {
  const props = (component.library?.props ?? {}) as AnyProps;
  const family = String(props.family ?? 'card');
  const slug = String(props.componentSlug ?? component.library?.component ?? 'creative-component');
  const mode = String(props.mode ?? 'signature');
  const scope = designStyleScopeProps(component.style);
  return <div
    className={cx('creative-runtime', `library-${component.library?.name}`, `creative-family-${family}`, `creative-${slug}`, `mode-${mode}`, scope.className)}
    style={{
      ...scope.style,
      '--creative-accent': String(props.accent ?? tokens?.colors.primary ?? '#7c3aed'),
      '--creative-surface': tokens?.colors.surface ?? '#ffffff',
      '--creative-text': tokens?.colors.text ?? '#111827',
      fontFamily: tokens?.typography.fontFamily
    } as React.CSSProperties}
  >
    <CreativeRenderer component={component} family={family} slug={slug} props={props} preview={preview} showcase={showcase} slotContent={slotContent} />
  </div>;
}

function CreativeRenderer({ component, family, slug, props, preview, showcase, slotContent }: {
  component: WebDesignComponent;
  family: string;
  slug: string;
  props: AnyProps;
  preview: boolean;
  showcase: boolean;
  slotContent: Record<string, ReactNode>;
}) {
  const [active, setActive] = useState(false);
  const [copied, setCopied] = useState(false);
  const [checked, setChecked] = useState(Boolean(props.defaultChecked));
  const [value, setValue] = useState(58);
  const [lens, setLens] = useState({ x: 62, y: 42 });
  const [selected, setSelected] = useState(0);
  const [uploadedFiles, setUploadedFiles] = useState<string[]>(() => strings(props.files));
  const title = String(props.title ?? component.content);
  const items = strings(props.items, ['Design', 'Motion', 'Interaction']);
  const values = (Array.isArray(props.values) ? props.values : [32, 48, 41, 68, 57, 82]).map(Number);
  const mode = String(props.mode ?? 'signature');

  if (component.library?.name === 'inspira' && INSPIRA_BACKGROUND_SLUGS.has(slug)) {
    return <LibraryRuntimeComponent component={component} preview={preview} slotContent={slotContent.content} />;
  }

  if (family === 'card') return <article className="creative-card" onPointerMove={(event) => setPointerVariables(event)}>
    <div className="creative-card-glow" />
    <header><span>{slug === 'glare-hover' ? '✦ HOVER GLARE' : slug === 'neon-gradient-card' ? 'NEON SYSTEM' : 'MAGIC INTERFACE'}</span><b>{slug === 'glare-hover' ? '↗' : '✦'}</b></header>
    {slotContent.content ?? <>{mode === 'editorial' ? <div className="creative-card-editorial"><small>01 / FEATURE</small><h3>{component.content}</h3><ul><li>Visual system</li><li>Interaction model</li></ul></div> : mode === 'immersive' ? <div className="creative-card-immersive"><h3>{component.content}</h3><div><span><b>98%</b> fidelity</span><span><b>24ms</b> response</span></div></div> : <><h3>{component.content}</h3><p>{String(props.description)}</p></>}<footer><span><i />{mode === 'editorial' ? 'Case study' : mode === 'immersive' ? 'Live telemetry' : 'Live effect'}</span><strong>{slug === 'magic-card' ? '24.8K' : slug === 'neon-gradient-card' ? '98%' : 'Explore'}</strong></footer></>}
  </article>;

  if (family === 'device') {
    const phone = slug === 'iphone' || slug === 'android';
    return <div className={cx('creative-device', phone && 'phone', `device-${slug}`)}>
      <div className="creative-device-frame"><header>{phone ? <><i /><span /></> : <><i /><i /><i /><span>{mode === 'editorial' ? 'studio / case-01' : 'design.local'}</span></>}</header><main>{slotContent.content ?? <div className="creative-device-screen">{mode === 'editorial' && <aside>01<br />02<br />03</aside>}<small>{mode === 'immersive' ? 'LIVE PRODUCT SIGNAL' : 'AI DESIGN STUDIO'}</small><strong>{component.content}</strong>{mode === 'immersive' ? <div className="creative-device-metrics"><b>8.4K</b><span>active sessions</span></div> : <button>{mode === 'editorial' ? 'Read story' : 'Start creating'}</button>}<div><i /><i /><i /></div></div>}</main></div>
    </div>;
  }

  if (family === 'background') return <section className="creative-background">
    <div className="creative-pattern-layer">{slug.includes('grid') || slug.includes('pattern') ? dots(42) : dots(18)}</div>
    <div className="creative-background-copy"><small>{slug.replaceAll('-', ' ').toUpperCase()}</small><strong>{component.content}</strong><span>{mode === 'immersive' ? 'Live generative environment · 60 FPS' : mode === 'editorial' ? 'Pattern study / scale 01' : '可作为主视觉、板块或整页背景'}</span>{mode === 'editorial' && <b>→</b>}</div>
  </section>;

  if (family === 'text') {
    const words = component.content.split(/\s+/);
    return <div className="creative-text-stage" aria-label={component.content}>
      {slug === 'spinning-text' && <span className="creative-spinning-ring">DESIGN · BUILD · SHIP ·</span>}
      <h2>{words.map((word, index) => <span key={`${word}-${index}`} style={{ '--word-index': index } as React.CSSProperties}>{word}{index < words.length - 1 ? '\u00a0' : ''}</span>)}</h2>
      {(slug === 'highlighter' || slug === 'dia-text-reveal') && <i className="creative-text-stroke" />}
      <small>{mode === 'immersive' ? 'MOTION TYPE / LIVE' : slug.replaceAll('-', ' ')}</small>
      {mode === 'editorial' && <div className="creative-text-meta"><b>Type study 01</b><span>Editorial scale · 2026</span></div>}
    </div>;
  }

  if (family === 'progress') {
    const circular = slug.includes('circular');
    if (mode === 'editorial') return <div className="creative-progress segmented"><header><small>PROJECT INDEX</small><strong>{component.content}</strong><b>78 / 100</b></header><div>{Array.from({ length: 10 }, (_, index) => <i className={index < 8 ? 'filled' : ''} key={index} />)}</div><footer><span>Discover</span><span>Design</span><span>Launch</span></footer></div>;
    return circular || mode === 'immersive' ? <div className="creative-progress circular"><svg viewBox="0 0 120 120"><circle cx="60" cy="60" r="48" /><circle className="value" cx="60" cy="60" r="48" /></svg><strong>78<small>%</small></strong><span>{mode === 'immersive' ? 'LIVE COMPLETION' : component.content}</span>{mode === 'immersive' && <b>+12% this week</b>}</div>
      : <div className="creative-progress linear"><header><strong>{component.content}</strong><span>{value}%</span></header><div><i style={{ width: `${value}%` }} /></div><input aria-label="progress" type="range" min="0" max="100" value={value} onChange={(event) => preview && setValue(Number(event.target.value))} /></div>;
  }

  if (family === 'lens' && mode === 'editorial') return <div className="creative-lens-split"><div><small>OVERVIEW</small><strong>Brand<br />system</strong></div><div><small>DETAIL ×4</small><i /><i /><i /></div><span>{component.content}</span></div>;
  if (family === 'lens') return <div className="creative-lens" onPointerMove={(event) => {
    const rect = event.currentTarget.getBoundingClientRect();
    setLens({ x: ((event.clientX - rect.left) / rect.width) * 100, y: ((event.clientY - rect.top) / rect.height) * 100 });
  }}>
    <div className="creative-lens-image"><span>DESIGN<br />DETAIL</span></div>
    {slug === 'progressive-blur' ? <div className="creative-progressive-blur" /> : <div className="creative-lens-orb" style={{ left: `${lens.x}%`, top: `${lens.y}%` }}><i /></div>}
    <small>{component.content}</small>
  </div>;

  if (family === 'pointer') return <div className="creative-pointer-stage" onPointerMove={(event) => setPointerVariables(event)}>
    <div className="creative-pointer-grid">{dots(12)}</div><strong>{mode === 'editorial' ? 'Live collaboration' : component.content}</strong><span>{mode === 'immersive' ? 'Magnetic target acquired' : mode === 'editorial' ? 'Two designers in this section' : 'Move your pointer'}</span><i className="creative-custom-pointer">{slug === 'smooth-cursor' ? '◇' : '↖'}</i>{mode === 'editorial' && <><i className="creative-collab-pointer one">林</i><i className="creative-collab-pointer two">AI</i></>}{mode === 'immersive' && <b className="creative-pointer-target">◎</b>}
  </div>;

  if (family === 'effect') return <div className="creative-effect"><div className="creative-effect-field">{dots(slug === 'particles' ? 28 : 12)}</div><div><small>{mode === 'editorial' ? 'EFFECT STUDY / 01' : slug.toUpperCase()}</small><strong>{component.content}</strong>{mode === 'editorial' ? <div className="creative-effect-spec"><span>Density 24</span><span>Speed .8</span><span>Depth 3</span></div> : <span>{slug === 'meteors' ? '☄ ☄ ☄' : mode === 'immersive' ? 'LIVE ENVIRONMENT' : '✦ · ✦ · ✦'}</span>}</div></div>;

  if (family === 'media') return <div className={cx('creative-media', active && 'open')}>
    <button className="creative-media-cover" onClick={() => preview && setActive(true)}>{mode === 'editorial' && <i>FILM<br />NO. 01</i>}<span>▶</span><strong>{component.content}</strong><small>{mode === 'immersive' ? '4K · Spatial sound · 01:24' : slug === 'backlight' ? 'Adaptive ambient glow' : 'Watch the product film'}</small></button>
    {(active || showcase) && <div className="creative-media-dialog"><button onClick={() => setActive(false)}>×</button><div><i /><span>00:18 / 01:24</span></div><strong>Design in motion</strong></div>}
  </div>;

  if (family === 'comparison' && mode === 'editorial') return <div className="creative-diff-review"><header><span>design.tsx</span><b>6 changes</b></header>{['- fixed 1200px canvas','+ responsive constraints','- static component','+ editable slots','+ visual regression checks'].map((line) => <code className={line.startsWith('+') ? 'added' : 'removed'} key={line}>{line}</code>)}<footer>Reviewed by AI Designer ✓</footer></div>;
  if (family === 'comparison' && mode === 'immersive') return <div className="creative-diff-columns"><section><small>BEFORE</small><pre>{'fixed width\nmanual layout\nstatic state'}</pre></section><section><small>AFTER</small><pre>{'responsive\nauto layout\ninteractive'}</pre></section><i>→</i></div>;
  if (family === 'comparison') return <div className="creative-comparison">
    <div className="creative-compare-before"><small>BEFORE</small><pre>{'const page = oldLayout();\nship(page);'}</pre></div>
    <div className="creative-compare-after" style={{ width: `${value}%` }}><small>AFTER</small><pre>{'const page = design.withAI();\nship(page);'}</pre></div>
    <input aria-label="comparison" type="range" min="12" max="88" value={value} onChange={(event) => preview && setValue(Number(event.target.value))} /><i style={{ left: `${value}%` }} />
  </div>;

  if (family === 'copy') return <div className="creative-copy"><code>{component.content}</code><button onClick={async () => { if (!preview) return; await navigator.clipboard?.writeText(component.content).catch(() => undefined); setCopied(true); window.setTimeout(() => setCopied(false), 1400); }}>{copied ? '✓ 已复制' : '⎘ 复制'}</button></div>;

  if (family === 'marquee') return <div className="creative-marquee"><div>{[...items, ...items].map((item, index) => <span key={`${item}-${index}`}><i>{['✦', '◇', '◉'][index % 3]}</i>{item}</span>)}</div></div>;

  if (family === 'matrix' && mode === 'editorial') return <div className="creative-matrix-ledger"><header><span>GLYPH INDEX</span><b>08 / 24</b></header><div>{['A1','B7','C3','D9','E4','F2','G8','H5'].map((cell, index) => <i key={cell}><strong>{cell}</strong><small>{[82,64,97,48,76,53,91,68][index]}</small></i>)}</div></div>;
  if (family === 'matrix') return <div className="creative-matrix"><pre>{Array.from({ length: 8 }, (_, row) => Array.from({ length: 18 }, (_, column) => (row * 17 + column * 7) % 5 === 0 ? '◆' : String.fromCharCode(65 + ((row + column) % 26))).join(' ')).join('\n')}</pre><strong>{mode === 'immersive' ? 'LIVE DATA STREAM' : component.content}</strong>{mode === 'immersive' && <span>128 nodes connected</span>}</div>;

  if (family === 'globe') return <div className={cx('creative-globe', (slug === 'dotted-map' || mode === 'editorial') && 'map')}>
    {slug === 'dotted-map' || mode === 'editorial' ? <><svg viewBox="0 0 500 240"><path d="M42 118c28-58 92-84 151-63 29 10 52 2 79-10 67-30 141 4 176 61-33 12-60 28-91 40-55 20-93 15-141-2-61-22-108 4-174-26Z" /><path className="route" d="M92 126 Q230 22 405 128" /><circle cx="92" cy="126" r="6" /><circle cx="405" cy="128" r="6" /></svg><strong>{component.content}</strong><span>8 regions · 42 live nodes</span></> : <><div className="creative-globe-sphere"><i /><i /><i /><b /><span /></div><div className="creative-globe-copy"><small>{mode === 'immersive' ? 'LIVE DATA ORBIT' : 'GLOBAL NETWORK'}</small><strong>{component.content}</strong><span>{mode === 'immersive' ? '8,429 signals connected now' : 'Singapore · Paris · San Francisco'}</span>{mode === 'immersive' && <b><i /> Network healthy</b>}</div></>}
  </div>;

  if (family === 'button') return <button className={cx('creative-button', active && 'active')} onClick={() => preview && setActive(!active)}><i />{mode === 'editorial' && <b>01</b>}{slug === 'animated-subscribe-button' && active ? '✓ 已订阅' : component.content}<span>{mode === 'immersive' ? '✦' : '→'}</span></button>;

  if (family === 'social') return <article className="creative-social"><header><i>{mode === 'editorial' ? '01' : 'AI'}</i><div><strong>{mode === 'immersive' ? 'Live design dispatch' : 'AI Design Studio'}</strong><span>{mode === 'editorial' ? 'INTERVIEW / SEPTEMBER' : '@designwithai · 2m'}</span></div><b>{mode === 'editorial' ? '↗' : '···'}</b></header><p>{component.content}</p>{mode === 'immersive' && <blockquote>“The canvas finally feels alive.”</blockquote>}<footer>{mode === 'editorial' ? <><span>Read the story</span><span>6 min →</span></> : <><span>♡ 1.2K</span><span>↻ 248</span><span>◰ 98K</span></>}</footer></article>;

  if (family === 'bento') return <div className="creative-bento">{slotContent.content ?? <>{items.slice(0, 3).map((item, index) => <article key={item} className={`cell-${index + 1}`}><i>{['✦', '◎', '↗'][index]}</i><strong>{item}</strong><span>{index === 0 ? 'Build with AI' : index === 1 ? 'Live collaboration' : 'Publish anywhere'}</span></article>)}</>}</div>;

  if (family === 'number') return <div className="creative-number"><small>LIVE METRIC</small><strong>{component.content}</strong><span><i /> +18.4% this month</span></div>;

  if (family === 'list') return <div className="creative-list">{slotContent.content ?? items.map((item, index) => <button key={item} className={selected === index ? 'active' : ''} onClick={() => preview && setSelected(index)}><i>{mode === 'editorial' ? String(index + 1).padStart(2, '0') : ['✓', '✦', '↗'][index % 3]}</i><span><strong>{item}</strong><small>{mode === 'immersive' ? `${[98, 72, 64][index % 3]}% signal strength` : mode === 'editorial' ? ['Research note', 'Visual direction', 'Launch log'][index % 3] : `${index + 2} 分钟前 · 已同步`}</small></span><b>{mode === 'immersive' ? '●' : '›'}</b></button>)}</div>;

  if (family === 'beam') return <div className="creative-beam"><svg viewBox="0 0 500 240"><defs><linearGradient id={`beam-${component.id}`}><stop stopColor="#22d3ee" /><stop offset="1" stopColor="#a855f7" /></linearGradient></defs>{slug === 'light-rays' ? <>{[70, 150, 230, 310, 390].map((x) => <path key={x} className="ray" d={`M${x} -20 L${x + 76} 260`} />)}</> : <><path d="M60 120 C155 20 345 20 440 120" /><path d="M60 120 C155 220 345 220 440 120" /></>}<circle cx="60" cy="120" r="22" /><circle cx="250" cy="120" r="28" /><circle cx="440" cy="120" r="22" /></svg><strong>{component.content}</strong></div>;

  if (family === 'orbit') return <div className="creative-orbit"><div className="creative-orbit-core">✦</div><div className="orbit orbit-one"><i>R</i><i>V</i><i>A</i></div><div className="orbit orbit-two"><i>AI</i><i>UI</i></div><strong>{component.content}</strong></div>;

  if (family === 'dock') return <div className="creative-dock">{['↖', '▭', 'T', '✎', '◎', '⚙'].map((icon, index) => <button key={icon} onClick={() => preview && setSelected(index)} className={selected === index ? 'active' : ''}>{icon}<small>{['Select', 'Frame', 'Text', 'Draw', 'AI', 'Settings'][index]}</small></button>)}</div>;

  if (family === 'avatars') return <div className="creative-avatars"><div>{['林', '陈', 'AI', '周', '+8'].map((name, index) => <i key={name}><span>{name}</span>{mode === 'editorial' && index < 4 && <small>{['Lead','UI','Agent','Dev'][index]}</small>}</i>)}</div><span><b />{mode === 'immersive' ? '12 live cursors on canvas' : mode === 'editorial' ? 'Core design team / 04' : '12 位成员正在协作'}</span></div>;

  if (family === 'iconcloud') return <div className="creative-iconcloud"><div>{['Re', 'Vue', 'AI', 'Fg', 'TS', 'Nu', 'JS', '3D'].map((icon, index) => <i key={icon} style={{ '--icon-index': index } as React.CSSProperties}>{icon}{mode === 'editorial' && <small>{String(index + 1).padStart(2,'0')}</small>}</i>)}</div><strong>{mode === 'immersive' ? 'TECH ORBIT' : component.content}</strong></div>;

  if (family === 'reveal') return <button className={cx('creative-reveal', active && 'revealed')} onClick={() => preview && setActive(true)}><span>点击揭示</span><strong>{component.content}</strong><i /></button>;

  if (family === 'confetti') return <button className={cx('creative-confetti', active && 'celebrate')} onClick={() => { if (!preview) return; setActive(false); requestAnimationFrame(() => setActive(true)); }}><div>{dots(24)}</div><strong>{component.content}</strong><span>点击庆祝 ✦</span></button>;

  if (family === 'tree') return <div className="creative-tree">{slotContent.content ?? <><header><i />website-studio</header>{items.map((item, index) => <div key={item} style={{ paddingLeft: 18 + index % 2 * 18 }}><span>{index < 3 ? '▿' : '◇'}</span><strong>{item}</strong><small>{index === 0 ? '12' : ''}</small></div>)}</>}</div>;

  if (family === 'terminal') return <div className="creative-terminal"><header><i /><i /><i /><span>design-cli — zsh</span></header><main>{slotContent.content ?? <><p><b>$</b> npm run design</p><p><em>✓</em> analysing visual language</p><p><em>✓</em> composing responsive sections</p><p><span>█</span> {component.content}</p></>}</main></div>;

  if (family === 'image' && mode === 'editorial') return <div className="creative-image-editorial"><div className="creative-pixel-grid">{dots(64)}</div><aside><small>IMAGE STUDY / 01</small><strong>{component.content}</strong><span>Pixel reconstruction<br />64 source cells</span></aside></div>;
  if (family === 'image') return <div className="creative-image"><div className="creative-pixel-grid">{dots(64)}</div><strong>{mode === 'immersive' ? 'RESOLVING…' : component.content}</strong><span>{mode === 'immersive' ? '64% reconstructed' : 'Hover to resolve'}</span></div>;

  if (family === 'timeline') return <div className="creative-timeline">{items.map((item, index) => <div key={item}><i>{index + 1}</i><span><strong>{item}</strong><small>{['Discover', 'Design', 'Launch'][index]}</small></span></div>)}</div>;

  if (family === 'theme') return <button className={cx('creative-theme-toggle', active && 'dark')} onClick={() => preview && setActive(!active)}><i><span /></i><strong>{active ? '深色' : '明亮'}</strong></button>;

  if (family === 'chart') return <CreativeChart title={title} values={values} preview={preview} mode={mode} />;

  if (family === 'book') return <div className={cx('creative-book', active && 'open')} onClick={() => preview && setActive(!active)}><div className="book-cover"><small>SPELL UI</small><strong>{component.content}</strong><span>Volume 01</span></div><div className="book-pages"><p>Design systems for expressive products.</p></div></div>;

  if (family === 'badge') return <span className="creative-badge"><i />{component.content}<b>✓</b></span>;

  if (family === 'color') return <div className="creative-color"><div style={{ background: `hsl(${value * 3.6} 82% 58%)` }} /><input type="range" min="0" max="100" value={value} onChange={(event) => preview && setValue(Number(event.target.value))} /><code>hsl({Math.round(value * 3.6)} 82% 58%)</code></div>;

  if (family === 'kbd') return <div className="creative-kbd">{strings(props.keys, ['⌘', 'K']).map((key, index) => <span key={key}><kbd>{key}</kbd>{index === 0 && <i>+</i>}</span>)}<small>{component.content}</small></div>;

  if (family === 'input') return <label className={cx('creative-input', active && 'focused')}><span>{title}</span><div><input placeholder={component.content} onFocus={() => setActive(true)} onBlur={() => setActive(false)} /><i>{slug.includes('exploding') ? '✦' : '→'}</i></div><small>{active ? '输入中…' : '可直接编辑与交互'}</small></label>;

  if (family === 'spinner') return <div className="creative-spinner"><i /><i /><i /><span>{component.content}</span></div>;

  if (family === 'checkbox') return <button role="checkbox" aria-checked={checked} className={cx('creative-checkbox', checked && 'checked')} onClick={() => preview && setChecked(!checked)}><i>{checked ? '✓' : ''}</i><span><strong>{component.content}</strong><small>{checked ? '已完成' : '点击切换状态'}</small></span></button>;

  if (family === 'qr') return <div className="creative-qr"><div>{dots(49)}</div><strong>{component.content}</strong><span>Scan to open</span></div>;

  if (family === 'upload') return <div className={cx('creative-upload', uploadedFiles.length > 0 && 'has-files')}>
    <label><input type="file" multiple={Boolean(props.multiple)} accept={String(props.accept ?? '')} onChange={(event) => setUploadedFiles(Array.from(event.target.files ?? []).map((file) => file.name))} /><i>↑</i><strong>{mode === 'editorial' ? '选择项目素材' : '拖放文件到这里'}</strong><span>{mode === 'immersive' ? 'Images, PDF and video · encrypted upload' : `或点击选择 · 单个最大 ${Number(props.maxSizeMb ?? 10)} MB`}</span></label>
    <div className="creative-upload-files">{uploadedFiles.length ? uploadedFiles.map((file, index) => <div key={`${file}-${index}`}><i>{file.split('.').at(-1)?.toUpperCase() ?? 'FILE'}</i><span><strong>{file}</strong><small>准备就绪 · {index + 1}.8 MB</small></span><b>✓</b></div>) : <small>尚未选择文件</small>}</div>
  </div>;

  if (family === 'tabs') return <div className="creative-tabs"><nav>{items.map((item, index) => <button className={selected === index ? 'active' : ''} key={item} onClick={() => preview && setSelected(index)}><i>{mode === 'editorial' ? String(index + 1).padStart(2, '0') : ''}</i>{item}</button>)}</nav><section>{mode === 'immersive' ? <><small>LIVE WORKSPACE</small><strong>{items[selected]} system</strong><div><i /><i /><i /></div></> : mode === 'editorial' ? <><small>CHAPTER {String(selected + 1).padStart(2, '0')}</small><strong>{items[selected]}</strong><p>Explore the structure, rhythm and content of this section.</p></> : <><strong>{items[selected]}</strong><p>切换标签查看不同的设计内容。</p><button>Open details →</button></>}</section></div>;

  if (family === 'modal') return <div className="creative-modal-stage"><button onClick={() => preview && setActive(true)}>打开弹窗</button>{(active || showcase) && <section className="creative-modal-card"><header><span>{mode === 'editorial' ? 'REQUEST / 01' : 'NEW PROJECT'}</span><button onClick={() => setActive(false)}>×</button></header><strong>{component.content}</strong><p>{mode === 'immersive' ? 'Create a focused, cinematic product experience.' : '在不离开当前画布的情况下编辑详细信息。'}</p><label>项目名称<input defaultValue="Inspira launch" /></label><footer><button>取消</button><button>创建项目</button></footer></section>}</div>;

  if (family === 'gallery') return <div className="creative-gallery"><div className="creative-gallery-stage">{items.map((item, index) => <button key={item} className={selected === index ? 'active' : ''} onClick={() => preview && setSelected(index)} style={{ '--gallery-index': index } as React.CSSProperties}><span>{String(index + 1).padStart(2, '0')}</span><strong>{item}</strong></button>)}</div><footer><span>{selected + 1} / {items.length}</span><strong>{items[selected] ?? items[0]}</strong><div><button onClick={() => setSelected((selected - 1 + items.length) % items.length)}>←</button><button onClick={() => setSelected((selected + 1) % items.length)}>→</button></div></footer></div>;

  if (family === 'tooltip') return <div className="creative-tooltip-stage"><button onMouseEnter={() => preview && setActive(true)} onMouseLeave={() => setActive(false)} onClick={() => preview && setActive(!active)}>{component.content}</button>{(active || showcase) && <aside><small>{mode === 'editorial' ? 'NOTE 01' : 'QUICK TIP'}</small><strong>{mode === 'immersive' ? 'Live component details' : '可以继续编辑这个信息浮层'}</strong><span>尺寸、内容和交互都会保存。</span></aside>}</div>;

  if (family === 'loader') return <div className="creative-loader"><header><small>{mode === 'editorial' ? 'PROGRESS LOG' : 'AI DESIGN PIPELINE'}</small><strong>{component.content}</strong><b>{Math.round(((selected + 1) / items.length) * 100)}%</b></header><div>{items.map((item, index) => <button key={item} className={index < selected ? 'done' : index === selected ? 'active' : ''} onClick={() => preview && setSelected(index)}><i>{index < selected ? '✓' : index + 1}</i><span><strong>{item}</strong><small>{index < selected ? '已完成' : index === selected ? '正在处理…' : '等待中'}</small></span></button>)}</div></div>;

  if (family === 'calendar') return <div className="creative-calendar"><header><button>‹</button><strong>September 2026</strong><button>›</button></header><div className="creative-calendar-week">{['S','M','T','W','T','F','S'].map((day, index) => <small key={`${day}-${index}`}>{day}</small>)}</div><div className="creative-calendar-days">{Array.from({ length: 35 }, (_, index) => { const day = index - 1; return <button key={index} className={day === Number(props.selectedDay ?? 18) ? 'selected' : day < 1 || day > 30 ? 'muted' : ''}>{day > 0 && day <= 30 ? day : ''}{[8,18,24].includes(day) && <i />}</button>; })}</div><footer>{strings(props.events, ['Design review']).map((event, index) => <span key={event}><i style={{ background: index ? '#22d3ee' : '#8b5cf6' }} />{event}<b>{index ? '14:30' : '10:00'}</b></span>)}</footer></div>;

  if (family === 'testimonial') return <article className="creative-testimonial"><header><div><i>{['林', '陈', 'AI'][selected % 3]}</i><span><strong>{['Lin Chen', 'Maya Zhou', 'AI Studio'][selected % 3]}</strong><small>{mode === 'editorial' ? 'DESIGN LEADER / 2026' : 'Product design team'}</small></span></div><b>“</b></header><blockquote>{items[selected] ?? items[0]}</blockquote><footer><span>★★★★★</span><div>{items.map((_, index) => <button aria-label={`查看评价 ${index + 1}`} className={selected === index ? 'active' : ''} key={index} onClick={() => preview && setSelected(index)} />)}</div></footer></article>;

  return <div className="creative-fallback"><span>{component.library?.name ?? 'Creative UI'}</span><strong>{component.library?.component}</strong></div>;
}

function setPointerVariables(event: PointerEvent<HTMLElement>) {
  const rect = event.currentTarget.getBoundingClientRect();
  event.currentTarget.style.setProperty('--pointer-x', `${event.clientX - rect.left}px`);
  event.currentTarget.style.setProperty('--pointer-y', `${event.clientY - rect.top}px`);
}

function CreativeChart({ title, values, preview, mode }: { title: string; values: number[]; preview: boolean; mode: string }) {
  const [hovered, setHovered] = useState<number | undefined>();
  const chart = useRef<SVGSVGElement | null>(null);
  const points = useMemo(() => values.map((value, index) => ({ x: 24 + index * (252 / Math.max(1, values.length - 1)), y: 126 - value })), [values]);
  const path = points.map((point, index) => `${index ? 'L' : 'M'}${point.x} ${point.y}`).join(' ');
  return <div className="creative-chart"><header><span><small>{mode === 'editorial' ? 'QUARTERLY INDEX' : mode === 'immersive' ? 'LIVE ANALYTICS' : 'ANALYTICS'}</small><strong>{title}</strong></span><b>{mode === 'editorial' ? '2026 / Q3' : '+24.8%'}</b></header><svg ref={chart} viewBox="0 0 300 150" onPointerMove={(event) => {
    if (!preview) return;
    const rect = chart.current?.getBoundingClientRect();
    if (!rect) return;
    const x = ((event.clientX - rect.left) / rect.width) * 300;
    setHovered(points.reduce((best, point, index) => Math.abs(point.x - x) < Math.abs(points[best].x - x) ? index : best, 0));
  }} onPointerLeave={() => setHovered(undefined)}><path className="grid" d="M20 30H285M20 65H285M20 100H285M20 135H285" />{mode === 'editorial' ? values.map((item, index) => <rect key={index} className="bar" x={20 + index * 44} y={138 - item} width="23" height={item} />) : <><path className={cx('area', mode === 'signature' && 'muted')} d={`${path} L${points.at(-1)?.x} 138 L${points[0]?.x} 138Z`} /><path className="line" d={path} />{points.map((point, index) => <circle key={index} className={hovered === index ? 'active' : ''} cx={point.x} cy={point.y} r={hovered === index ? 5 : 3} />)}</>}{hovered !== undefined && mode !== 'editorial' && <g className="tooltip" transform={`translate(${Math.min(240, Math.max(8, points[hovered].x - 26))} ${Math.max(4, points[hovered].y - 34)})`}><rect width="58" height="24" rx="7" /><text x="29" y="16">{values[hovered]}K</text></g>}</svg>{mode === 'editorial' && <footer>{values.slice(0, 6).map((_, index) => <span key={index}>M{index + 1}</span>)}</footer>}</div>;
}
