import { useState } from 'react';
import {
  Affix, Alert, Anchor, AutoComplete, Avatar, Badge, Breadcrumb, Button, Calendar, Card, Carousel,
  Cascader, Checkbox, Collapse, ColorPicker, DatePicker, Descriptions, Divider, Drawer, Dropdown,
  Empty, Flex, FloatButton, Form, Image as AntImage, Input, InputNumber, Layout, List, Masonry, Menu,
  Mentions, Modal, Pagination, Popconfirm, Popover, Progress, QRCode, Radio, Rate, Result, Row, Col,
  Segmented, Select, Skeleton, Slider, Space, Spin, Splitter, Statistic, Steps, Switch, Table, Tabs,
  Tag, TimePicker, Timeline, Tooltip, Transfer, Tree, TreeSelect, Typography, Upload, Watermark
} from 'antd';
import { AntDesignOutlined, UploadOutlined } from '@ant-design/icons';
import type { WebDesignComponent } from '../../src/schema';

type AnyProps = Record<string, any>;

export function AntdCanvasComponent({ component, preview }: { component: WebDesignComponent; preview: boolean }) {
  const [modalOpen, setModalOpen] = useState(false);
  const [drawerOpen, setDrawerOpen] = useState(false);
  const binding = component.library;
  if (!binding || binding.name !== 'antd') return null;
  const p = binding.props as AnyProps;
  const name = binding.component;
  const fill = { width: '100%', height: '100%' };

  switch (name) {
    case 'Button': return <Button {...p}>{component.content}</Button>;
    case 'FloatButton': return <div style={{ ...fill, position: 'relative' }}><FloatButton {...p} style={{ position: 'absolute', right: 6, bottom: 6 }} /></div>;
    case 'Icon': return <AntDesignOutlined style={{ color: '#1677ff', fontSize: 32 }} />;
    case 'Typography': return <Typography.Title level={p.level ?? 3} style={{ margin: 0 }}>{component.content}</Typography.Title>;
    case 'Divider': return <Divider {...p}>{component.content}</Divider>;
    case 'Flex': return <Flex {...p} style={fill}><Button>按钮一</Button><Button type="primary">按钮二</Button><Tag color="blue">标签</Tag></Flex>;
    case 'Grid': return <Row gutter={p.gutter ?? 8} style={{ ...fill, width: '100%' }}>{[1, 2, 3].map((value) => <Col key={value} span={8}><div className="antd-grid-cell">{value}</div></Col>)}</Row>;
    case 'Layout': return <Layout style={fill}><Layout.Header className="antd-layout-header">Header</Layout.Header><Layout><Layout.Sider width="28%" className="antd-layout-sider">Sider</Layout.Sider><Layout.Content className="antd-layout-content">Content</Layout.Content></Layout></Layout>;
    case 'Masonry': {
      const items = [72, 110, 88, 128, 96, 70].map((height, index) => ({ key: index, data: { height, label: index + 1 } }));
      return <Masonry {...p} items={items} itemRender={({ data }: AnyProps) => <div className="antd-masonry-item" style={{ height: data.height }}>{data.label}</div>} />;
    }
    case 'Space': return <Space {...p}><Button>取消</Button><Button type="primary">确定</Button><Tag>更多</Tag></Space>;
    case 'Splitter': return <Splitter {...p} style={fill}><Splitter.Panel><div className="antd-split-panel">面板一</div></Splitter.Panel><Splitter.Panel><div className="antd-split-panel">面板二</div></Splitter.Panel></Splitter>;
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
    case 'Form': return <Form {...p} style={{ width: '100%' }}><Form.Item label="名称" style={{ marginBottom: 10 }}><Input placeholder="请输入名称" /></Form.Item><Form.Item label="邮箱" style={{ marginBottom: 10 }}><Input placeholder="name@example.com" /></Form.Item><Button type="primary">提交</Button></Form>;
    case 'Input': return <Input {...p} placeholder={component.content} />;
    case 'InputNumber': return <InputNumber {...p} style={{ width: '100%' }} />;
    case 'Mentions': return <Mentions {...p} placeholder={component.content} style={{ height: '100%' }} />;
    case 'Radio': return <Radio.Group {...p} />;
    case 'Rate': return <Rate {...p} />;
    case 'Select': return <Select {...p} style={{ width: '100%' }} placeholder={component.content} />;
    case 'Slider': return <Slider {...p} style={{ width: '100%' }} />;
    case 'Switch': return <Space><Switch {...p} /><span>{component.content}</span></Space>;
    case 'TimePicker': return <TimePicker {...p} style={{ width: '100%' }} />;
    case 'Transfer': return <Transfer {...p} render={(item: AnyProps) => item.title} showSelectAll={false} listStyle={{ width: 190, height: 170 }} />;
    case 'TreeSelect': return <TreeSelect {...p} style={{ width: '100%' }} placeholder={component.content} />;
    case 'Upload': return <Upload {...p}><Button icon={<UploadOutlined />}>{component.content}</Button></Upload>;
    case 'Avatar': return <Avatar {...p}>{component.content}</Avatar>;
    case 'Badge': return <Badge {...p}><Avatar shape="square" icon={<AntDesignOutlined />} /></Badge>;
    case 'Calendar': return <Calendar {...p} style={{ width: '100%', height: '100%', overflow: 'hidden' }} />;
    case 'Card': return <Card {...p} style={fill}>{component.content}</Card>;
    case 'Carousel': return <Carousel {...p} style={{ width: '100%' }}>{['产品设计', 'AI 协作', '代码交付'].map((text) => <div key={text}><div className="antd-carousel-slide">{text}</div></div>)}</Carousel>;
    case 'Collapse': return <Collapse {...p} style={{ width: '100%' }} />;
    case 'Descriptions': return <Descriptions {...p} style={{ width: '100%' }} />;
    case 'Empty': return <Empty {...p} description={component.content} />;
    case 'Image': return <AntImage {...p} width="100%" height="100%" style={{ objectFit: 'cover' }} />;
    case 'List': return <List {...p} style={{ width: '100%' }} renderItem={(entry: string) => <List.Item>{entry}</List.Item>} />;
    case 'Popover': return <Popover {...p}><Button>{component.content}</Button></Popover>;
    case 'QRCode': return <QRCode {...p} value={p.value ?? 'https://ant.design'} />;
    case 'Segmented': return <Segmented {...p} options={p.options ?? ['日', '周', '月']} block />;
    case 'Statistic': return <Statistic {...p} />;
    case 'Table': return <Table {...p} style={{ width: '100%' }} />;
    case 'Tabs': return <Tabs {...p} style={{ width: '100%' }} />;
    case 'Tag': return <Tag {...p}>{component.content}</Tag>;
    case 'Timeline': return <Timeline {...p} style={{ width: '100%' }} />;
    case 'Tooltip': return <Tooltip {...p}><Button>{component.content}</Button></Tooltip>;
    case 'Tree': return <Tree {...p} style={{ width: '100%' }} />;
    case 'Alert': return <Alert {...p} style={{ width: '100%' }} />;
    case 'Modal': return <><Button type="primary" onClick={() => preview && setModalOpen(true)}>{component.content}</Button><Modal {...p} open={modalOpen} onOk={() => setModalOpen(false)} onCancel={() => setModalOpen(false)} getContainer={false}>这是可交互的 Ant Design 对话框。</Modal></>;
    case 'Drawer': return <><Button onClick={() => preview && setDrawerOpen(true)}>{component.content}</Button><Drawer {...p} open={drawerOpen} onClose={() => setDrawerOpen(false)} getContainer={false}>这是抽屉内容。</Drawer></>;
    case 'Popconfirm': return <Popconfirm {...p} title={p.title ?? '确定执行吗？'} disabled={!preview}><Button danger>{component.content}</Button></Popconfirm>;
    case 'Progress': return <Progress {...p} style={{ width: '100%' }} />;
    case 'Result': return <Result {...p} style={{ padding: 12 }} />;
    case 'Skeleton': return <Skeleton {...p} />;
    case 'Spin': return <Spin {...p} tip={component.content}><div style={{ width: 120, height: 54 }} /></Spin>;
    case 'Affix': return <Affix {...p}><Button type="primary">{component.content}</Button></Affix>;
    case 'Watermark': return <Watermark {...p} style={fill}><div className="antd-watermark-content">产品内容区域</div></Watermark>;
    default: return <Tag color="blue">Ant Design · {name}</Tag>;
  }
}
