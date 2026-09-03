import { useRef, useState, type ReactNode } from 'react';
import {
  Affix, Alert, Anchor, AutoComplete, Avatar, Badge, Breadcrumb, Button, Calendar, Card, Carousel, ConfigProvider,
  Cascader, Checkbox, Collapse, ColorPicker, DatePicker, Descriptions, Divider, Drawer, Dropdown,
  Empty, Flex, FloatButton, Form, Image as AntImage, Input, InputNumber, Layout, List, Masonry, Menu,
  Mentions, Modal, Pagination, Popconfirm, Popover, Progress, QRCode, Radio, Rate, Result, Row, Col,
  Segmented, Select, Skeleton, Slider, Space, Spin, Splitter, Statistic, Steps, Switch, Table, Tabs,
  Tag, TimePicker, Timeline, Tooltip, Tour, Transfer, Tree, TreeSelect, Typography, Upload, Watermark,
  message, notification
} from 'antd';
import { AntDesignOutlined, UploadOutlined } from '@ant-design/icons';
import type { WebDesignComponent, WebDesignTokens } from '../../src/schema';

type AnyProps = Record<string, any>;

export function AntdCanvasComponent({ component, preview, tokens, slotContent = {} }: { component: WebDesignComponent; preview: boolean; tokens?: WebDesignTokens; slotContent?: Record<string, ReactNode> }) {
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
  }} getPopupContainer={(triggerNode) => (triggerNode?.closest('.design-canvas') as HTMLElement | null) ?? document.body}><AntdCanvasRenderer component={component} preview={preview} slotContent={slotContent} /></ConfigProvider>;
}

