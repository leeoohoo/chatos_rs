import { useEffect, useRef, useState, type ReactNode } from 'react';
import {
  Affix, Alert, Anchor, App as AntApp, AutoComplete, Avatar, Badge, BorderBeam, Breadcrumb, Button, Calendar, Card, Carousel, ConfigProvider,
  Cascader, Checkbox, Collapse, ColorPicker, DatePicker, Descriptions, Divider, Drawer, Dropdown,
  Empty, Flex, FloatButton, Form, Image as AntImage, Input, InputNumber, Layout, List, Listy, Masonry, Menu,
  Mentions, Modal, Pagination, Popconfirm, Popover, Progress, QRCode, Radio, Rate, Result, Row, Col,
  Segmented, Select, Skeleton, Slider, Space, Spin, Splitter, Statistic, Steps, Switch, Table, Tabs,
  Tag, TimePicker, Timeline, Tooltip, Tour, Transfer, Tree, TreeSelect, Typography, Upload, Watermark,
  message, notification
} from 'antd';
import { AntDesignOutlined, UploadOutlined } from '@ant-design/icons';
import type { WebDesignComponent, WebDesignTokens } from '../../src/schema';
import { componentStyleToCss, designStyleScopeProps } from './component-style';

type AnyProps = Record<string, any>;

export function AntdCanvasComponent({ component, preview, showcase = false, tokens, slotContent = {} }: { component: WebDesignComponent; preview: boolean; showcase?: boolean; tokens?: WebDesignTokens; slotContent?: Record<string, ReactNode> }) {
  const scope = designStyleScopeProps(component.style);
  return <ConfigProvider theme={{
    token: tokens ? {
      colorPrimary: tokens.colors.primary,
      colorInfo: tokens.colors.primary,
      colorSuccess: tokens.colors.accent,
      colorBgContainer: tokens.colors.surface,
      colorText: tokens.colors.text,
      colorTextSecondary: tokens.colors.muted,
      borderRadius: tokens.radii.medium,
      fontFamily: tokens.typography.fontFamily,
      fontSize: tokens.typography.baseFontSize
    } : undefined
  }} getPopupContainer={(triggerNode) => (triggerNode?.closest('[data-library-portal-host], .design-canvas') as HTMLElement | null) ?? document.body}><div {...scope}><AntdCanvasRenderer component={component} preview={preview} showcase={showcase} slotContent={slotContent} /></div></ConfigProvider>;
}

