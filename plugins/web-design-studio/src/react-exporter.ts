import { breakpointFor, componentsForPage, resolveComponent } from './editor-model.js';
import { pagesForDocument, tokensForDocument, type WebDesignDevice, type WebDesignDocument } from './schema.js';

export interface ExportedReactFile {
  filename: string;
  content: string;
}

export function exportReactComponent(document: WebDesignDocument, device: WebDesignDevice = 'desktop'): ExportedReactFile {
  const viewport = breakpointFor(document, device);
  const tokens = tokensForDocument(document);
  const pages = pagesForDocument(document).map((page) => ({
    ...page,
    components: componentsForPage(document, page.id)
      .sort((left, right) => left.zIndex - right.zIndex)
      .map((component) => {
        const frame = resolveComponent(component, device);
        return {
          id: component.id,
          type: component.type,
          name: component.name,
          content: component.content,
          library: component.library,
          interaction: component.interaction,
          hidden: frame.hidden,
          style: {
            position: 'absolute', left: frame.x, top: frame.y, width: frame.width, height: frame.height,
            zIndex: component.zIndex, display: 'flex', overflow: 'hidden', whiteSpace: 'pre-wrap',
            background: frame.style.background, color: frame.style.color, borderColor: frame.style.borderColor,
            borderWidth: frame.style.borderWidth, borderStyle: frame.style.borderWidth ? 'solid' : undefined,
            borderRadius: frame.style.borderRadius, fontSize: frame.style.fontSize, fontWeight: frame.style.fontWeight,
            textAlign: frame.style.textAlign, opacity: frame.style.opacity, boxShadow: frame.style.shadow
          }
        };
      })
  }));
  const payload = JSON.stringify({ pages, viewport, background: document.viewport.background, tokens }).replace(/</g, '\\u003c');
  const content = `import React, { useEffect, useState } from 'react';
import * as Antd from 'antd';

const design = ${payload};

function DesignComponent({ component, activate }) {
  if (component.hidden) return null;
  const className = \`component type-\${component.type}\${component.library?.name === 'antd' ? ' library-antd' : ''}\`;
  const props = { className, style: { ...component.style, cursor: component.interaction ? 'pointer' : component.style.cursor }, onClick: () => activate(component) };
  if (component.library?.name === 'antd') return <AntDesignComponent component={component} outerProps={props} />;
  if (component.type === 'image') {
    return component.content
      ? <img {...props} src={component.content} alt={component.name} />
      : <div {...props}>图片</div>;
  }
  if (component.type === 'avatar') {
    return /^(data:image\\/|https?:\\/\\/)/.test(component.content)
      ? <img {...props} src={component.content} alt={component.name} />
      : <div {...props}>{component.content || 'AI'}</div>;
  }
  if (component.type === 'video') return component.content ? <video {...props} src={component.content} controls /> : <div {...props}>▶ 视频</div>;
  if (component.type === 'input') return <input {...props} placeholder={component.content} />;
  if (component.type === 'textarea') return <textarea {...props} placeholder={component.content} />;
  if (component.type === 'select') return <select {...props}>{component.content.split('\\n').filter(Boolean).map((option) => <option key={option}>{option}</option>)}</select>;
  if (component.type === 'checkbox') return <label {...props}><input type="checkbox" defaultChecked />{component.content}</label>;
  if (component.type === 'switch') return <label {...props}><input type="checkbox" role="switch" defaultChecked />{component.content}</label>;
  if (component.type === 'button') return <button {...props}>{component.content}</button>;
  if (component.type === 'list') return <ul {...props}>{component.content.split('\\n').filter(Boolean).map((item) => <li key={item}>{item}</li>)}</ul>;
  if (component.type === 'table') return <table {...props}><tbody>{component.content.split('\\n').filter(Boolean).map((row, rowIndex) => <tr key={rowIndex}>{row.split('|').map((cell, cellIndex) => <td key={cellIndex}>{cell}</td>)}</tr>)}</tbody></table>;
  return <div {...props}>{component.type === 'section' || component.type === 'divider' ? null : component.content}</div>;
}

function AntDesignComponent({ component, outerProps }) {
  const name = component.library.component;
  const p = component.library.props || {};
  let content;
  if (name === 'Typography') content = <Antd.Typography.Title level={p.level || 3} style={{ margin: 0 }}>{component.content}</Antd.Typography.Title>;
  else if (name === 'Grid') content = <Antd.Row gutter={p.gutter || 8} style={{ width: '100%' }}>{[1, 2, 3].map((value) => <Antd.Col key={value} span={8}><div className="antd-grid-cell">{value}</div></Antd.Col>)}</Antd.Row>;
  else if (name === 'Layout') content = <Antd.Layout style={{ width: '100%', height: '100%' }}><Antd.Layout.Header>Header</Antd.Layout.Header><Antd.Layout><Antd.Layout.Sider width="28%">Sider</Antd.Layout.Sider><Antd.Layout.Content>Content</Antd.Layout.Content></Antd.Layout></Antd.Layout>;
  else if (name === 'Splitter') content = <Antd.Splitter {...p} style={{ width: '100%', height: '100%' }}><Antd.Splitter.Panel>面板一</Antd.Splitter.Panel><Antd.Splitter.Panel>面板二</Antd.Splitter.Panel></Antd.Splitter>;
  else if (name === 'Form') content = <Antd.Form {...p} style={{ width: '100%' }}><Antd.Form.Item label="名称"><Antd.Input placeholder="请输入名称" /></Antd.Form.Item><Antd.Button type="primary">提交</Antd.Button></Antd.Form>;
  else if (name === 'List') content = <Antd.List {...p} renderItem={(entry) => <Antd.List.Item>{entry}</Antd.List.Item>} />;
  else if (name === 'Masonry') {
    const items = [72, 110, 88, 128, 96, 70].map((height, index) => ({ key: index, data: { height, label: index + 1 } }));
    content = <Antd.Masonry {...p} items={items} itemRender={({ data }) => <div className="antd-masonry-item" style={{ height: data.height }}>{data.label}</div>} />;
  } else if (name === 'Carousel') content = <Antd.Carousel {...p}>{['产品设计', 'AI 协作', '代码交付'].map((text) => <div key={text}><div className="antd-carousel-slide">{text}</div></div>)}</Antd.Carousel>;
  else if (name === 'Icon') content = <span style={{ color: '#1677ff', fontSize: 32 }}>✦</span>;
  else {
    const Candidate = Antd[name];
    if (!Candidate) content = <Antd.Tag color="blue">Ant Design · {name}</Antd.Tag>;
    else {
      const childrenless = new Set(['Anchor', 'AutoComplete', 'Calendar', 'Cascader', 'ColorPicker', 'DatePicker', 'Descriptions', 'Empty', 'Image', 'Input', 'InputNumber', 'Menu', 'Mentions', 'Pagination', 'Progress', 'QRCode', 'Radio', 'Rate', 'Result', 'Segmented', 'Select', 'Skeleton', 'Slider', 'Spin', 'Statistic', 'Steps', 'Switch', 'Table', 'Tabs', 'TimePicker', 'Timeline', 'Transfer', 'Tree', 'TreeSelect']);
      content = childrenless.has(name) ? <Candidate {...p} /> : <Candidate {...p}>{component.content}</Candidate>;
    }
  }
  return <div {...outerProps}><div className="antd-export-content">{content}</div></div>;
}

export default function WebDesignApp() {
  const [route, setRoute] = useState(() => window.location.pathname || '/');
  useEffect(() => {
    const update = () => setRoute(window.location.pathname || '/');
    window.addEventListener('popstate', update);
    return () => window.removeEventListener('popstate', update);
  }, []);
  const page = design.pages.find((candidate) => candidate.slug === route) ?? design.pages[0];
  const navigate = (slug) => {
    window.history.pushState({}, '', slug);
    setRoute(slug);
  };
  const activate = (component) => {
    if (!component.interaction) return;
    if (component.interaction.type === 'page') {
      const target = design.pages.find((candidate) => candidate.id === component.interaction.target);
      if (target) navigate(target.slug);
    } else {
      window.open(component.interaction.target, '_blank', 'noopener,noreferrer');
    }
  };
  const variables = {
    '--color-primary': design.tokens.colors.primary,
    '--color-accent': design.tokens.colors.accent,
    '--color-surface': design.tokens.colors.surface,
    '--color-text': design.tokens.colors.text,
    '--color-muted': design.tokens.colors.muted,
    '--radius-small': design.tokens.radii.small + 'px',
    '--radius-medium': design.tokens.radii.medium + 'px',
    '--radius-large': design.tokens.radii.large + 'px'
  };
  return <div style={{ ...variables, minHeight: '100vh', background: '#e5e7eb', fontFamily: design.tokens.typography.fontFamily }}>
    {design.pages.length > 1 && <nav className="route-nav">{design.pages.map((item) => <button key={item.id} onClick={() => navigate(item.slug)}>{item.name}</button>)}</nav>}
    <main className="page" style={{ width: design.viewport.width, height: design.viewport.height, background: design.background }}>
      {page.components.map((component) => <DesignComponent key={component.id} component={component} activate={activate} />)}
    </main>
    <style>{\`
      * { box-sizing: border-box; }
      body { margin: 0; }
      .page { position: relative; margin: 0 auto; overflow: hidden; }
      .type-text, .type-heading, .type-card, .type-link, .type-logo { align-items: center; padding: 12px 16px; line-height: 1.22; }
      .type-button { align-items: center; justify-content: center; padding: 0 16px; }
      .type-input, .type-textarea, .type-select { padding: 0 14px; }
      .type-textarea { padding-top: 13px; resize: none; }
      .type-image, .type-avatar, .type-video { object-fit: cover; }
      .type-icon, .type-badge, .type-avatar { align-items: center; justify-content: center; }
      .type-checkbox, .type-switch { align-items: center; gap: 8px; }
      .type-divider { height: 1px !important; border-width: 0 !important; border-top: 1px solid; }
      .type-list { margin: 0; padding: 13px 20px 13px 38px; flex-direction: column; gap: 8px; }
      .type-table { border-collapse: collapse; display: table; }
      .type-table td { padding: 10px 12px; border-bottom: 1px solid rgba(128,128,145,.18); }
      .library-antd { align-items: flex-start; justify-content: flex-start; padding: 0; overflow: visible; }
      .antd-export-content { width: 100%; height: 100%; display: flex; align-items: center; overflow: visible; }
      .antd-grid-cell { display: grid; place-items: center; min-height: 54px; border-radius: 7px; background: #eaf3ff; color: #0066cc; }
      .antd-masonry-item { display: grid; place-items: center; border-radius: 8px; background: #eaf3ff; color: #0066cc; }
      .antd-carousel-slide { height: 164px; display: grid; place-items: center; border-radius: 10px; background: linear-gradient(145deg,#1677ff,#64d2ff); color: white; }
      .route-nav { display: flex; justify-content: center; gap: 8px; padding: 12px; }
    \`}</style>
  </div>;
}
`;
  return { filename: 'WebDesignApp.jsx', content };
}