function AntdCanvasRenderer({ component, preview, slotContent }: { component: WebDesignComponent; preview: boolean; slotContent: Record<string, ReactNode> }) {
  const [modalOpen, setModalOpen] = useState(false);
  const [drawerOpen, setDrawerOpen] = useState(false);
  const [tourOpen, setTourOpen] = useState(false);
  const modalTrigger = useRef<HTMLButtonElement>(null);
  const drawerTrigger = useRef<HTMLButtonElement>(null);
  const [transferTargetKeys, setTransferTargetKeys] = useState<string[]>(() => {
    const targetKeys = component.library?.props.targetKeys;
    return Array.isArray(targetKeys) ? targetKeys.map(String) : [];
  });
  const tourTarget = useRef<HTMLButtonElement>(null);
  const [messageApi, messageContext] = message.useMessage();
  const [notificationApi, notificationContext] = notification.useNotification();
  const binding = component.library;
  if (!binding || binding.name !== 'antd') return null;
  const p = binding.props as AnyProps;
  const name = binding.component;
  const fill = { width: '100%', height: '100%' };
  const designCanvasFor = (trigger: HTMLButtonElement | null): HTMLElement => (trigger?.closest('.design-canvas') as HTMLElement | null) ?? document.body;

  switch (name) {
    case 'Button': return <Button {...p}>{component.content}</Button>;
    case 'FloatButton': return <div style={{ ...fill, position: 'relative' }}><FloatButton {...p} style={{ position: 'absolute', right: 6, bottom: 6 }} /></div>;
    case 'Icon': return <AntDesignOutlined style={{ color: p.color ?? '#1677ff', fontSize: p.size ?? 32 }} />;
    case 'Typography': {
      if (binding.variant === 'paragraph') return <Typography.Paragraph style={{ margin: 0 }}>{component.content}</Typography.Paragraph>;
      if (binding.variant === 'text') return <Typography.Text>{component.content}</Typography.Text>;
      return <Typography.Title level={p.level ?? 3} style={{ margin: 0 }}>{component.content}</Typography.Title>;
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
    case 'Splitter': return <Splitter {...p} style={fill}><Splitter.Panel>{slotContent['panel-1'] ?? <div className="antd-split-panel">面板一</div>}</Splitter.Panel><Splitter.Panel>{slotContent['panel-2'] ?? <div className="antd-split-panel">面板二</div>}</Splitter.Panel></Splitter>;
    case 'Anchor': return <Anchor {...p} />;
    case 'Breadcrumb': return <Breadcrumb {...p} />;
    case 'Dropdown': return <Dropdown {...p}><Button>{component.content}⌄</Button></Dropdown>;
    case 'Menu': return <Menu {...p} style={{ width: '100%' }} />;
    case 'Pagination': return <Pagination {...p} />;
    case 'Steps': return <Steps {...p} size="small" />;
    case 'AutoComplete': return <AutoComplete {...p} style={{ width: '100%' }} placeholder={component.content} />;
    case 'Cascader': return <Cascader {...p} style={{ width: '100%' }} placeholder={component.content} />;
    case 'Checkbox': return <Checkbox {...p}>{component.content}</Checkbox>;
    case 'ColorPicker': return <ColorPicker {...p} />;
    case 'DatePicker': return <DatePicker {...p} style={{ width: '100%' }} />;
    case 'Form': return <Form {...p} style={{ width: '100%', height: '100%' }}>{slotContent.content ?? <><Form.Item label="名称" style={{ marginBottom: 10 }}><Input placeholder="请输入名称" /></Form.Item><Form.Item label="邮箱" style={{ marginBottom: 10 }}><Input placeholder="name@example.com" /></Form.Item><Button type="primary">提交</Button></>}</Form>;
    case 'Input': {
      if (binding.variant === 'search') return <Input.Search {...p} placeholder={component.content} />;
      if (binding.variant === 'password') return <Input.Password {...p} placeholder={component.content} />;
      if (binding.variant === 'textarea') return <Input.TextArea {...p} placeholder={component.content} style={{ height: '100%' }} />;
      return <Input {...p} placeholder={component.content} />;
    }
    case 'InputNumber': return <InputNumber {...p} style={{ width: '100%' }} />;
    case 'Mentions': return <Mentions {...p} placeholder={component.content} style={{ height: '100%' }} />;
    case 'Radio': return <Radio.Group {...p} />;
    case 'Rate': return <Rate {...p} />;
    case 'Select': return <Select {...p} style={{ width: '100%' }} placeholder={component.content} />;
    case 'Slider': return <Slider {...p} style={{ width: '100%' }} />;
    case 'Switch': return <Space><Switch {...p} /><span>{component.content}</span></Space>;
    case 'TimePicker': return <TimePicker {...p} style={{ width: '100%' }} />;
    case 'Transfer': return <Transfer {...p} targetKeys={transferTargetKeys} onChange={(keys) => setTransferTargetKeys(keys.map(String))} render={(item: AnyProps) => item.title} listStyle={{ width: 190, height: 170 }} />;
    case 'TreeSelect': return <TreeSelect {...p} style={{ width: '100%' }} placeholder={component.content} />;
    case 'Upload': return binding.variant === 'picture'
      ? <Upload {...p}><button className="antd-picture-upload"><UploadOutlined /><span>上传图片</span></button></Upload>
      : <Upload {...p}><Button icon={<UploadOutlined />}>{component.content}</Button></Upload>;
    case 'Avatar': return <Avatar {...p}>{component.content}</Avatar>;
    case 'Badge': return p.status ? <Badge {...p} /> : <Badge {...p}><Avatar shape="square" icon={<AntDesignOutlined />} /></Badge>;
    case 'Calendar': return <Calendar {...p} style={{ width: '100%', height: '100%', overflow: 'hidden' }} />;
    case 'Card': return <Card {...p} style={fill}>{slotContent.content ?? component.content}</Card>;
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
      return <List {...p} style={{ width: '100%' }} renderItem={(entry: string) => <List.Item>{entry}</List.Item>} />;
    }
    case 'Popover': return <Popover {...p} content={slotContent.popup ?? p.content}><Button>{component.content}</Button></Popover>;
    case 'QRCode': return <QRCode {...p} value={p.value ?? 'https://ant.design'} />;
    case 'Segmented': return <Segmented {...p} options={p.options ?? ['日', '周', '月']} block />;
    case 'Statistic': return <Statistic {...p} />;
    case 'Table': return <Table {...p} style={{ width: '100%' }} />;
    case 'Tabs': return <Tabs {...p} items={Array.isArray(p.items) ? p.items.map((item: AnyProps, index: number) => ({ ...item, children: slotContent[`tab-${String(item.key ?? index + 1).replace(/[^a-zA-Z0-9_-]/g, '-')}`] ?? item.children })) : p.items} style={{ width: '100%' }} />;
    case 'Tag': return <Tag {...p}>{component.content}</Tag>;
    case 'Timeline': return <Timeline {...p} style={{ width: '100%' }} />;
    case 'Tooltip': return <Tooltip {...p}><Button>{component.content}</Button></Tooltip>;
    case 'Tour': return <><Button ref={tourTarget} type="primary" onClick={() => preview && setTourOpen(true)}>{component.content}</Button><Tour open={tourOpen} mask={p.mask} onClose={() => setTourOpen(false)} steps={[{ title: p.title ?? '功能引导', description: p.description ?? '了解这个组件。', placement: p.placement ?? 'bottom', target: () => tourTarget.current! }]} /></>;
    case 'Tree': return <Tree {...p} style={{ width: '100%' }} />;
    case 'Alert': return <Alert {...p} style={{ width: '100%' }} />;
    case 'Modal': return <><Button ref={modalTrigger} type="primary" onClick={() => preview && setModalOpen(true)}>{component.content}</Button><Modal {...p} open={modalOpen} getContainer={() => designCanvasFor(modalTrigger.current)} onOk={() => setModalOpen(false)} onCancel={() => setModalOpen(false)}>{slotContent.content ?? <><p>这是可交互的 Ant Design 对话框。</p><Input placeholder="可以在这里输入内容" /></>}</Modal></>;
    case 'Drawer': return <><Button ref={drawerTrigger} onClick={() => preview && setDrawerOpen(true)}>{component.content}</Button><Drawer {...p} open={drawerOpen} getContainer={() => designCanvasFor(drawerTrigger.current)} rootStyle={{ position: 'absolute' }} onClose={() => setDrawerOpen(false)}>{slotContent.content ?? <Space direction="vertical" size="middle" style={{ width: '100%' }}><Typography.Paragraph>抽屉已经进入网页内部的真实交互状态，可以承载详情、表单或导航内容。</Typography.Paragraph><Input placeholder="输入内容" /><Button type="primary">保存修改</Button></Space>}</Drawer></>;
    case 'Message': return <>{messageContext}<Button onClick={() => preview && messageApi[p.type === 'success' ? 'success' : p.type === 'warning' ? 'warning' : p.type === 'error' ? 'error' : 'info'](p.content ?? '操作已完成')}>{component.content}</Button></>;
    case 'Popconfirm': return <Popconfirm {...p} title={p.title ?? '确定执行吗？'} disabled={!preview}><Button danger>{component.content}</Button></Popconfirm>;
    case 'Notification': return <>{notificationContext}<Button onClick={() => preview && notificationApi.open({ type: p.type ?? 'info', placement: p.placement ?? 'topRight', message: p.message ?? '设计已更新', description: p.description ?? '操作已经完成。' })}>{component.content}</Button></>;
    case 'Progress': return <Progress {...p} style={{ width: '100%' }} />;
    case 'Result': return <Result {...p} extra={slotContent.extra ?? p.extra} style={{ padding: 12 }} />;
    case 'Skeleton': return <Skeleton {...p} />;
    case 'Spin': return <Spin {...p} tip={component.content}><div style={{ width: 120, height: 54 }} /></Spin>;
    case 'Affix': return <Affix {...p}><Button type="primary">{component.content}</Button></Affix>;
    case 'Watermark': return <Watermark {...p} style={fill}>{slotContent.content ?? <div className="antd-watermark-content">产品内容区域</div>}</Watermark>;
    default: return <Tag color="blue">Ant Design · {name}</Tag>;
  }
}