function AntdCanvasRenderer({ component, preview, showcase, slotContent }: { component: WebDesignComponent; preview: boolean; showcase: boolean; slotContent: Record<string, ReactNode> }) {
  const [modalOpen, setModalOpen] = useState(false);
  const [drawerOpen, setDrawerOpen] = useState(false);
  const [tourOpen, setTourOpen] = useState(false);
  const [listyCount, setListyCount] = useState(20);
  const [draggingListyId, setDraggingListyId] = useState<number>();
  const [listyOrder, setListyOrder] = useState(() => Array.from({ length: 20 }, (_, index) => index));
  const modalTrigger = useRef<HTMLButtonElement>(null);
  const drawerTrigger = useRef<HTMLButtonElement>(null);
  const [transferTargetKeys, setTransferTargetKeys] = useState<string[]>(() => {
    const targetKeys = component.library?.props.targetKeys;
    return Array.isArray(targetKeys) ? targetKeys.map(String) : [];
  });
  const tourTarget = useRef<HTMLButtonElement>(null);
  const listyRef = useRef<any>(null);
  const initialTabItems = Array.isArray(component.library?.props.items) ? component.library.props.items as AnyProps[] : [];
  const [editableTabItems, setEditableTabItems] = useState<AnyProps[]>(() => initialTabItems.map((entry) => ({ ...entry })));
  const [messageApi, messageContext] = message.useMessage();
  const [notificationApi, notificationContext] = notification.useNotification();
  const binding = component.library;
  if (!binding || binding.name !== 'antd') return null;
  const p = binding.props as AnyProps;
  const name = binding.component;
  const fill = { width: '100%', height: '100%' };
  const completeVisualStyle = componentStyleToCss(component.style);
  const {
    opacity: _opacity, filter: _filter, backdropFilter: _backdropFilter, WebkitBackdropFilter: _webkitBackdropFilter,
    transform: _transform, overflow: _overflow, objectFit: _objectFit, objectPosition: _objectPosition,
    mixBlendMode: _mixBlendMode, ...visualStyle
  } = completeVisualStyle;
  const textStyle = {
    ...visualStyle,
    background: undefined,
    borderColor: undefined,
    borderWidth: undefined,
    borderStyle: undefined,
    borderRadius: undefined,
    boxShadow: undefined,
    padding: undefined,
    filter: undefined,
    backdropFilter: undefined,
    transform: undefined,
    whiteSpace: 'pre-line' as const
  };
  const designCanvasFor = (trigger: HTMLButtonElement | null): HTMLElement => (trigger?.closest('[data-library-portal-host], .design-canvas') as HTMLElement | null) ?? document.body;
  useEffect(() => {
    if (!showcase) return;
    if (name === 'Modal') setModalOpen(true);
    if (name === 'Drawer') setDrawerOpen(true);
    if (name === 'Tour') setTourOpen(true);
  }, [showcase, name]);

  switch (name) {
    case 'Button': {
      const { gradient, ...buttonProps } = p;
      return <Button {...buttonProps} style={{ ...fill, ...(p.style ?? {}), ...(gradient && !component.style.background ? { border: 0, color: '#fff', background: 'linear-gradient(135deg,#1677ff,#722ed1)' } : {}), ...visualStyle }}>{component.content}</Button>;
    }
    case 'FloatButton': return <div style={{ ...fill, position: 'relative' }}><FloatButton {...p} style={{ position: 'absolute', right: 6, bottom: 6 }} /></div>;
    case 'Icon': return <AntDesignOutlined style={{ color: p.color ?? '#1677ff', fontSize: p.size ?? 32 }} />;
    case 'Typography': {
      if (binding.variant === 'paragraph') return <Typography.Paragraph style={{ margin: 0, ...textStyle }}>{component.content}</Typography.Paragraph>;
      if (binding.variant === 'text') return <Typography.Text style={textStyle}>{component.content}</Typography.Text>;
      return <Typography.Title level={p.level ?? 3} style={{ margin: 0, ...textStyle }}>{component.content}</Typography.Title>;
    }
    case 'Divider': return <Divider {...p}>{component.content}</Divider>;
    case 'Flex': return <Flex {...p} style={fill}>{slotContent.content ?? <><Button>按钮一</Button><Button type="primary">按钮二</Button><Tag color="blue">标签</Tag></>}</Flex>;
    case 'Grid': {
      const columns = Math.max(2, Number(p.columns ?? 3));
      if (slotContent.content) return <div style={fill}>{slotContent.content}</div>;
      return <Row gutter={p.gutter ?? 8} style={{ ...fill, width: '100%' }}>{Array.from({ length: columns }, (_, index) => index + 1).map((value) => <Col key={value} span={24 / columns}><div className="antd-grid-cell">{value}</div></Col>)}</Row>;
    }
    case 'Layout': {
      if (binding.variant === 'top') return <Layout style={fill}><Layout.Header className="antd-layout-header">{slotContent.header ?? 'Header'}</Layout.Header><Layout.Content className="antd-layout-content">{slotContent.content ?? 'Content'}</Layout.Content></Layout>;
      if (binding.variant === 'right-sidebar') return <Layout style={fill}><Layout.Header className="antd-layout-header">{slotContent.header ?? 'Header'}</Layout.Header><Layout><Layout.Content className="antd-layout-content">{slotContent.content ?? 'Content'}</Layout.Content><Layout.Sider width="28%" className="antd-layout-sider">{slotContent.sider ?? 'Sider'}</Layout.Sider></Layout></Layout>;
      return <Layout style={fill}><Layout.Header className="antd-layout-header">{slotContent.header ?? 'Header'}</Layout.Header><Layout><Layout.Sider width="28%" className="antd-layout-sider">{slotContent.sider ?? 'Sider'}</Layout.Sider><Layout.Content className="antd-layout-content">{slotContent.content ?? 'Content'}</Layout.Content></Layout></Layout>;
    }
    case 'Masonry': {
      const items = [72, 110, 88, 128, 96, 70].map((height, index) => ({ key: index, data: { height, label: index + 1 } }));
      return <Masonry {...p} items={items} itemRender={({ data }: AnyProps) => <div className="antd-masonry-item" style={{ height: data.height }}>{data.label}</div>} />;
    }
    case 'Space': return <Space {...p}>{slotContent.content ?? <><Button>取消</Button><Button type="primary">确定</Button><Tag>更多</Tag></>}</Space>;
    case 'Splitter': {
      const collapsible = Boolean(p.collapsible);
      return <Splitter {...p} collapsible={collapsible ? { motion: true } : undefined} style={fill}><Splitter.Panel collapsible={collapsible ? { end: true } : false}>{slotContent['panel-1'] ?? <div className="antd-split-panel">面板一</div>}</Splitter.Panel><Splitter.Panel collapsible={collapsible ? { start: true } : false}>{slotContent['panel-2'] ?? <div className="antd-split-panel">面板二</div>}</Splitter.Panel></Splitter>;
    }
    case 'Anchor': return <Anchor {...p} />;
    case 'Breadcrumb': return <Breadcrumb {...p} />;
    case 'Dropdown': return <Dropdown {...p} open={showcase ? true : undefined}><Button>{component.content}⌄</Button></Dropdown>;
    case 'Menu': return <Menu {...p} style={{ width: '100%' }} />;
    case 'Pagination': return <Pagination {...p} />;
    case 'Steps': return <Steps {...p} size="small" />;
    case 'AutoComplete': return <AutoComplete {...p} open={showcase ? true : undefined} style={{ width: '100%' }} placeholder={component.content} />;
    case 'Cascader': return <Cascader {...p} open={showcase ? true : undefined} style={{ width: '100%' }} placeholder={component.content} />;
    case 'Checkbox': return <Checkbox {...p}>{component.content}</Checkbox>;
    case 'ColorPicker': return <ColorPicker {...p} />;
    case 'DatePicker': {
      const { pickerMode, ...dateProps } = p;
      return pickerMode === 'range' ? <DatePicker.RangePicker {...dateProps} open={showcase ? true : undefined} style={{ width: '100%' }} /> : <DatePicker {...dateProps} open={showcase ? true : undefined} style={{ width: '100%' }} />;
    }
    case 'Form': {
      const { formTemplate, ...formProps } = p;
      const fallback = formTemplate === 'login' ? <><Form.Item name="account" label={p.layout === 'inline' ? undefined : '账号'} style={{ marginBottom: 10 }}><Input placeholder="邮箱或手机号" /></Form.Item><Form.Item name="password" label={p.layout === 'inline' ? undefined : '密码'} style={{ marginBottom: 10 }}><Input.Password placeholder="请输入密码" /></Form.Item><Button type="primary">登录</Button></>
        : formTemplate === 'registration' ? <><Form.Item label="姓名" style={{ marginBottom: 10 }}><Input placeholder="请输入姓名" /></Form.Item><Form.Item label="邮箱" style={{ marginBottom: 10 }}><Input placeholder="name@example.com" /></Form.Item><Form.Item label="密码" style={{ marginBottom: 10 }}><Input.Password placeholder="至少 8 位字符" /></Form.Item><Form.Item style={{ marginBottom: 10 }}><Checkbox>同意服务条款</Checkbox></Form.Item><Button type="primary" block>创建账号</Button></>
          : <><Form.Item label="名称" style={{ marginBottom: 10 }}><Input placeholder="请输入名称" /></Form.Item><Form.Item label="邮箱" style={{ marginBottom: 10 }}><Input placeholder="name@example.com" /></Form.Item><Button type="primary">提交</Button></>;
      return <Form {...formProps} style={{ width: '100%', height: '100%' }}>{slotContent.content ?? fallback}</Form>;
    }
    case 'Input': {
      if (binding.variant === 'otp') return <Input.OTP {...p} />;
      if (binding.variant === 'search') return <Input.Search {...p} placeholder={component.content} />;
      if (binding.variant === 'password') return <Input.Password {...p} placeholder={component.content} />;
      if (binding.variant === 'textarea') return <Input.TextArea {...p} placeholder={component.content} style={{ height: '100%' }} />;
      const { prefixText, suffixText, ...inputProps } = p;
      return <Input {...inputProps} prefix={prefixText} suffix={suffixText} placeholder={component.content} />;
    }
    case 'InputNumber': return <InputNumber {...p} style={{ width: '100%' }} />;
    case 'Mentions': return <Mentions {...p} placeholder={component.content} style={{ height: '100%' }} />;
    case 'Radio': return <Radio.Group {...p} />;
    case 'Rate': return <Rate {...p} />;
    case 'Select': {
      const { optionGroups, ...selectProps } = p;
      return <Select {...selectProps} open={showcase ? true : undefined} options={binding.variant === 'grouped' ? optionGroups : p.options} style={{ width: '100%' }} placeholder={component.content} />;
    }
    case 'Slider': return <Slider {...p} style={{ width: '100%' }} />;
    case 'Switch': return <Space><Switch {...p} /><span>{component.content}</span></Space>;
    case 'TimePicker': return <TimePicker {...p} style={{ width: '100%' }} />;
    case 'Transfer': return <Transfer {...p} targetKeys={transferTargetKeys} onChange={(keys) => setTransferTargetKeys(keys.map(String))} render={(item: AnyProps) => item.title} listStyle={{ width: 190, height: 170 }} />;
    case 'TreeSelect': return <TreeSelect {...p} open={showcase ? true : undefined} style={{ width: '100%' }} placeholder={component.content} />;
    case 'Upload': return binding.variant === 'dragger'
      ? <Upload.Dragger {...p} style={{ height: '100%' }}><p className="antd-upload-drag-icon"><UploadOutlined /></p><p>点击或拖拽文件到这里上传</p><small>支持单个或批量文件</small></Upload.Dragger>
      : binding.variant === 'picture'
      ? <Upload {...p}><button className="antd-picture-upload"><UploadOutlined /><span>上传图片</span></button></Upload>
      : <Upload {...p}><Button icon={<UploadOutlined />}>{component.content}</Button></Upload>;
    case 'Avatar': return <Avatar {...p}>{component.content}</Avatar>;
    case 'Badge': return p.status ? <Badge {...p} /> : <Badge {...p}><Avatar shape="square" icon={<AntDesignOutlined />} /></Badge>;
    case 'Calendar': return <Calendar {...p} style={{ width: '100%', height: '100%', overflow: 'hidden' }} />;
    case 'Card': {
      const { showcase = 'basic', ...cardProps } = p;
      const cardStyle = { ...fill, ...(p.style ?? {}), ...visualStyle };
      const bodyStyle = { height: '100%', ...(p.styles?.body ?? {}), ...textStyle };
      const editableContent = slotContent.content ?? component.content;
      if (showcase === 'cover') return <Card {...cardProps} title={undefined} cover={<div className="antd-card-cover-demo"><span>AI</span><strong>Design System</strong></div>} style={cardStyle} styles={{ ...(p.styles ?? {}), body: { ...bodyStyle, height: 'auto' } }}><Card.Meta title={editableContent} description="从想法到可编辑网站" /></Card>;
      if (showcase === 'actions') return <Card {...cardProps} style={cardStyle} actions={[<button key="edit">编辑</button>, <button key="share">分享</button>, <button key="more">更多</button>]} styles={{ ...(p.styles ?? {}), body: { ...bodyStyle, height: 'auto' } }}><strong>设计项目</strong><p className="antd-card-demo-copy">{editableContent}</p><Space><Tag color="blue">进行中</Tag><Typography.Text type="secondary">刚刚更新</Typography.Text></Space></Card>;
      if (showcase === 'meta') return <Card {...cardProps} title={undefined} style={cardStyle} styles={{ ...(p.styles ?? {}), body: { ...bodyStyle, height: 'auto', display: 'flex', alignItems: 'center' } }}><Card.Meta avatar={<Avatar size={48}>AI</Avatar>} title="Alex Chen" description={<><div>{editableContent}</div><Typography.Text type="secondary">负责官网与设计系统</Typography.Text></>} /></Card>;
      if (showcase === 'grid') return <Card {...cardProps} title={editableContent} style={cardStyle} styles={{ ...(p.styles ?? {}), body: { ...bodyStyle, height: 'auto', padding: 0 } }}>{[['页面', '12'], ['组件', '86'], ['批注', '7'], ['进度', '72%']].map(([label, value]) => <Card.Grid key={label} hoverable style={{ width: '50%', padding: 15, textAlign: 'center' }}><strong className="antd-card-metric">{value}</strong><small>{label}</small></Card.Grid>)}</Card>;
      if (showcase === 'inner') return <Card {...cardProps} title="项目概览" style={cardStyle} styles={{ ...(p.styles ?? {}), body: { ...bodyStyle, height: 'auto' } }}><p className="antd-card-demo-copy">{editableContent}</p><Card type="inner" size="small" title="本周数据" extra={<a>详情</a>}><Space size="large"><Statistic title="访问" value={12840} /><Statistic title="转化率" value={18.6} suffix="%" /></Space></Card></Card>;
      if (showcase === 'hoverable') return <Card {...cardProps} style={{ ...cardStyle, cursor: 'pointer' }} styles={{ ...(p.styles ?? {}), body: bodyStyle }}><div className="antd-card-hover-hint"><span>↗</span><strong>移入查看悬浮反馈</strong></div><p className="antd-card-demo-copy">{editableContent}</p></Card>;
      if (showcase === 'compact') return <Card {...cardProps} title="紧凑信息" extra={<a>查看</a>} style={cardStyle} styles={{ ...(p.styles ?? {}), body: { ...bodyStyle, height: 'auto' } }}>{editableContent}</Card>;
      if (showcase === 'borderless') return <div className="antd-card-borderless-surface"><Card {...cardProps} style={cardStyle} styles={{ ...(p.styles ?? {}), body: bodyStyle }}>{editableContent}</Card></div>;
      return <Card {...cardProps} style={cardStyle} styles={{ ...(p.styles ?? {}), body: bodyStyle }}>{editableContent}</Card>;
    }
    case 'Carousel': return <Carousel {...p} style={{ width: '100%' }}>{['产品设计', 'AI 协作', '代码交付'].map((text, index) => <div key={text}><div className="antd-carousel-slide">{slotContent[`slide-${index + 1}`] ?? text}</div></div>)}</Carousel>;
    case 'Collapse': return <Collapse {...p} items={Array.isArray(p.items) ? p.items.map((item: AnyProps, index: number) => ({ ...item, children: slotContent[`panel-${String(item.key ?? index + 1).replace(/[^a-zA-Z0-9_-]/g, '-')}`] ?? item.children })) : p.items} style={{ width: '100%' }} />;
    case 'Descriptions': return <Descriptions {...p} style={{ width: '100%' }} />;
    case 'Empty': {
      const image = p.image === 'simple' ? Empty.PRESENTED_IMAGE_SIMPLE : p.image === 'default' ? undefined : p.image;
      return <Empty {...p} image={image} description={component.content} />;
    }
    case 'Image': return <AntImage {...p} width="100%" height="100%" style={{ objectFit: 'cover', ...(p.style ?? {}) }} />;
    case 'List': {
      if (binding.variant === 'metadata') return <List {...p} style={{ width: '100%' }} renderItem={(entry: AnyProps) => <List.Item><List.Item.Meta avatar={<Avatar style={{ background: '#1677ff' }}>{entry.avatar}</Avatar>} title={entry.title} description={entry.description} /></List.Item>} />;
      if (binding.variant === 'actions') return <List {...p} style={{ width: '100%' }} renderItem={(entry: AnyProps) => <List.Item actions={[<a key="edit">编辑</a>, <a key="more">更多</a>]}><List.Item.Meta title={entry.title} description={entry.description} /></List.Item>} />;
      if (binding.variant === 'grid') return <List {...p} style={{ width: '100%' }} renderItem={(entry: AnyProps) => <List.Item><Card size="small" title={entry.title}>{entry.description}</Card></List.Item>} />;
      if (binding.variant === 'vertical') return <List {...p} style={{ width: '100%' }} renderItem={(entry: AnyProps) => <List.Item><List.Item.Meta title={entry.title} description={entry.description} /></List.Item>} />;
      return <List {...p} style={{ width: '100%' }} renderItem={(entry: string) => <List.Item>{entry}</List.Item>} />;
    }
    case 'Listy': {
      const total = Math.max(1, Math.min(10000, Number(p.itemCount ?? 20)));
      const visibleCount = binding.variant === 'infinite' ? Math.min(total, listyCount) : total;
      const orderedIds = binding.variant === 'drag-sorting' ? listyOrder.slice(0, visibleCount) : Array.from({ length: visibleCount }, (_, index) => index);
      const items = orderedIds.map((index) => ({ id: index, name: `成员 ${String(index + 1).padStart(3, '0')}`, group: String.fromCharCode(65 + (index % 8)), description: index % 3 === 0 ? '完成了页面结构与组件状态检查。' : '更新了设计批注和交互细节。', time: `${String(9 + index % 9).padStart(2, '0')}:${String((index * 7) % 60).padStart(2, '0')}` }));
      const listHeight = binding.variant === 'infinite' || binding.variant === 'scroll-control' ? Math.max(160, Number(p.height ?? 280) - 48) : Number(p.height ?? 260);
      const group = binding.variant === 'grouped' ? { key: (entry: AnyProps) => entry.group, title: (key: string) => <strong className="antd-listy-group">分组 {key}</strong> } : undefined;
      const itemRender = binding.variant === 'rich' ? (entry: AnyProps) => <div className="antd-listy-rich-item"><Avatar style={{ background: '#1677ff' }}>{entry.group}</Avatar><div><strong>{entry.name}</strong><span>{entry.description}</span></div><time>{entry.time}</time></div>
        : binding.variant === 'drag-sorting' ? (entry: AnyProps) => <div className="antd-listy-item antd-listy-sortable" draggable={preview} onDragStart={() => setDraggingListyId(entry.id)} onDragOver={(event) => event.preventDefault()} onDrop={() => {
          if (!preview || draggingListyId === undefined || draggingListyId === entry.id) return;
          setListyOrder((current) => {
            const next = current.filter((id) => id !== draggingListyId);
            next.splice(Math.max(0, next.indexOf(entry.id)), 0, draggingListyId);
            return next;
          });
          setDraggingListyId(undefined);
        }}><b>≡</b><span>{entry.name}</span><small>{entry.time}</small></div>
          : (entry: AnyProps) => <div className="antd-listy-item"><span>{entry.name}</span><small>{entry.time}</small></div>;
      return <div className={`antd-listy-shell ${p.semanticStyle ? 'semantic-style' : ''}`}><Listy ref={listyRef} items={items} rowKey="id" height={listHeight} virtual={Boolean(p.virtual)} sticky={Boolean(p.sticky)} group={group} itemRender={itemRender} />{binding.variant === 'infinite' && <Button size="small" disabled={visibleCount >= total} onClick={() => preview && setListyCount((value) => Math.min(total, value + 20))}>{visibleCount >= total ? '已加载全部' : `加载更多 · ${visibleCount}/${total}`}</Button>}{binding.variant === 'scroll-control' && <Space size="small"><Button size="small" onClick={() => preview && listyRef.current?.scrollTo({ key: Math.min(79, total - 1), align: 'top' })}>跳到第 80 项</Button><Button size="small" onClick={() => preview && listyRef.current?.scrollTo(0)}>回到顶部</Button></Space>}</div>;
    }
    case 'Popover': return <Popover {...p} open={showcase ? true : undefined} content={slotContent.popup ?? p.content}><Button>{component.content}</Button></Popover>;
    case 'QRCode': return <QRCode {...p} value={p.value ?? 'https://ant.design'} />;
    case 'Segmented': return <Segmented {...p} options={p.options ?? ['日', '周', '月']} />;
    case 'Statistic': return <Statistic {...p} />;
    case 'Table': {
      const expandable = binding.variant === 'expandable' ? { ...(p.expandable ?? {}), expandedRowRender: (record: AnyProps) => <div style={{ padding: '8px 36px' }}>“{record.name}”的详细说明、负责人和最近更新时间。</div> } : p.expandable;
      return <Table {...p} expandable={expandable} style={{ width: '100%' }} />;
    }
    case 'Tabs': {
      const baseItems: AnyProps[] = (binding.variant === 'editable' ? editableTabItems : Array.isArray(p.items) ? p.items : []).map((item: AnyProps, index: number) => ({ ...item, children: slotContent[`tab-${String(item.key ?? index + 1).replace(/[^a-zA-Z0-9_-]/g, '-')}`] ?? item.children, closable: index > 0 }));
      const onEdit = (target: any, action: 'add' | 'remove') => {
        if (!preview) return;
        if (action === 'add') setEditableTabItems((items) => [...items, { key: `new-${items.length + 1}`, label: `新标签 ${items.length + 1}`, children: '新标签页内容' }]);
        else setEditableTabItems((items) => items.filter((item) => String(item.key) !== String(target)));
      };
      return <Tabs {...p} items={baseItems as any} onEdit={binding.variant === 'editable' ? onEdit : undefined} style={{ width: '100%' }} />;
    }
    case 'Tag': return <Tag {...p} style={{ ...(p.style ?? {}), ...visualStyle }}>{component.content}</Tag>;
    case 'Timeline': return <Timeline {...p} style={{ width: '100%' }} />;
    case 'Tooltip': return <Tooltip {...p} open={showcase ? true : undefined}><Button>{component.content}</Button></Tooltip>;
    case 'Tour': return <><Button ref={tourTarget} type="primary" onClick={() => preview && setTourOpen(true)}>{component.content}</Button><Tour open={tourOpen} mask={p.mask} onClose={() => setTourOpen(false)} steps={[{ title: p.title ?? '功能引导', description: p.description ?? '了解这个组件。', placement: p.placement ?? 'bottom', target: () => tourTarget.current! }]} /></>;
    case 'Tree': return <Tree {...p} style={{ width: '100%' }} />;
    case 'Alert': return <Alert {...p} style={{ width: '100%' }} />;
    case 'Modal': {
      const previewWidth = showcase ? Number(p.width ?? 520) >= 700 ? 'calc(100% - 12px)' : Number(p.width ?? 520) <= 420 ? 'calc(100% - 72px)' : 'calc(100% - 36px)' : p.width;
      return <><Button ref={modalTrigger} type="primary" onClick={() => preview && setModalOpen(true)}>{component.content}</Button><Modal {...p} width={previewWidth} open={modalOpen} getContainer={() => designCanvasFor(modalTrigger.current)} rootStyle={{ position: 'absolute' }} onOk={() => setModalOpen(false)} onCancel={() => setModalOpen(false)}>{slotContent.content ?? <><p>这是可交互的 Ant Design 对话框。</p><Input placeholder="可以在这里输入内容" /></>}</Modal></>;
    }
    case 'Drawer': return <><Button ref={drawerTrigger} onClick={() => preview && setDrawerOpen(true)}>{component.content}</Button><Drawer {...p} open={drawerOpen} getContainer={() => designCanvasFor(drawerTrigger.current)} rootStyle={{ position: 'absolute' }} onClose={() => setDrawerOpen(false)}>{slotContent.content ?? <Space direction="vertical" size="middle" style={{ width: '100%' }}><Typography.Paragraph>抽屉已经进入网页内部的真实交互状态，可以承载详情、表单或导航内容。</Typography.Paragraph><Input placeholder="输入内容" /><Button type="primary">保存修改</Button></Space>}</Drawer></>;
    case 'Message': return <>{messageContext}<Button onClick={() => preview && messageApi[p.type === 'success' ? 'success' : p.type === 'warning' ? 'warning' : p.type === 'error' ? 'error' : 'info'](p.content ?? '操作已完成')}>{component.content}</Button></>;
    case 'Popconfirm': return <Popconfirm {...p} open={showcase ? true : undefined} title={p.title ?? '确定执行吗？'} disabled={!preview}><Button danger>{component.content}</Button></Popconfirm>;
    case 'Notification': return <>{notificationContext}<Button onClick={() => preview && notificationApi.open({ type: p.type ?? 'info', placement: p.placement ?? 'topRight', message: p.message ?? '设计已更新', description: p.description ?? '操作已经完成。' })}>{component.content}</Button></>;
    case 'Progress': return <Progress {...p} style={{ width: '100%' }} />;
    case 'Result': return <Result {...p} extra={slotContent.extra ?? p.extra} style={{ padding: 12 }} />;
    case 'Skeleton': return <Skeleton {...p} />;
    case 'Spin': return <Spin {...p} tip={component.content}><div style={{ width: 120, height: 54 }} /></Spin>;
    case 'Affix': return <Affix {...p}><Button type="primary">{component.content}</Button></Affix>;
    case 'Watermark': return <Watermark {...p} style={fill}>{slotContent.content ?? <div className="antd-watermark-content">产品内容区域</div>}</Watermark>;
    case 'App': {
      const { messageMaxCount, notificationPlacement, ...appProps } = p;
      return <AntApp {...appProps} message={{ maxCount: Number(messageMaxCount ?? 3) }} notification={{ placement: notificationPlacement ?? 'topRight' }} style={fill}>{slotContent.content ?? <AntdAppActions active={preview} />}</AntApp>;
    }
    case 'ConfigProvider': {
      const { direction, componentSize, componentDisabled, ...configProps } = p;
      return <ConfigProvider {...configProps} direction={direction} componentSize={componentSize} componentDisabled={componentDisabled}><div className="antd-config-provider-demo" style={fill}>{slotContent.content ?? <><Input placeholder="全局配置下的输入框" /><Select defaultValue="design" options={[{ value: 'design', label: '网站设计' }, { value: 'ai', label: 'AI 应用' }]} /><Button type="primary">主要操作</Button></>}</div></ConfigProvider>;
    }
    case 'BorderBeam': return <BorderBeam {...p}><div className="antd-border-beam-host" style={fill}>{slotContent.content ?? <><Typography.Title level={4}>重点内容区域</Typography.Title><Typography.Paragraph>适合 AI 模块、推荐卡片和关键行动区域。</Typography.Paragraph><Button type="primary">开始设计</Button></>}</div></BorderBeam>;
    default: return <Tag color="blue">Ant Design · {name}</Tag>;
  }
}

function AntdAppActions({ active }: { active: boolean }) {
  const { message: appMessage, modal, notification: appNotification } = AntApp.useApp();
  return <Space wrap><Button onClick={() => active && appMessage.success('保存成功')}>Message</Button><Button onClick={() => active && modal.info({ title: '应用级对话框', content: '它继承 App 与 ConfigProvider 的上下文。' })}>Modal</Button><Button onClick={() => active && appNotification.info({ title: '应用通知', description: '通知位置和数量可在右侧配置。' })}>Notification</Button></Space>;
}
