import { useEffect, useRef, useState, type ReactNode } from 'react';
import {
  AbsoluteCenter, Accordion, ActionBar, Alert, AspectRatio, Avatar, Badge, Bleed, Blockquote, Box, Breadcrumb, Button,
  Card, Carousel, Center, ChakraProvider, Checkbox, CheckboxCard, Checkmark, Clipboard, CloseButton, Code, CodeBlock, Collapsible,
  ClientOnly, ColorPicker, ColorSwatch, Combobox, Container, DataList, DateInput, DatePicker, Dialog, DownloadTrigger,
  Editable, Em, EmptyState, EnvironmentProvider, Field, Fieldset, FileUpload, Flex, Float, FloatingPanel, For,
  FormatByte, FormatNumber, Grid, Group, Heading, Highlight, HoverCard, Icon, IconButton, Image, Input, InputGroup, Kbd,
  Link, LinkBox, LinkOverlay, List, Listbox, LocaleProvider, Marquee, Mark, NativeSelect,
  NumberInput, Pagination, PinInput, Popover as ChakraPopover, Portal, Progress, ProgressCircle, QrCode, RadioCard,
  Presence, RadioGroup, Radiomark, RatingGroup, ScrollArea, SegmentGroup, Select, Separator, Show, SimpleGrid, Skeleton,
  SkipNavContent, SkipNavLink, Slider, Spinner, Splitter, Stack, Stat, Status, Steps, Switch, Table, Tabs, Tag, TagsInput,
  Text, Textarea, Theme, Timeline, Toast,
  Toaster as ChakraToaster, TreeView, Wrap, WrapItem, chakra, createListCollection, createOverlay, createToaster,
  VisuallyHidden, createTreeCollection, defaultSystem, defineStyle, parseColor, plainTextAdapter
} from '@chakra-ui/react';
import { EditorContent, useEditor } from '@tiptap/react';
import StarterKit from '@tiptap/starter-kit';
import { parseDate, parseDateTime } from '@internationalized/date';
import {
  Bell, Bold, Braces, CalendarDays, Check, CheckCircle2, ChevronLeft, ChevronRight, Circle, Download, Eye, EyeOff, File,
  Folder, FolderOpen, GripHorizontal, Heart, Heading1, Heading2, Heading3, Info, Italic, List as ListIcon, ListOrdered,
  Maximize2, Minus, Play, Quote, Redo2, Search, Share2, Sparkles, Square, Strikethrough, Trash2, TriangleAlert, Undo2,
  Settings, Upload, X
} from 'lucide-react';
import type { WebDesignComponent, WebDesignTokens } from '../../src/schema';
import { DesignOverlay, FloatingSurface, SkeletonComposition, runtimeRecords, runtimeRows, runtimeStrings } from './LibraryRuntimePrimitives';
import { designStyleScopeProps } from './component-style';

type AnyProps = Record<string, any>;

function overlayManagerBody(props: AnyProps) {
  if (props.kind === 'form') return <Stack gap="3"><Text textStyle="sm" color="fg.muted">{String(props.description)}</Text><Field.Root><Field.Label>项目名称</Field.Label><Input defaultValue="品牌官网改版" /></Field.Root><Field.Root><Field.Label>项目说明</Field.Label><Textarea defaultValue="统一首页视觉语言与响应式体验。" rows={2} /></Field.Root></Stack>;
  if (props.kind === 'details') return <Stack gap="3"><Text textStyle="sm" color="fg.muted">{String(props.description)}</Text><DataList.Root orientation="horizontal" size="sm"><DataList.Item><DataList.ItemLabel>组件</DataList.ItemLabel><DataList.ItemValue>Hero Section</DataList.ItemValue></DataList.Item><DataList.Item><DataList.ItemLabel>尺寸</DataList.ItemLabel><DataList.ItemValue>1200 × 640</DataList.ItemValue></DataList.Item><DataList.Item><DataList.ItemLabel>状态</DataList.ItemLabel><DataList.ItemValue><Status.Root colorPalette="green"><Status.Indicator />可用</Status.Root></DataList.ItemValue></DataList.Item></DataList.Root></Stack>;
  return <Stack gap="3"><Flex gap="3" align="start"><Center width="9" height="9" flex="none" borderRadius="full" background="orange.100" color="orange.700"><TriangleAlert size={18} /></Center><Box><Text fontWeight="semibold">{String(props.title)}</Text><Text textStyle="sm" color="fg.muted">{String(props.description)}</Text></Box></Flex><Flex justify="end" gap="2"><Button size="sm" variant="outline">取消</Button><Button size="sm" colorPalette="blue">确认发布</Button></Flex></Stack>;
}

function portalBody(props: AnyProps) {
  if (props.kind === 'panel') return <Stack width="64" gap="3"><Flex justify="space-between" align="center"><Text fontWeight="semibold">顶层图层</Text><CloseButton size="xs" /></Flex><Text textStyle="sm" color="fg.muted">面板脱离原层级，固定显示在页面顶层。</Text><Progress.Root value={68} colorPalette="purple" size="sm"><Progress.Track><Progress.Range /></Progress.Track></Progress.Root></Stack>;
  if (props.kind === 'message') return <Alert.Root status="info" variant="subtle"><Alert.Indicator /><Alert.Content><Alert.Title>页面消息</Alert.Title><Alert.Description>设计已同步到所有协作者。</Alert.Description></Alert.Content></Alert.Root>;
  return <Badge size="lg" colorPalette="purple" variant="solid" borderRadius="full" padding="2 3"><Sparkles size={14} />3 条新批注</Badge>;
}

function fieldsetBody(props: AnyProps) {
  if (props.kind === 'locked') return <Stack mt="4" gap="3"><Flex align="center" justify="space-between" padding="3" borderRadius="lg" background="gray.100"><Box><Text textStyle="xs" color="fg.muted">组织 ID</Text><Text fontWeight="semibold">ORG-2026-0918</Text></Box><Badge colorPalette="gray">管理员维护</Badge></Flex><Field.Root><Field.Label>组织名称</Field.Label><Input defaultValue="Design Systems Lab" /></Field.Root><Field.Root><Field.Label>认证域名</Field.Label><Input defaultValue="design.example.com" /></Field.Root></Stack>;
  if (props.kind === 'profile') return <Stack mt="4" gap="4"><Flex align="center" gap="3"><Avatar.Root size="lg" colorPalette="purple"><Avatar.Fallback name="林设计师" /></Avatar.Root><Box><Text fontWeight="semibold">头像与公开资料</Text><Text textStyle="xs" color="fg.muted">用于团队空间和分享页面</Text></Box><Button marginStart="auto" size="sm" variant="outline">更换头像</Button></Flex><SimpleGrid columns={2} gap="3"><Field.Root><Field.Label>姓名</Field.Label><Input defaultValue="林设计师" /></Field.Root><Field.Root><Field.Label>职位</Field.Label><Input defaultValue="产品设计师" /></Field.Root></SimpleGrid><Field.Root><Field.Label>个人简介</Field.Label><Textarea defaultValue="关注 AI 设计工具与多端体验。" rows={2} /></Field.Root><Button alignSelf="start" colorPalette="purple">保存资料</Button></Stack>;
  return <Stack mt="4" gap="3"><Field.Root><Field.Label>电子邮箱</Field.Label><Input type="email" placeholder="name@example.com" /></Field.Root><Field.Root><Field.Label>手机号码</Field.Label><Input type="tel" placeholder="+86 138 0000 0000" /></Field.Root><Button alignSelf="start" size="sm" variant="surface" colorPalette="blue">保存联系方式</Button></Stack>;
}

function toggleTipBody(props: AnyProps) {
  if (props.kind === 'rich') return <Stack width="64" gap="2"><Flex align="center" gap="2"><Settings size={16} /><Text fontWeight="semibold">锁定组件</Text></Flex><Text textStyle="sm" color="fg.muted">{String(props.content)}</Text><Button size="xs" alignSelf="start" variant="surface" colorPalette="purple">知道了</Button></Stack>;
  if (props.kind === 'shortcut') return <Flex align="center" gap="4"><Text textStyle="sm">{String(props.content)}</Text><Kbd>{String(props.shortcut ?? '⌘ K')}</Kbd></Flex>;
  return <Text textStyle="xs">{String(props.content)}</Text>;
}

function clientOnlyBody(props: AnyProps) {
  if (props.kind === 'time') return <Flex width="100%" height="100%" align="center" justify="space-between" padding="4" borderWidth="1px" borderRadius="xl"><Box><Text textStyle="xs" color="fg.muted">本地时间</Text><Heading size="xl" fontVariantNumeric="tabular-nums">{new Date().toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })}</Heading></Box><CalendarDays size={28} color="#5856d6" /></Flex>;
  if (props.kind === 'viewport') {
    const width = globalThis.window?.innerWidth ?? 1200;
    const height = globalThis.window?.innerHeight ?? 900;
    return <Stack width="100%" height="100%" padding="4" borderWidth="1px" borderRadius="xl" gap="3"><Flex justify="space-between"><Text fontWeight="semibold">客户端视口</Text><Status.Root colorPalette="green"><Status.Indicator />已连接</Status.Root></Flex><SimpleGrid columns={3} gap="2">{[['宽度', `${width}px`], ['高度', `${height}px`], ['像素比', `${globalThis.window?.devicePixelRatio ?? 1}x`]].map(([label, value]) => <Box key={label} padding="2" borderRadius="md" background="gray.50"><Text textStyle="xs" color="fg.muted">{label}</Text><Text fontWeight="semibold">{value}</Text></Box>)}</SimpleGrid></Stack>;
  }
  return <Flex width="100%" height="100%" align="center" gap="3" padding="4" borderWidth="1px" borderRadius="xl"><Center width="9" height="9" borderRadius="full" background="green.100" color="green.700"><CheckCircle2 size={18} /></Center><Box><Text fontWeight="semibold">客户端渲染就绪</Text><Text textStyle="xs" color="fg.muted">浏览器能力与交互事件均可使用</Text></Box></Flex>;
}

function environmentBody(props: AnyProps) {
  if (props.environment === 'canvas') return <Stack width="100%" height="100%" padding="4" borderWidth="1px" borderRadius="xl" gap="3"><Flex justify="space-between" align="center"><Badge colorPalette="purple">CANVAS ROOT</Badge><Text textStyle="xs" color="fg.muted">1200 × 900</Text></Flex><Box position="relative" flex="1" borderWidth="1px" borderStyle="dashed" borderRadius="lg" background="purple.50"><Center position="absolute" inset="3" borderRadius="md" background="white" shadow="sm"><Text textStyle="sm" fontWeight="medium">Popover / Drawer 定位层</Text></Center></Box></Stack>;
  if (props.environment === 'iframe') return <Stack width="100%" height="100%" padding="3" borderWidth="1px" borderRadius="xl" gap="2"><Flex align="center" gap="2"><Circle size={8} fill="#ff5f57" color="#ff5f57" /><Circle size={8} fill="#febc2e" color="#febc2e" /><Circle size={8} fill="#28c840" color="#28c840" /><Text marginStart="2" textStyle="xs" color="fg.muted">embedded-preview.html</Text></Flex><Center flex="1" borderRadius="lg" background="blue.50"><Stack gap="1" align="center"><Text fontWeight="semibold">iframe Document</Text><Text textStyle="xs" color="fg.muted">浮层不会逃逸到宿主页面</Text></Stack></Center></Stack>;
  return <Flex width="100%" height="100%" padding="4" borderWidth="1px" borderRadius="xl" align="center" gap="3"><Center width="10" height="10" borderRadius="lg" background="teal.100" color="teal.700"><Braces size={20} /></Center><Box><Badge colorPalette="teal">DOCUMENT</Badge><Heading size="sm" marginTop="1">{String(props.label)}</Heading><Text textStyle="xs" color="fg.muted">window.document · body portal root</Text></Box></Flex>;
}

function presenceBody(props: AnyProps) {
  if (props.kind === 'confirm') return <Stack width="100%" padding="4" borderRadius="xl" background="purple.50" gap="3"><Flex gap="3" align="center"><Center width="9" height="9" borderRadius="full" background="purple.600" color="white"><Sparkles size={17} /></Center><Box><Text fontWeight="semibold">应用 AI 修改？</Text><Text textStyle="xs" color="fg.muted">该面板缩放进入并在退出后卸载</Text></Box></Flex><Flex justify="end" gap="2"><Button size="xs" variant="outline">取消</Button><Button size="xs" colorPalette="purple">应用</Button></Flex></Stack>;
  if (props.kind === 'details') return <Stack width="100%" padding="4" borderRadius="xl" background="blue.50" gap="2"><Flex justify="space-between"><Text fontWeight="semibold">设计详情</Text><Badge colorPalette="blue">Lazy mount</Badge></Flex><Text textStyle="sm" color="fg.muted">仅在首次展开时创建内容，收起后仍保留状态。</Text><Progress.Root value={76} size="sm" colorPalette="blue"><Progress.Track><Progress.Range /></Progress.Track></Progress.Root></Stack>;
  return <Flex width="100%" padding="4" borderRadius="xl" background="green.50" align="center" gap="3"><Status.Root colorPalette="green"><Status.Indicator /></Status.Root><Box><Text fontWeight="semibold">设计已实时同步</Text><Text textStyle="xs" color="fg.muted">常驻内容，不在退出时卸载</Text></Box></Flex>;
}

function skipNavBody(props: AnyProps) {
  if (props.kind === 'dashboard') return <Grid width="100%" height="100%" templateColumns="112px 1fr" gap="3"><Stack padding="3" borderRadius="lg" background="gray.900" color="white" gap="3"><Text fontWeight="semibold">Console</Text>{['概览', '项目', '团队', '设置'].map((item) => <Text key={item} textStyle="xs" opacity={item === '项目' ? 1 : .6}>{item}</Text>)}</Stack><SkipNavContent><Stack height="100%" padding="4" borderRadius="lg" background="blue.50"><Text textStyle="xs" color="blue.600">{String(props.contentLabel)}</Text><Heading size="md">项目数据</Heading><SimpleGrid columns={2} gap="2"><Box height="12" borderRadius="md" background="white" /><Box height="12" borderRadius="md" background="white" /></SimpleGrid></Stack></SkipNavContent></Grid>;
  if (props.kind === 'product') return <Stack width="100%" height="100%" gap="3"><Flex padding="3" borderBottomWidth="1px" justify="space-between"><Text fontWeight="bold">NORTH</Text><Flex gap="4" textStyle="xs"><Text>Product</Text><Text>Solutions</Text><Text>Pricing</Text></Flex><Button size="xs">Start free</Button></Flex><SkipNavContent><Center flex="1" borderRadius="lg" background="linear-gradient(135deg, #eef2ff, #f5f3ff)"><Stack align="center" gap="1"><Badge colorPalette="purple">NEW</Badge><Heading size="lg">{String(props.contentLabel)}</Heading></Stack></Center></SkipNavContent></Stack>;
  return <Stack width="100%" height="100%" gap="3"><Flex padding="3" borderRadius="lg" background="gray.100" justify="space-between"><Text fontWeight="bold">Studio</Text><Text textStyle="sm">{String(props.navLabel)}</Text><Button size="xs" variant="outline">登录</Button></Flex><SkipNavContent><Box flex="1" padding="5" borderRadius="lg" background="blue.50"><Text textStyle="xs" color="blue.600">{String(props.contentLabel)}</Text><Heading size="lg" marginTop="1">设计更好的网站</Heading><Text textStyle="sm" color="fg.muted">键盘焦点会直接抵达这个主要区域。</Text></Box></SkipNavContent></Stack>;
}

const studioToaster = createToaster({ placement: 'bottom-end', pauseOnPageIdle: true });
const managedOverlay = createOverlay<AnyProps>((props) => {
  const { title, description, kind, content, ...rootProps } = props;
  return <Dialog.Root {...rootProps}><Portal><Dialog.Backdrop /><Dialog.Positioner><Dialog.Content><Dialog.Header><Dialog.Title>{String(title ?? '浮层')}</Dialog.Title></Dialog.Header><Dialog.Body>{content ?? overlayManagerBody({ title, description, kind })}</Dialog.Body><Dialog.CloseTrigger asChild><CloseButton /></Dialog.CloseTrigger></Dialog.Content></Dialog.Positioner></Portal></Dialog.Root>;
});
const sampleImage = `data:image/svg+xml,${encodeURIComponent('<svg xmlns="http://www.w3.org/2000/svg" width="800" height="480" viewBox="0 0 800 480"><defs><linearGradient id="g" x1="0" y1="0" x2="1" y2="1"><stop stop-color="#5D50DF"/><stop offset="1" stop-color="#8B5CF6"/></linearGradient></defs><rect width="800" height="480" rx="32" fill="url(#g)"/><circle cx="650" cy="100" r="180" fill="#fff" opacity=".12"/><rect x="80" y="90" width="320" height="30" rx="15" fill="#fff" opacity=".92"/><rect x="80" y="145" width="500" height="18" rx="9" fill="#fff" opacity=".45"/><rect x="80" y="210" width="640" height="190" rx="24" fill="#fff" opacity=".16"/><text x="105" y="315" font-family="Arial" font-size="42" font-weight="700" fill="white">Web Design Studio</text></svg>')}`;

function StudioToaster() {
  return <Portal><ChakraToaster toaster={studioToaster}>{(toast) => <Toast.Root width="sm"><Toast.Indicator /><Stack gap="1" flex="1"><Toast.Title>{toast.title}</Toast.Title><Toast.Description>{toast.description}</Toast.Description></Stack>{toast.closable && <Toast.CloseTrigger />}</Toast.Root>}</ChakraToaster></Portal>;
}

const chakraPortalShowcaseComponents = new Set(['ActionBar', 'FloatingPanel', 'HoverCard', 'OverlayManager', 'ToggleTip', 'Portal', 'Toast']);

function floatingPanelBody(props: AnyProps) {
  if (props.kind === 'layers') return <Stack gap="2">{['导航栏', '主视觉', '功能卡片'].map((label, index) => <Flex key={label} align="center" gap="2" padding="2" borderWidth="1px" borderRadius="md" background={index === 1 ? 'purple.50' : 'bg.panel'}><Eye size={14} /><Box flex="1"><Text textStyle="sm" fontWeight="medium">{label}</Text></Box><Text textStyle="xs" color="fg.muted">{index + 1}</Text></Flex>)}</Stack>;
  if (props.kind === 'audit') return <Stack gap="3"><SimpleGrid columns={3} gap="2">{[['对比度', 'AA'], ['间距', '8pt'], ['问题', '3']].map(([label, value]) => <Box key={label} padding="3" borderWidth="1px" borderRadius="lg"><Text textStyle="xs" color="fg.muted">{label}</Text><Heading size="md" marginTop="1">{value}</Heading></Box>)}</SimpleGrid><Progress.Root value={82} colorPalette="green" size="sm"><Progress.Track><Progress.Range /></Progress.Track></Progress.Root><Text textStyle="xs" color="fg.muted">设计规范覆盖率 82%</Text></Stack>;
  return <Stack gap="3"><Flex gap="2"><IconButton aria-label="设置" variant="subtle"><Settings /></IconButton><IconButton aria-label="搜索" variant="subtle"><Search /></IconButton><IconButton aria-label="分享" variant="subtle"><Share2 /></IconButton></Flex><Text textStyle="sm" color="fg.muted">常用设计操作集中在独立可拖动面板中。</Text></Stack>;
}

function hoverCardBody(props: AnyProps) {
  if (props.kind === 'product') return <Stack gap="3"><Flex align="center" gap="3"><Center width="11" height="11" borderRadius="xl" background="linear-gradient(135deg, #5856d6, #007aff)" color="white" fontWeight="bold">W</Center><Box><Text fontWeight="semibold">{String(props.title)}</Text><Text textStyle="xs" color="fg.muted">{String(props.description)}</Text></Box></Flex><SimpleGrid columns={2} gap="2"><Box padding="2" borderRadius="md" background="purple.50"><Text textStyle="xs">86 个组件</Text></Box><Box padding="2" borderRadius="md" background="blue.50"><Text textStyle="xs">3 套组件库</Text></Box></SimpleGrid></Stack>;
  if (props.kind === 'compact') return <Flex gap="2" align="center"><Center width="7" height="7" borderRadius="full" background="green.100" color="green.700"><Check size={15} /></Center><Box><Text textStyle="sm" fontWeight="semibold">{String(props.title)}</Text><Text textStyle="xs" color="fg.muted">{String(props.description)}</Text></Box></Flex>;
  return <Flex gap="3" align="start"><Avatar.Root size="md" colorPalette="purple"><Avatar.Fallback name={String(props.title)} /></Avatar.Root><Stack gap="1"><Text fontWeight="semibold">{String(props.title)}</Text><Text textStyle="sm" color="fg.muted">{String(props.description)}</Text><Flex gap="1"><Badge size="sm" colorPalette="purple">Product</Badge><Badge size="sm" variant="outline">Design</Badge></Flex></Stack></Flex>;
}

function ChakraPortalShowcase({ component, props, slotContent }: { component: WebDesignComponent; props: AnyProps; slotContent: Record<string, ReactNode> }) {
  const anchorRef = useRef<HTMLButtonElement | null>(null);
  const [open, setOpen] = useState(false);
  const name = component.library?.component ?? '';
  useEffect(() => setOpen(true), [component.id, name, props.title, props.kind, props.placement]);
  const placement = String(props.placement ?? (name === 'ActionBar' ? 'bottom' : 'top'));
  const palette = props.type === 'error' ? 'red' : props.type === 'success' ? 'green' : props.type === 'loading' ? 'orange' : 'blue';
  let content: ReactNode;
  if (name === 'ActionBar') {
    content = props.kind === 'bulk'
      ? <Flex className="chakra-showcase-actionbar" align="center" gap="2"><Badge size="lg" colorPalette="blue">{Number(props.selectedCount ?? 0)} 个页面</Badge><Button size="xs" colorPalette="blue"><Upload size={13} />发布</Button><Button size="xs" variant="outline">归档</Button><IconButton aria-label="删除" size="xs" variant="subtle" colorPalette="red"><Trash2 size={13} /></IconButton></Flex>
      : props.kind === 'layout'
        ? <Flex className="chakra-showcase-actionbar" align="center" gap="1"><Text paddingInline="2" textStyle="xs" color="fg.muted">{Number(props.selectedCount ?? 0)} 层</Text><IconButton aria-label="左对齐" size="xs" variant="ghost"><AlignLeftIcon /></IconButton><Button size="xs" variant="ghost"><GroupIcon />组合</Button><Button size="xs" variant="ghost"><Settings size={13} />锁定</Button></Flex>
        : <Flex className="chakra-showcase-actionbar" align="center" gap="2"><Badge colorPalette="purple">{Number(props.selectedCount ?? 0)} 项</Badge><Button size="xs" variant="outline"><CopyIcon />复制</Button><Button size="xs" variant="outline"><MoveIcon />移动</Button><IconButton aria-label="删除" size="xs" variant="subtle" colorPalette="red"><Trash2 size={13} /></IconButton></Flex>;
  } else if (name === 'FloatingPanel') {
    content = <Stack className="chakra-showcase-panel" gap="3"><Flex align="center" justify="space-between"><Flex align="center" gap="2"><GripHorizontal size={15} /><Text fontWeight="semibold">{String(props.title ?? '浮动面板')}</Text></Flex><Flex gap="1"><Minus size={13} /><Square size={13} /><X size={13} /></Flex></Flex>{slotContent.content ?? floatingPanelBody(props)}</Stack>;
  } else if (name === 'HoverCard') {
    content = slotContent.popup ?? hoverCardBody(props);
  } else if (name === 'OverlayManager') {
    content = overlayManagerBody(props);
  } else if (name === 'ToggleTip') {
    content = <Flex align="center" gap="2">{props.showArrow && <Text color="purple.500">▲</Text>}{slotContent.popup ?? toggleTipBody(props)}</Flex>;
  } else if (name === 'Portal') {
    content = portalBody(props);
  } else {
    content = <Flex align="start" gap="3"><Center width="7" height="7" flex="none" borderRadius="full" background={`${palette}.100`} color={`${palette}.700`}>{props.type === 'loading' ? '…' : props.type === 'error' ? '!' : '✓'}</Center><Stack gap="1" flex="1"><Text fontWeight="semibold">{String(props.title)}</Text><Text textStyle="sm" color="fg.muted">{String(props.description)}</Text></Stack>{props.closable && <CloseButton size="xs" onClick={() => setOpen(false)} />}</Flex>;
  }
  const trigger = name === 'ToggleTip' && props.kind === 'icon'
    ? <IconButton ref={anchorRef} aria-label="查看帮助" variant="outline" colorPalette="purple" onClick={() => setOpen((value) => !value)}><Info /></IconButton>
    : <Button ref={anchorRef} minWidth={name === 'ToggleTip' ? undefined : '9rem'} variant="outline" colorPalette={palette} onClick={() => setOpen((value) => !value)}>{name === 'ToggleTip' && props.kind === 'shortcut' ? <><Search />命令</> : name === 'ToggleTip' && props.kind === 'rich' ? <><Settings />锁定说明</> : component.content}</Button>;
  return <Center width="100%" height="100%">{trigger}<FloatingSurface anchorRef={anchorRef} open={open} placement={placement.startsWith('top') ? 'top' : placement.startsWith('bottom') ? 'bottom' : placement.endsWith('end') ? 'right' : 'left'} className={`chakra-showcase-surface kind-${name.toLowerCase()} size-${String(props.size ?? 'md')}`}>{content}</FloatingSurface></Center>;
}

function AlignLeftIcon() {
  return <Box width="14px"><Box width="12px" borderTopWidth="2px" /><Box width="8px" marginTop="3px" borderTopWidth="2px" /><Box width="11px" marginTop="3px" borderTopWidth="2px" /></Box>;
}

function GroupIcon() {
  return <Box width="14px" height="14px" position="relative"><Box position="absolute" inset="0 4px 4px 0" borderWidth="1px" borderRadius="2px" /><Box position="absolute" inset="4px 0 0 4px" borderWidth="1px" borderRadius="2px" /></Box>;
}

function CopyIcon() {
  return <GroupIcon />;
}

function MoveIcon() {
  return <Text lineHeight="1" fontSize="14px">↔</Text>;
}

function ListIndicatorIcon({ kind }: { kind: string }) {
  if (kind === 'check') return <CheckCircle2 size={18} strokeWidth={2.2} />;
  if (kind === 'info') return <Info size={18} strokeWidth={2.2} />;
  if (kind === 'circle') return <Circle size={10} strokeWidth={0} fill="currentColor" />;
  return null;
}

function ChakraList({ props }: { props: AnyProps }) {
  const variant = String(props.variant ?? 'marker');
  const indicator = String(props.indicator ?? 'none');
  const markerContent = typeof props.markerContent === 'string' ? props.markerContent : '';
  const markerStyle = markerContent || props.markerColor
    ? {
        color: props.markerColor ?? 'fg.subtle',
        ...(markerContent ? { content: `"${markerContent.replace(/["\\]/g, '\\$&')}"` } : {})
      }
    : undefined;

  const renderList = (value: unknown, nested = false): ReactNode => {
    const records = runtimeRecords(value as AnyProps['items']);
    const ordered = nested ? Boolean(props.nestedOrdered) : Boolean(props.ordered);
    return <List.Root
      as={ordered ? 'ol' : 'ul'}
      variant={variant as 'marker' | 'plain'}
      align={(props.align ?? 'start') as 'start' | 'center' | 'end'}
      unstyled={Boolean(props.unstyled)}
      gap={props.gap ?? 2}
      paddingStart={variant === 'marker' && !props.unstyled ? '5' : '0'}
    >
      {records.map((entry, index) => {
        const children = runtimeRecords(entry.children);
        const key = String(entry.key ?? entry.value ?? index);
        return <List.Item key={key} _marker={markerStyle}>
          {indicator !== 'none' && <List.Indicator asChild color={props.indicatorColor ?? 'blue.500'}><ListIndicatorIcon kind={indicator} /></List.Indicator>}
          <Box as="span">
            <Text as="span" fontWeight={entry.description ? 'medium' : 'normal'}>{String(entry.label ?? entry.title ?? entry.value ?? '')}</Text>
            {entry.description && <Text mt="1" textStyle="sm" color="fg.muted">{String(entry.description)}</Text>}
            {children.length > 0 && renderList(entry.children, true)}
          </Box>
        </List.Item>;
      })}
    </List.Root>;
  };

  return <Box width="100%" height="100%" padding="2">{renderList(props.items)}</Box>;
}

const ChakraProse = chakra('article', {
  base: {
    color: 'fg.muted',
    maxWidth: '65ch',
    lineHeight: '1.7',
    '& h1, & h2, & h3, & h4': { color: 'fg', fontWeight: '600', letterSpacing: '-0.02em' },
    '& h1': { fontSize: '2.15em', lineHeight: '1.2', marginBottom: '0.8em' },
    '& h2': { fontSize: '1.65em', lineHeight: '1.3', marginTop: '1.6em', marginBottom: '0.8em' },
    '& h3': { fontSize: '1.35em', lineHeight: '1.4', marginTop: '1.5em', marginBottom: '0.4em' },
    '& p': { marginTop: '1em', marginBottom: '1em' },
    '& strong': { color: 'fg', fontWeight: '600' },
    '& em': { fontStyle: 'italic' },
    '& a': { color: 'fg', fontWeight: '500', textDecoration: 'underline', textUnderlineOffset: '3px', textDecorationThickness: '2px' },
    '& blockquote': { color: 'fg', marginBlock: '1.285em', paddingInline: '1.285em', borderInlineStartWidth: '0.25em' },
    '& code': { background: 'bg.muted', borderWidth: '1px', borderRadius: 'md', paddingInline: '0.25em', fontSize: '0.925em' },
    '& ul, & ol': { marginBlock: '1em', paddingInlineStart: '1.5em' },
    '& ul > li': { listStyleType: 'disc', marginBlock: '0.285em' },
    '& ol > li': { listStyleType: 'decimal', marginBlock: '0.285em' },
    '& table': { width: '100%', marginBlock: '2em', textAlign: 'start', borderCollapse: 'collapse' },
    '& th': { color: 'fg', fontWeight: '600', textAlign: 'start', padding: '0.65em 1em', borderBottomWidth: '1px' },
    '& td': { padding: '0.65em 1em', borderBottomWidth: '1px' }
  }
});

const richTextCss = defineStyle({
  display: 'flex',
  flexDirection: 'column',
  height: '100%',
  overflow: 'hidden',
  borderWidth: '1px',
  borderRadius: 'lg',
  background: 'bg',
  '& .ProseMirror': {
    outline: 'none',
    minHeight: '100%',
    padding: '5',
    lineHeight: '1.65',
    '& > * + *': { marginTop: '0.75em' },
    '& h1': { fontSize: '2.15em', fontWeight: '600', lineHeight: '1.2' },
    '& h2': { fontSize: '1.65em', fontWeight: '600', lineHeight: '1.3' },
    '& h3': { fontSize: '1.35em', fontWeight: '600', lineHeight: '1.4' },
    '& code': { background: 'bg.muted', borderWidth: '1px', borderRadius: 'sm', paddingInline: '0.25em', fontFamily: 'mono' },
    '& pre': { background: 'gray.900', color: 'gray.100', padding: '4', borderRadius: 'lg', overflowX: 'auto' },
    '& pre code': { background: 'transparent', borderWidth: '0', padding: '0' },
    '& blockquote': { borderInlineStartWidth: '4px', borderColor: 'border', paddingInlineStart: '4' },
    '& ul': { listStyleType: 'disc', paddingInlineStart: '1.25rem' },
    '& ol': { listStyleType: 'decimal', paddingInlineStart: '1.25rem' }
  }
});

const richTextControlIcons: Record<string, ReactNode> = {
  bold: <Bold />, italic: <Italic />, strike: <Strikethrough />, code: <Braces />,
  h1: <Heading1 />, h2: <Heading2 />, h3: <Heading3 />, bullet: <ListIcon />, ordered: <ListOrdered />,
  quote: <Quote />, undo: <Undo2 />, redo: <Redo2 />
};

function ChakraRichTextEditor({ html, props, preview }: { html: string; props: AnyProps; preview: boolean }) {
  const [, setRevision] = useState(0);
  const editor = useEditor({
    extensions: [StarterKit],
    content: html,
    editable: preview && props.editable !== false,
    immediatelyRender: false,
    onUpdate: () => setRevision((value) => value + 1)
  });

  useEffect(() => {
    editor?.setEditable(preview && props.editable !== false);
  }, [editor, preview, props.editable]);

  useEffect(() => {
    if (editor && editor.getHTML() !== html) editor.commands.setContent(html);
  }, [editor, html]);

  const controls = runtimeStrings(props.toolbar);
  const run = (control: string) => {
    if (!editor || !preview || props.editable === false) return;
    const chain = editor.chain().focus();
    if (control === 'bold') chain.toggleBold().run();
    else if (control === 'italic') chain.toggleItalic().run();
    else if (control === 'strike') chain.toggleStrike().run();
    else if (control === 'code') chain.toggleCode().run();
    else if (control === 'h1') chain.toggleHeading({ level: 1 }).run();
    else if (control === 'h2') chain.toggleHeading({ level: 2 }).run();
    else if (control === 'h3') chain.toggleHeading({ level: 3 }).run();
    else if (control === 'bullet') chain.toggleBulletList().run();
    else if (control === 'ordered') chain.toggleOrderedList().run();
    else if (control === 'quote') chain.toggleBlockquote().run();
    else if (control === 'undo') chain.undo().run();
    else if (control === 'redo') chain.redo().run();
  };
  const active = (control: string) => {
    if (!editor) return false;
    if (control === 'h1' || control === 'h2' || control === 'h3') return editor.isActive('heading', { level: Number(control.slice(1)) });
    if (control === 'bullet') return editor.isActive('bulletList');
    if (control === 'ordered') return editor.isActive('orderedList');
    if (control === 'quote') return editor.isActive('blockquote');
    return editor.isActive(control === 'strike' ? 'strike' : control);
  };

  return <Box css={richTextCss}>
    {controls.length > 0 && <Flex gap="1" flexWrap="wrap" padding="2" borderBottomWidth="1px" background="bg.panel">
      {controls.map((control) => <IconButton
        key={control}
        aria-label={control}
        title={control}
        size="xs"
        variant={active(control) ? 'subtle' : 'ghost'}
        colorPalette={active(control) ? 'blue' : 'gray'}
        disabled={!preview || props.editable === false || ((control === 'undo' || control === 'redo') && !editor?.can()[control]())}
        onClick={() => run(control)}
      >{richTextControlIcons[control] ?? control}</IconButton>)}
    </Flex>}
    <Box flex="1" minHeight="0" overflow="auto"><EditorContent editor={editor} /></Box>
    {props.showFooter && <Flex justify="space-between" padding="2 3" borderTopWidth="1px" color="fg.muted" textStyle="xs">
      <Text>{props.editable === false ? '只读模式' : '编辑模式'}</Text>
      <Text>{editor?.getText().length ?? 0} 字符</Text>
    </Flex>}
  </Box>;
}

const frameworkOptions = [
  { label: 'React', value: 'react' }, { label: 'Vue', value: 'vue' }, { label: 'Angular', value: 'angular' },
  { label: 'Svelte', value: 'svelte' }, { label: 'Next.js', value: 'nextjs' }
];
const peopleOptions = [
  { label: '林设计师', value: 'lin' }, { label: '王产品', value: 'wang' }, { label: 'AI 助手', value: 'ai' }, { label: '陈前端', value: 'chen' }
];
const sizeOptions = [{ label: '小', value: 'small' }, { label: '中', value: 'medium' }, { label: '大', value: 'large' }];
const statusOptions = [{ label: '准备中', value: 'draft' }, { label: '可用', value: 'ready' }, { label: '已发布', value: 'published' }];
const pageOptions = [{ label: '落地页', value: 'landing' }, { label: '控制台', value: 'dashboard' }, { label: '内容站点', value: 'content' }];

function optionsForKind(kind: unknown) {
  if (kind === 'people') return peopleOptions;
  if (kind === 'sizes') return sizeOptions;
  if (kind === 'status') return statusOptions;
  if (kind === 'pages') return pageOptions;
  return frameworkOptions;
}

function ChakraPasswordInput({ props, preview }: { props: AnyProps; preview: boolean }) {
  const [visible, setVisible] = useState(Boolean(props.defaultVisible));
  useEffect(() => setVisible(Boolean(props.defaultVisible)), [props.defaultVisible]);
  return <Stack width="100%" gap="2">
    <InputGroup endElement={<IconButton aria-label="切换密码可见性" size="sm" variant="ghost" disabled={!preview} onPointerDown={(event) => { event.preventDefault(); if (preview) setVisible((value) => !value); }}>{visible ? <EyeOff /> : <Eye />}</IconButton>}>
      <Input type={visible ? 'text' : 'password'} defaultValue="Design@2026" placeholder={String(props.placeholder ?? '请输入密码')} size={props.size} variant={props.variant} readOnly={!preview} />
    </InputGroup>
    {props.showStrength && <Stack gap="1"><Flex gap="1">{Array.from({ length: 4 }, (_, index) => <Box key={index} height="1" flex="1" borderRadius="full" background={index < Number(props.strength ?? 0) ? (Number(props.strength) >= 3 ? 'green.500' : 'orange.500') : 'gray.200'} />)}</Flex><Text textStyle="xs" color="fg.muted" textAlign="right">密码强度：{Number(props.strength ?? 0) >= 3 ? '强' : '一般'}</Text></Stack>}
  </Stack>;
}

function datePickerValues(props: AnyProps) {
  const configured = runtimeStrings(props.defaultValue);
  const fallback = props.selectionMode === 'range'
    ? ['2026-09-11', '2026-09-20']
    : props.selectionMode === 'multiple'
      ? ['2026-09-03', '2026-09-11', '2026-09-21']
      : [props.size === 'lg' ? '2026-09-13' : '2026-09-11'];
  return (configured.length > 0 ? configured : fallback).flatMap((value) => {
    try {
      return [parseDate(value)];
    } catch {
      return [];
    }
  });
}

function formatPickerDate(value: { year: number; month: number; day: number }) {
  return `${value.year}/${String(value.month).padStart(2, '0')}/${String(value.day).padStart(2, '0')}`;
}

function ChakraDatePicker({ props, preview, showcase }: { props: AnyProps; preview: boolean; showcase: boolean }) {
  const selectionMode = (props.selectionMode ?? 'single') as 'single' | 'range' | 'multiple';
  const initialValues = datePickerValues(props);
  const valueKey = initialValues.map((value) => value.toString()).join('|');
  const [value, setValue] = useState<ReturnType<typeof datePickerValues>[number][]>(initialValues);

  useEffect(() => setValue(datePickerValues(props)), [valueKey, selectionMode]);

  const calendar = <DatePicker.View view="day"><DatePicker.Header /><DatePicker.DayTable /></DatePicker.View>;
  const inputControl = selectionMode === 'range'
    ? <Stack gap="1.5" width="100%">
        <Flex gap="2" align="end">
          <Box flex="1" minWidth="0"><Text textStyle="xs" color="fg.muted" marginBottom="1">开始日期</Text><DatePicker.Input index={0} /></Box>
          <Text color="fg.muted" paddingBottom={props.size === 'lg' ? '3' : '2.5'}>至</Text>
          <Box flex="1" minWidth="0"><Text textStyle="xs" color="fg.muted" marginBottom="1">结束日期</Text><DatePicker.Input index={1} /></Box>
          <DatePicker.Trigger alignSelf="end" marginBottom={props.size === 'lg' ? '2.5' : '2'}><CalendarDays /></DatePicker.Trigger>
        </Flex>
      </Stack>
    : <DatePicker.Control>
        <DatePicker.Input index={0} />
        <DatePicker.IndicatorGroup><DatePicker.Trigger><CalendarDays /></DatePicker.Trigger></DatePicker.IndicatorGroup>
      </DatePicker.Control>;

  return <DatePicker.Root
    width="100%"
    disabled={!preview}
    locale={String(props.locale ?? 'zh-CN')}
    size={props.size ?? 'md'}
    variant={props.variant ?? 'outline'}
    colorPalette={props.colorPalette ?? 'purple'}
    selectionMode={selectionMode}
    closeOnSelect={props.closeOnSelect !== false}
    maxSelectedDates={Number(props.maxSelectedDates ?? 6)}
    defaultFocusedValue={initialValues[0] ?? parseDate('2026-09-01')}
    value={value}
    onValueChange={(details) => setValue(details.value.map((date) => parseDate(date.toString().slice(0, 10))))}
    inline={showcase}
    openOnClick={preview}
  >
    <DatePicker.Label>{String(props.label ?? '选择日期')}</DatePicker.Label>
    {inputControl}
    {selectionMode === 'multiple' && <Flex gap="1.5" flexWrap="wrap" minHeight="5">
      {value.map((date) => <Tag.Root key={date.toString()} size="sm" variant="subtle" colorPalette={props.colorPalette ?? 'teal'}><Tag.Label>{formatPickerDate(date)}</Tag.Label></Tag.Root>)}
    </Flex>}
    {showcase
      ? <Box marginTop="2" padding="3" borderWidth="1px" borderRadius="xl" background="bg.panel" boxShadow="sm">{calendar}</Box>
      : <Portal><DatePicker.Positioner><DatePicker.Content>{calendar}<DatePicker.View view="month"><DatePicker.Header /><DatePicker.MonthTable /></DatePicker.View><DatePicker.View view="year"><DatePicker.Header /><DatePicker.YearTable /></DatePicker.View></DatePicker.Content></DatePicker.Positioner></Portal>}
  </DatePicker.Root>;
}

function ChakraDateInput({ props, preview }: { props: AnyProps; preview: boolean }) {
  const range = props.selectionMode === 'range';
  const rawValues = Array.isArray(props.defaultValue) ? props.defaultValue.map(String) : [String(props.defaultValue ?? (props.granularity === 'minute' ? '2026-09-04T14:30' : '2026-09-04'))];
  const values = rawValues.map((value) => props.granularity === 'minute' ? parseDateTime(value) : parseDate(value));
  return <DateInput.Root width="100%" disabled={!preview} locale={String(props.locale ?? 'zh-CN')} granularity={props.granularity ?? 'day'} selectionMode={range ? 'range' : 'single'} size={props.size} defaultValue={values}>
    <DateInput.Label>{String(props.label ?? '选择日期')}</DateInput.Label>
    {range
      ? <Flex width="100%" gap="2" align="center"><DateInput.Control flex="1"><DateInput.Segments index={0} /></DateInput.Control><Text color="fg.muted">至</Text><DateInput.Control flex="1"><DateInput.Segments index={1} /></DateInput.Control></Flex>
      : <DateInput.Control><DateInput.Segments /></DateInput.Control>}
    <DateInput.HiddenInput />
  </DateInput.Root>;
}

function ChakraColorPicker({ props, preview, showcase }: { props: AnyProps; preview: boolean; showcase: boolean }) {
  const sliders = props.showAlpha
    ? <ColorPicker.Sliders />
    : <ColorPicker.ChannelSlider channel="hue" flex="1" paddingX="1"><ColorPicker.ChannelSliderTrack /><ColorPicker.ChannelSliderThumb /></ColorPicker.ChannelSlider>;
  const content = <ColorPicker.Content position={showcase ? 'relative' : undefined} inset={showcase ? 'auto' : undefined} width="100%">
    <ColorPicker.Area height={props.size === 'sm' ? '32' : props.size === 'lg' ? '48' : '40'} />
    <Flex gap="2" align="center"><ColorPicker.EyeDropper size="xs" variant="outline" />{sliders}</Flex>
  </ColorPicker.Content>;
  return <ColorPicker.Root width="100%" disabled={!preview} defaultValue={parseColor(String(props.defaultValue ?? '#5D50DF'))} format={props.format ?? 'rgba'} size={props.size} inline={showcase} open={showcase ? true : undefined}>
    <ColorPicker.HiddenInput /><ColorPicker.Label>{String(props.label ?? '选择颜色')}</ColorPicker.Label><ColorPicker.Control><ColorPicker.Input /><ColorPicker.Trigger /></ColorPicker.Control>
    {showcase ? <Box marginTop="2">{content}</Box> : <Portal><ColorPicker.Positioner>{content}</ColorPicker.Positioner></Portal>}
  </ColorPicker.Root>;
}

function ChakraCombobox({ props, preview, showcase }: { props: AnyProps; preview: boolean; showcase: boolean }) {
  const initialItems = optionsForKind(props.dataKind);
  const [items, setItems] = useState(initialItems);
  const configuredValue = runtimeStrings(props.defaultValue);
  const [value, setValue] = useState(configuredValue);
  useEffect(() => setItems(initialItems), [props.dataKind]);
  useEffect(() => setValue(runtimeStrings(props.defaultValue)), [JSON.stringify(props.defaultValue)]);
  const collection = createListCollection({ items });
  const selectedItems = initialItems.filter((item) => value.includes(item.value));
  const content = <Combobox.Content position={showcase ? 'relative' : undefined} inset={showcase ? 'auto' : undefined} width="100%"><Combobox.Empty>没有匹配项</Combobox.Empty>{collection.items.map((item) => <Combobox.Item key={item.value} item={item}>{item.label}<Combobox.ItemIndicator /></Combobox.Item>)}</Combobox.Content>;
  return <Combobox.Root collection={collection} multiple={Boolean(props.multiple)} disabled={!preview} width="100%" size={props.size} value={value} onValueChange={(details) => setValue(details.value)} open={showcase ? true : undefined} onInputValueChange={(details) => {
    const query = details.inputValue.trim().toLowerCase();
    setItems(query ? initialItems.filter((item) => item.label.toLowerCase().includes(query)) : initialItems);
  }}>
    <Combobox.Label>{String(props.label ?? '选择项目')}</Combobox.Label>
    <Combobox.Control><Combobox.Input placeholder={String(props.placeholder ?? '输入并搜索')} /><Combobox.IndicatorGroup><Combobox.ClearTrigger /><Combobox.Trigger /></Combobox.IndicatorGroup></Combobox.Control>
    {props.multiple && selectedItems.length > 0 && <Flex gap="1.5" flexWrap="wrap">{selectedItems.map((item) => <Tag.Root key={item.value} size="sm" colorPalette="purple" variant="subtle"><Tag.Label>{item.label}</Tag.Label></Tag.Root>)}</Flex>}
    {showcase ? <Box marginTop="2">{content}</Box> : <Portal><Combobox.Positioner>{content}</Combobox.Positioner></Portal>}
  </Combobox.Root>;
}

const fileTree = createTreeCollection({
  nodeToValue: (node: AnyProps) => String(node.id),
  nodeToString: (node: AnyProps) => String(node.name),
  rootNode: {
    id: 'ROOT', name: '', children: [
      { id: 'src', name: 'src', children: [{ id: 'src/components', name: 'components', children: [{ id: 'src/components/button.tsx', name: 'button.tsx' }, { id: 'src/components/form.tsx', name: 'form.tsx' }] }, { id: 'src/app.tsx', name: 'app.tsx' }, { id: 'src/styles.css', name: 'styles.css' }] },
      { id: 'public', name: 'public', children: [{ id: 'public/logo.svg', name: 'logo.svg' }] },
      { id: 'package.json', name: 'package.json' }
    ]
  }
});

export function ChakraCanvasComponent({ component, preview, showcase = false, tokens, slotContent = {} }: {
  component: WebDesignComponent;
  preview: boolean;
  showcase?: boolean;
  tokens?: WebDesignTokens;
  slotContent?: Record<string, ReactNode>;
}) {
  const scope = designStyleScopeProps(component.style);
  return <ChakraProvider value={defaultSystem}><Box {...scope} color={component.style.color} fontFamily={tokens?.typography.fontFamily}>
    <ChakraCanvasRenderer component={component} preview={preview} showcase={showcase} slotContent={slotContent} />
  </Box></ChakraProvider>;
}

function ChakraCanvasRenderer({ component, preview, showcase, slotContent }: { component: WebDesignComponent; preview: boolean; showcase: boolean; slotContent: Record<string, ReactNode> }) {
  const binding = component.library!;
  const p = binding.props as AnyProps;
  const name = binding.component;
  const triggerRef = useRef<HTMLButtonElement | null>(null);
  const hoverRef = useRef<HTMLButtonElement | null>(null);
  const [open, setOpen] = useState(false);
  const [visible, setVisible] = useState(Boolean(p.present));
  const [counter, setCounter] = useState(Number(p.initialCount ?? 0));
  useEffect(() => setVisible(Boolean(p.present) || (showcase && name === 'Presence')), [p.present, showcase, name]);
  useEffect(() => setCounter(Number(p.initialCount ?? 0)), [p.initialCount]);
  useEffect(() => {
    if (showcase && ['Dialog', 'Drawer', 'Popover', 'Tooltip', 'Menu'].includes(name)) setOpen(true);
  }, [showcase, name]);
  const fill = { width: '100%', height: '100%' };
  const items = runtimeRecords(p.items);

  if (showcase && chakraPortalShowcaseComponents.has(name)) return <ChakraPortalShowcase component={component} props={p} slotContent={slotContent} />;

  if (name === 'AspectRatio') return <AspectRatio width="100%" ratio={Number(p.ratio ?? 16 / 9)} borderRadius="lg" overflow="hidden" background={p.background ?? 'gray.900'}>
    {slotContent.content ?? <Flex direction="column" align="center" justify="center" color={p.kind === 'video' ? 'white' : 'blue.700'} gap="2"><Play fill="currentColor" /><Text fontWeight="medium">{p.kind === 'photo' ? '图片比例区域' : p.kind === 'square' ? '方形媒体区域' : '视频比例区域'}</Text></Flex>}
  </AspectRatio>;
  if (name === 'Bleed') return <Box {...fill} padding="6" borderWidth="1px" borderRadius="lg" overflow="hidden" background="bg.subtle">
    <Bleed inline={p.inline} block={p.block} inlineStart={p.inlineStart} inlineEnd={p.inlineEnd} blockStart={p.blockStart} blockEnd={p.blockEnd}>
      <Box minHeight="20" padding="4" background={`${p.colorPalette ?? 'blue'}.100`} color={`${p.colorPalette ?? 'blue'}.800`} display="grid" placeItems="center" fontWeight="medium">{slotContent.content ?? '内容突破容器内边距'}</Box>
    </Bleed>
  </Box>;
  if (name === 'AbsoluteCenter') return <Box {...fill} position="relative" borderWidth="1px" borderRadius="lg" background="bg.subtle">
    <Box position="absolute" insetX="4" top="50%" borderTopWidth="1px" borderStyle="dashed" borderColor="border" />
    <Box position="absolute" insetY="4" left="50%" borderLeftWidth="1px" borderStyle="dashed" borderColor="border" />
    <AbsoluteCenter axis={p.axis ?? 'both'}><Badge colorPalette={p.colorPalette ?? 'blue'} padding="2 3">{slotContent.content ?? component.content}</Badge></AbsoluteCenter>
  </Box>;
  if (name === 'Center') return <Center {...fill} inline={Boolean(p.inline)} borderWidth="1px" borderRadius="lg" background="bg.subtle">
    {slotContent.content ?? (p.contentKind === 'avatar' ? <Avatar.Root size="xl" colorPalette={p.colorPalette ?? 'teal'}><Avatar.Fallback name="AI Designer" /></Avatar.Root> : <Button colorPalette={p.colorPalette ?? 'blue'}>{component.content}</Button>)}
  </Center>;
  if (name === 'Float') return <Box {...fill} position="relative" borderWidth="1px" borderRadius="lg" padding="4" background="bg.subtle">
    {slotContent.content ?? <><Heading size="sm">产品更新</Heading><Text mt="2" textStyle="sm" color="fg.muted">浮标会依附在容器边缘。</Text></>}
    <Float placement={p.placement ?? 'top-end'} offset={p.offset ?? 3}><Badge colorPalette={p.colorPalette ?? 'red'} variant="solid" borderRadius="full">{component.content || '新'}</Badge></Float>
  </Box>;
  if (name === 'Wrap') return <Wrap {...fill} justify={p.justify} align={p.align} direction={p.direction} gap={p.gap ?? 3} padding="3" borderWidth="1px" borderRadius="lg" overflow="hidden">
    {slotContent.content ?? Array.from({ length: Number(p.itemCount ?? 8) }, (_, index) => <WrapItem key={index}><Badge size="lg" variant={index % 2 ? 'surface' : 'subtle'} colorPalette={['blue', 'purple', 'teal', 'orange'][index % 4]}>标签 {index + 1}</Badge></WrapItem>)}
  </Wrap>;
  if (name === 'Box') return <Box {...fill} padding={p.padding ?? 4} borderWidth={p.borderWidth ?? 1} borderRadius={p.borderRadius ?? 'lg'} bg={p.background ?? 'white'} shadow={p.shadow}>{slotContent.content ?? <Text color="fg.muted">Box 内容区域</Text>}</Box>;
  if (name === 'Container') {
    const showcaseWidth = p.fluid ? '100%' : p.maxWidth === 'md' ? '58%' : p.maxWidth === '4xl' ? '84%' : '72%';
    return <Box {...fill} display="flex" justifyContent="center" alignItems="stretch" padding={showcase ? '2' : '0'} border={showcase ? '1px dashed' : undefined} borderColor={showcase ? 'border' : undefined} borderRadius="lg" background={showcase ? 'bg.subtle' : undefined}><Container width={showcase ? showcaseWidth : '100%'} height="100%" maxW={p.fluid ? 'full' : p.maxWidth ?? 'lg'} centerContent={p.centerContent} fluid={p.fluid} padding="4" borderWidth="1px" borderRadius="lg" background="bg.panel">{slotContent.content ?? <Stack gap="2" align={p.centerContent ? 'center' : 'stretch'} width="100%"><Badge alignSelf={p.centerContent ? 'center' : 'start'} colorPalette={p.fluid ? 'green' : p.maxWidth === 'md' ? 'purple' : 'blue'}>{p.fluid ? '100% 流式' : p.maxWidth === 'md' ? '窄内容' : '宽内容'}</Badge><Text color="fg.muted">Container 内容区域</Text></Stack>}</Container></Box>;
  }
  if (name === 'Flex') return <Flex {...fill} direction={p.direction} gap={p.gap} align={p.align} justify={p.justify} wrap={p.wrap} padding="3" borderWidth="1px" borderRadius="lg">{slotContent.content ?? <><Button size="sm">取消</Button><Button size="sm" colorPalette="blue">确定</Button><Badge>标签</Badge></>}</Flex>;
  if (name === 'Grid') {
    const columns = Math.max(1, Number(p.columns ?? 3));
    return <Grid {...fill} templateColumns={`repeat(${columns}, minmax(0, 1fr))`} gap={p.gap ?? 4} padding="3" borderWidth="1px" borderRadius="lg">{slotContent.content ?? Array.from({ length: columns }, (_, index) => <Box key={index} borderRadius="md" bg="blue.50" color="blue.700" display="grid" placeItems="center">{index + 1}</Box>)}</Grid>;
  }
  if (name === 'SimpleGrid') {
    const columns = Math.max(1, Number(p.columns ?? 3));
    return <SimpleGrid {...fill} columns={columns} gap={p.gap ?? 4} padding="3" borderWidth="1px" borderRadius="lg">{slotContent.content ?? Array.from({ length: columns }, (_, index) => <Box key={index} borderRadius="md" bg="purple.50" color="purple.700" display="grid" placeItems="center">{index + 1}</Box>)}</SimpleGrid>;
  }
  if (name === 'Stack') return <Stack {...fill} direction={p.direction ?? 'column'} gap={p.gap ?? 4} align={p.align} padding="3" borderWidth="1px" borderRadius="lg">{slotContent.content ?? <><Button size="sm" variant="outline">第一项</Button><Button size="sm" variant="outline">第二项</Button><Button size="sm" variant="outline">第三项</Button></>}</Stack>;
  if (name === 'Group') return <Group width="100%" height="100%" attached={Boolean(p.attached)} orientation={p.orientation ?? 'horizontal'} padding="2">{slotContent.content ?? <><Button size="sm">上一页</Button><Button size="sm" variant="outline">下一页</Button></>}</Group>;
  if (name === 'Separator') return <Flex {...fill} align="center" justify="center"><Separator width={p.orientation === 'vertical' ? undefined : '100%'} height={p.orientation === 'vertical' ? '100%' : undefined} orientation={p.orientation ?? 'horizontal'} variant={p.variant ?? 'solid'} size={p.size ?? 'sm'} /></Flex>;
  if (name === 'ScrollArea') return <ScrollArea.Root {...fill} variant={p.variant ?? 'hover'} size={p.size ?? 'md'} borderWidth="1px" borderRadius="lg"><ScrollArea.Viewport><ScrollArea.Content padding="3">{slotContent.content ?? <Stack gap="3">{Array.from({ length: Number(p.itemCount ?? 9) }, (_, index) => <Box key={index} padding="3" borderWidth="1px" borderRadius="md"><Text fontWeight="medium">滚动内容 {index + 1}</Text><Text textStyle="sm" color="fg.muted">这是可以滚动查看的 Chakra 内容。</Text></Box>)}</Stack>}</ScrollArea.Content></ScrollArea.Viewport><ScrollArea.Scrollbar><ScrollArea.Thumb /></ScrollArea.Scrollbar><ScrollArea.Corner /></ScrollArea.Root>;
  if (name === 'Splitter') {
    const vertical = p.orientation === 'vertical';
    return <Splitter.Root {...fill} orientation={vertical ? 'vertical' : 'horizontal'} panels={[{ id: 'panel-1', minSize: 20 }, { id: 'panel-2', minSize: 20 }]} defaultSize={Array.isArray(p.defaultSizes) ? p.defaultSizes.map(Number) : [45, 55]} borderWidth="1px" borderRadius="lg" overflow="hidden"><Splitter.Panel id="panel-1"><Box {...fill} padding="3" bg="gray.50">{slotContent['panel-1'] ?? '面板一'}</Box></Splitter.Panel><Splitter.ResizeTrigger id="panel-1:panel-2"><Splitter.ResizeTriggerSeparator /><Splitter.ResizeTriggerIndicator /></Splitter.ResizeTrigger><Splitter.Panel id="panel-2"><Box {...fill} padding="3">{slotContent['panel-2'] ?? '面板二'}</Box></Splitter.Panel></Splitter.Root>;
  }

  if (name === 'Heading') return <Heading as={`h${Math.min(6, Math.max(1, Number(p.level ?? 2)))}` as any} size={p.size ?? '2xl'}>{component.content}</Heading>;
  if (name === 'Text') return <Text textStyle={p.textStyle ?? 'md'} color={p.color ?? 'fg'}>{component.content}</Text>;
  if (name === 'Code') return <Code variant={p.variant} colorPalette={p.colorPalette} size={p.size} padding="2" borderRadius="md">{component.content}</Code>;
  if (name === 'CodeBlock') return <CodeBlock.AdapterProvider value={plainTextAdapter}>
    <CodeBlock.Root {...fill} code={component.content} language={String(p.language ?? 'tsx')} maxLines={Number(p.maxLines ?? 0) > 0 ? Number(p.maxLines) : undefined} meta={{ showLineNumbers: Boolean(p.showLineNumbers), wordWrap: Boolean(p.wordWrap) }}>
      {p.showHeader && <CodeBlock.Header><CodeBlock.Title>{String(p.title ?? 'code.tsx')}</CodeBlock.Title><CodeBlock.Control><CodeBlock.CopyTrigger asChild><IconButton aria-label="复制代码" size="xs" variant="ghost"><Braces /></IconButton></CodeBlock.CopyTrigger></CodeBlock.Control></CodeBlock.Header>}
      <CodeBlock.Content><CodeBlock.Code><CodeBlock.CodeText /></CodeBlock.Code></CodeBlock.Content>
    </CodeBlock.Root>
  </CodeBlock.AdapterProvider>;
  if (name === 'Em') return <Text textStyle="lg" color={p.color ?? 'fg'}><Em>{component.content}</Em></Text>;
  if (name === 'Highlight') return <Text textStyle="lg" lineHeight="1.8"><Highlight query={runtimeStrings(p.query)} ignoreCase={p.ignoreCase !== false} styles={p.treatment === 'underline' ? { textDecoration: 'underline', textDecorationThickness: '3px', textUnderlineOffset: '4px', textDecorationColor: `${p.colorPalette ?? 'purple'}.400` } : { background: `${p.colorPalette ?? 'yellow'}.200`, color: `${p.colorPalette ?? 'yellow'}.900`, paddingInline: '1', borderRadius: 'sm' }}>{component.content}</Highlight></Text>;
  if (name === 'LinkOverlay') return <LinkBox {...fill} as="article" position="relative" padding="5" borderWidth="1px" borderRadius="xl" background="bg.panel" shadow={p.variant === 'article' ? 'sm' : undefined} _hover={{ borderColor: 'blue.300', shadow: 'md' }}>
    {slotContent.content ? <><LinkOverlay position="absolute" inset="0" href={String(p.href ?? '#')} aria-label={component.content} onClick={(event) => event.preventDefault()} />{slotContent.content}</> : <><Badge colorPalette="blue" variant="subtle">{p.variant === 'article' ? '设计文章' : '产品能力'}</Badge>
      <Heading size="md" mt="3"><LinkOverlay href={String(p.href ?? '#')} target={p.external ? '_blank' : undefined} onClick={(event) => event.preventDefault()}>{component.content}</LinkOverlay></Heading>
      <Text mt="2" color="fg.muted" textStyle="sm">点击卡片任意区域都可以激活主链接{p.external ? ' ↗' : ''}</Text></>}
  </LinkBox>;
  if (name === 'Mark') return <Text textStyle="lg">这里是 <Mark colorPalette={p.colorPalette ?? 'yellow'} variant={p.variant ?? 'subtle'}>{component.content}</Mark>，可用于强调关键信息。</Text>;
  if (name === 'Prose') return <Box {...fill} overflow="auto" padding="5" borderWidth="1px" borderRadius="lg"><ChakraProse maxWidth={p.maxWidth ?? '65ch'} fontSize={p.size === 'lg' ? 'md' : 'sm'}>
    <h1>为真实产品而设计</h1>
    <p>优秀的网站设计不仅要好看，还要让信息层级、交互反馈与<strong>用户目标</strong>保持一致。</p>
    <h2>设计原则</h2>
    <p>通过 <a href="#prose" onClick={(event) => event.preventDefault()}>Chakra UI</a> 的组合能力，AI 和设计师可以共同构建稳定、清晰的界面。</p>
    <blockquote>设计出来是什么样，保存和刷新之后就应该仍然是什么样。</blockquote>
    <ul><li>一致的视觉语言</li><li>真实可操作的组件</li><li>可靠的响应式布局</li></ul>
    {p.showTable && <table><thead><tr><th>能力</th><th>状态</th></tr></thead><tbody><tr><td>布局保存</td><td>稳定</td></tr><tr><td>组件交互</td><td>可用</td></tr></tbody></table>}
  </ChakraProse></Box>;
  if (name === 'RichTextEditor') return <ChakraRichTextEditor html={component.content} props={p} preview={preview} />;
  if (name === 'Blockquote') return <Blockquote.Root {...fill} variant={p.variant ?? 'subtle'} justify={p.justify ?? 'start'} colorPalette={p.colorPalette ?? 'blue'} padding="3"><Blockquote.Content>{component.content}</Blockquote.Content><Blockquote.Caption>— {String(p.cite ?? 'Web Design Studio')}</Blockquote.Caption></Blockquote.Root>;
  if (name === 'Kbd') return <Kbd variant={p.variant} size={p.size}>{component.content}</Kbd>;
  if (name === 'Link') return <Link colorPalette={p.colorPalette ?? 'blue'} variant={p.variant ?? 'underline'} textDecoration={p.variant === 'plain' ? 'none' : 'underline'} textUnderlineOffset="3px" fontWeight={p.variant === 'plain' ? 'medium' : 'normal'} target={p.external ? '_blank' : undefined}>{component.content}{p.external ? ' ↗' : ''}</Link>;
  if (name === 'List') return <ChakraList props={p} />;

  if (name === 'Button') return <Button width="100%" height="100%" variant={p.variant} colorPalette={p.colorPalette} size={p.size}>{component.content}</Button>;
  if (name === 'IconButton') return <IconButton width="100%" height="100%" variant={p.variant} colorPalette={p.colorPalette} size={p.size} borderWidth={p.variant === 'outline' || p.variant === 'surface' ? '1px' : '0'} borderColor={p.variant === 'outline' ? 'gray.400' : undefined} background={p.variant === 'outline' ? 'white' : p.variant === 'ghost' || p.variant === 'plain' ? 'transparent' : undefined} shadow={p.variant === 'surface' ? 'sm' : undefined} aria-label="快捷操作">{component.content}</IconButton>;
  if (name === 'CloseButton') return <CloseButton width="100%" height="100%" variant={p.variant} colorPalette={p.colorPalette} size={p.size} borderWidth={p.variant === 'outline' ? '1px' : '0'} borderColor={p.variant === 'outline' ? 'gray.400' : undefined} background={p.variant === 'outline' ? 'white' : undefined} aria-label="关闭" />;
  if (name === 'DownloadTrigger') return <DownloadTrigger asChild fileName={String(p.fileName ?? 'download.txt')} mimeType={p.mimeType ?? 'text/plain'} data={String(p.data ?? '')} onClick={(event) => { if (!preview) event.preventDefault(); }}><Button width="100%" height="100%" variant={p.variant} colorPalette={p.colorPalette}><Download />{component.content}</Button></DownloadTrigger>;
  if (name === 'DateInput') return <ChakraDateInput props={p} preview={preview} />;
  if (name === 'DatePicker') return <ChakraDatePicker props={p} preview={preview} showcase={showcase} />;
  if (name === 'Calendar') {
    const defaultValue = p.selectionMode === 'range'
      ? [parseDate('2026-09-03'), parseDate('2026-09-09')]
      : p.selectionMode === 'multiple'
        ? [parseDate('2026-09-03'), parseDate('2026-09-12'), parseDate('2026-09-21')]
        : [parseDate('2026-09-03')];
    return <Box width="100%" height="100%" position="relative"><DatePicker.Root inline width="100%" height="100%" disabled={!preview} locale={String(p.locale ?? 'zh-CN')} size={p.size} selectionMode={p.selectionMode ?? 'single'} defaultFocusedValue={parseDate('2026-09-01')} defaultValue={defaultValue} hideOutsideDays={Boolean(p.hideOutsideDays)} showWeekNumbers={Boolean(p.showWeekNumbers)}>
      <DatePicker.View view="day"><DatePicker.Header /><DatePicker.DayTable /></DatePicker.View><DatePicker.View view="month"><DatePicker.Header /><DatePicker.MonthTable /></DatePicker.View><DatePicker.View view="year"><DatePicker.Header /><DatePicker.YearTable /></DatePicker.View>
    </DatePicker.Root>{showcase && <Badge position="absolute" right="2" bottom="2" colorPalette={p.selectionMode === 'range' ? 'blue' : p.selectionMode === 'multiple' ? 'purple' : 'gray'}>{p.selectionMode === 'range' ? '起止范围' : p.selectionMode === 'multiple' ? '多个日期' : '单个日期'}</Badge>}</Box>;
  }
  if (name === 'CheckboxCard') return <CheckboxCard.Root width="100%" height="100%" defaultChecked={Boolean(p.defaultChecked)} disabled={!preview} colorPalette={p.colorPalette} variant={p.variant} size={p.size}>
    <CheckboxCard.HiddenInput /><CheckboxCard.Control><Box flex="1"><CheckboxCard.Label>{String(p.title ?? '选择项目')}</CheckboxCard.Label><CheckboxCard.Description>{String(p.description ?? '')}</CheckboxCard.Description></Box><CheckboxCard.Indicator /></CheckboxCard.Control>
  </CheckboxCard.Root>;
  if (name === 'ColorPicker') return <ChakraColorPicker props={p} preview={preview} showcase={showcase} />;
  if (name === 'ColorSwatch') return <Center {...fill}><Box position="relative"><ColorSwatch value={String(p.value ?? '#5D50DF')} size={p.size} borderRadius={p.shape === 'full' ? 'full' : 'lg'} />{p.showCheck && <Center position="absolute" inset="0" color="white"><Check size={20} /></Center>}</Box></Center>;
  if (name === 'Field') return <Field.Root width="100%" required={Boolean(p.required)} invalid={Boolean(p.invalid)} disabled={Boolean(p.disabled) || !preview}>
    <Field.Label>{String(p.label ?? '字段')}<Field.RequiredIndicator /></Field.Label><Input placeholder={component.content} readOnly={!preview} />{p.helperText && <Field.HelperText>{String(p.helperText)}</Field.HelperText>}{p.errorText && <Field.ErrorText>{String(p.errorText)}</Field.ErrorText>}
  </Field.Root>;
  if (name === 'FileUpload') return <FileUpload.Root width="100%" maxFiles={Number(p.maxFiles ?? 1)} accept={String(p.accept ?? '*/*').split(',')} defaultAcceptedFiles={typeof globalThis.File === 'undefined' ? [] : runtimeStrings(p.sampleFiles).map((fileName, index) => new globalThis.File([`design-preview-${index}`], fileName, { type: 'image/png' }))} disabled={!preview}>
    <FileUpload.HiddenInput />{p.kind === 'dropzone' ? <FileUpload.Dropzone height="100%" minHeight="32"><Upload /><FileUpload.DropzoneContent><Text fontWeight="medium">{String(p.label ?? '拖入文件或点击选择')}</Text><Text textStyle="xs" color="fg.muted">支持 {String(p.accept ?? '*/*')} · 最多 {Number(p.maxFiles ?? 1)} 个</Text></FileUpload.DropzoneContent></FileUpload.Dropzone> : <><FileUpload.Trigger asChild><Button variant="outline"><Upload />{String(p.label ?? '上传文件')}</Button></FileUpload.Trigger><FileUpload.List /></>}
  </FileUpload.Root>;
  if (name === 'NumberInput') {
    const formatOptions = p.format === 'currency' ? { style: 'currency', currency: String(p.currency ?? 'CNY') } : p.format === 'percent' ? { style: 'unit', unit: 'percent' } : undefined;
    return <Stack width="100%" gap="2">{p.kind === 'currency' && <Flex justify="space-between" align="center"><Text textStyle="xs" color="fg.muted">预算金额</Text><Badge colorPalette="green">CNY</Badge></Flex>}{p.kind === 'percent' && <Flex justify="space-between" align="center"><Text textStyle="xs" color="fg.muted">完成比例</Text><Text textStyle="xs" color="blue.600">0–100%</Text></Flex>}{p.kind === 'stepper' && <Text textStyle="xs" color="fg.muted">画布缩放倍数 · 每次调整 0.1</Text>}{p.kind === 'large' && <Text fontWeight="semibold">组件数量</Text>}<NumberInput.Root width="100%" defaultValue={String(p.defaultValue ?? '0')} min={Number(p.min ?? 0)} max={Number(p.max ?? 100)} step={Number(p.step ?? 1)} size={p.size} disabled={!preview} formatOptions={formatOptions as any}><NumberInput.Control /><NumberInput.Input /></NumberInput.Root>{p.kind === 'stepper' && <Flex justify="space-between" textStyle="xs" color="fg.muted"><Text>最小 {Number(p.min)}</Text><Text>当前精度 1 位小数</Text><Text>最大 {Number(p.max)}</Text></Flex>}</Stack>;
  }
  if (name === 'PasswordInput') return <ChakraPasswordInput props={p} preview={preview} />;
  if (name === 'PinInput') return <Stack width="100%" gap="2">{showcase && p.mask && <Badge width="fit-content" colorPalette="purple">内容已掩码</Badge>}<PinInput.Root width="100%" disabled={!preview} type={p.type ?? 'numeric'} mask={Boolean(p.mask)} size={p.size} defaultValue={runtimeStrings(p.defaultValue)}><PinInput.HiddenInput /><PinInput.Control>{Array.from({ length: Number(p.count ?? 4) }, (_, index) => <PinInput.Input key={index} index={index} />)}</PinInput.Control></PinInput.Root></Stack>;
  if (name === 'RadioCard') return <RadioCard.Root width="100%" defaultValue={String(p.defaultValue ?? 'react')} disabled={!preview} colorPalette={p.colorPalette} variant={p.variant} size={p.size}>
    <RadioCard.Label>选择技术框架</RadioCard.Label><Flex direction={p.orientation === 'vertical' ? 'column' : 'row'} gap="2" align="stretch">{frameworkOptions.slice(0, 3).map((option) => <RadioCard.Item key={option.value} value={option.value} flex="1"><RadioCard.ItemHiddenInput /><RadioCard.ItemControl><RadioCard.ItemText>{option.label}</RadioCard.ItemText><RadioCard.ItemIndicator /></RadioCard.ItemControl></RadioCard.Item>)}</Flex>
  </RadioCard.Root>;
  if (name === 'Rating') return <RatingGroup.Root count={Number(p.count ?? 5)} defaultValue={Number(p.defaultValue ?? 3)} size={p.size} colorPalette={p.colorPalette} allowHalf={Boolean(p.allowHalf)} disabled={!preview}><RatingGroup.HiddenInput /><RatingGroup.Control /></RatingGroup.Root>;
  if (name === 'SegmentedControl') return <SegmentGroup.Root width="100%" defaultValue={String(p.defaultValue ?? '')} orientation={p.orientation ?? 'horizontal'} size={p.size} disabled={!preview}><SegmentGroup.Indicator />{items.map((item) => <SegmentGroup.Item key={String(item.value)} value={String(item.value)}><SegmentGroup.ItemText>{String(item.label)}</SegmentGroup.ItemText><SegmentGroup.ItemHiddenInput /></SegmentGroup.Item>)}</SegmentGroup.Root>;
  if (name === 'TagsInput') return <TagsInput.Root width="100%" defaultValue={runtimeStrings(p.defaultValue)} max={Number(p.max ?? 10)} size={p.size} disabled={!preview}><TagsInput.Label>{String(p.label ?? '标签')}</TagsInput.Label><TagsInput.Control><TagsInput.Items /><TagsInput.Input placeholder={String(p.placeholder ?? '添加标签…')} /></TagsInput.Control><Text textStyle="xs" color="fg.muted" textAlign="right">按回车添加标签</Text></TagsInput.Root>;
  if (name === 'Combobox') return <ChakraCombobox props={p} preview={preview} showcase={showcase} />;
  if (name === 'Listbox') {
    const options = optionsForKind(p.dataKind); const collection = createListCollection({ items: options });
    return <Listbox.Root collection={collection} width="100%" height="100%" defaultValue={runtimeStrings(p.defaultValue)} selectionMode={p.selectionMode ?? 'single'} orientation={p.orientation ?? 'vertical'} disabled={!preview}><Listbox.Label>{String(p.label ?? '选择项目')}</Listbox.Label><Listbox.Content>{collection.items.map((option) => <Listbox.Item key={option.value} item={option}><Listbox.ItemText>{option.label}</Listbox.ItemText><Listbox.ItemIndicator /></Listbox.Item>)}</Listbox.Content></Listbox.Root>;
  }
  if (name === 'Select') {
    const options = optionsForKind(p.dataKind); const collection = createListCollection({ items: options });
    const content = <Select.Content position={showcase ? 'relative' : undefined} inset={showcase ? 'auto' : undefined} width="100%">{collection.items.map((option) => <Select.Item key={option.value} item={option}>{option.label}<Select.ItemIndicator /></Select.Item>)}</Select.Content>;
    return <Select.Root collection={collection} width="100%" multiple={Boolean(p.multiple)} size={p.size} variant={p.variant} disabled={!preview} defaultValue={runtimeStrings(p.defaultValue)} open={showcase ? true : undefined}><Select.HiddenSelect /><Select.Label>{String(p.label ?? '选择项目')}</Select.Label><Select.Control><Select.Trigger><Select.ValueText placeholder={String(p.placeholder ?? '请选择')} /></Select.Trigger><Select.IndicatorGroup><Select.ClearTrigger /><Select.Indicator /></Select.IndicatorGroup></Select.Control>{showcase ? <Box marginTop="2">{content}</Box> : <Portal><Select.Positioner>{content}</Select.Positioner></Portal>}</Select.Root>;
  }
  if (name === 'TreeView') return <TreeView.Root collection={fileTree} width="100%" height="100%" selectionMode={p.selectionMode ?? 'single'} defaultExpandedValue={runtimeStrings(p.defaultExpandedValue)} defaultSelectedValue={runtimeStrings(p.defaultSelectedValue)} pointerEvents={preview ? 'auto' : 'none'}><TreeView.Label>{String(p.label ?? '树视图')}</TreeView.Label><TreeView.Tree><TreeView.Node indentGuide={p.showGuide ? <TreeView.BranchIndentGuide /> : undefined} render={({ node, nodeState }) => nodeState.isBranch ? <TreeView.BranchControl><Folder /><TreeView.BranchText>{String((node as AnyProps).name)}</TreeView.BranchText></TreeView.BranchControl> : <TreeView.Item><File /><TreeView.ItemText>{String((node as AnyProps).name)}</TreeView.ItemText></TreeView.Item>} /></TreeView.Tree></TreeView.Root>;
  if (name === 'Input') return <Input width="100%" height="100%" placeholder={component.content} variant={p.variant} size={p.size} />;
  if (name === 'Textarea') return <Textarea width="100%" height="100%" placeholder={component.content} variant={p.variant} size={p.size} rows={Number(p.rows ?? 4)} />;
  if (name === 'NativeSelect') return <NativeSelect.Root width="100%" height="100%" variant={p.variant} size={p.size}><NativeSelect.Field defaultValue={runtimeRecords(p.options)[0]?.value as string}>{runtimeRecords(p.options).map((option) => <option key={String(option.value)} value={String(option.value)}>{String(option.label)}</option>)}</NativeSelect.Field><NativeSelect.Indicator /></NativeSelect.Root>;
  if (name === 'Checkbox') return <Checkbox.Root defaultChecked={Boolean(p.defaultChecked)} colorPalette={p.colorPalette} variant={p.variant} size={p.size}><Checkbox.HiddenInput /><Checkbox.Control><Checkbox.Indicator /></Checkbox.Control><Checkbox.Label>{component.content}</Checkbox.Label></Checkbox.Root>;
  if (name === 'Switch') return <Switch.Root defaultChecked={Boolean(p.defaultChecked)} colorPalette={p.colorPalette} variant={p.variant} size={p.size}><Switch.HiddenInput /><Switch.Control><Switch.Thumb /></Switch.Control><Switch.Label>{component.content}</Switch.Label></Switch.Root>;
  if (name === 'RadioGroup') return <RadioGroup.Root defaultValue={String(p.defaultValue ?? '')} variant={p.variant} size={p.size}><Flex direction={p.orientation === 'vertical' ? 'column' : 'row'} gap={p.orientation === 'vertical' ? 3 : 5} align={p.orientation === 'vertical' ? 'start' : 'center'} wrap="wrap">{runtimeRecords(p.options).map((option) => <RadioGroup.Item key={String(option.value)} value={String(option.value)}><RadioGroup.ItemHiddenInput /><RadioGroup.ItemIndicator /><RadioGroup.ItemText>{String(option.label)}</RadioGroup.ItemText></RadioGroup.Item>)}</Flex></RadioGroup.Root>;
  if (name === 'Slider') return <Slider.Root width={p.orientation === 'vertical' ? undefined : '100%'} height={p.orientation === 'vertical' ? '100%' : undefined} defaultValue={[Number(p.defaultValue ?? 50)]} min={Number(p.min ?? 0)} max={Number(p.max ?? 100)} colorPalette={p.colorPalette} variant={p.variant} size={p.size} orientation={p.orientation ?? 'horizontal'}><Slider.Control><Slider.Track><Slider.Range /></Slider.Track><Slider.Thumb index={0} /></Slider.Control></Slider.Root>;
  if (name === 'Fieldset') return <Fieldset.Root {...fill} padding="4" borderWidth="1px" borderRadius="xl" size={p.size} disabled={Boolean(p.disabled)} background={p.kind === 'locked' ? 'gray.50' : 'bg.panel'}><Fieldset.Legend>{String(p.legend ?? '字段组')}</Fieldset.Legend><Fieldset.HelperText>{String(p.helperText ?? '')}</Fieldset.HelperText>{slotContent.content ?? fieldsetBody(p)}</Fieldset.Root>;
  if (name === 'Editable') return <Editable.Root width="100%" defaultValue={String(p.value ?? component.content)} placeholder={String(p.placeholder ?? '')} size={p.size} defaultEdit={showcase && Boolean(p.defaultEdit)} activationMode={preview ? p.activationMode ?? 'click' : 'none'}><Flex align="center" gap="2"><Editable.Area flex="1"><Editable.Preview width="100%" minHeight="10" padding="2" borderBottomWidth="1px" /><Editable.Input width="100%" /></Editable.Area><Editable.Control><Editable.EditTrigger asChild><IconButton aria-label="编辑" size="xs" variant="ghost"><Settings /></IconButton></Editable.EditTrigger><Editable.CancelTrigger asChild><IconButton aria-label="取消" size="xs" variant="ghost"><X /></IconButton></Editable.CancelTrigger><Editable.SubmitTrigger asChild><IconButton aria-label="确认" size="xs" variant="ghost" colorPalette="green"><Check /></IconButton></Editable.SubmitTrigger></Editable.Control></Flex></Editable.Root>;

  if (name === 'Breadcrumb') return <Breadcrumb.Root variant={p.variant} size={p.size}><Breadcrumb.List>{items.map((item, index) => <Breadcrumb.Item key={String(item.key)}>{index === items.length - 1 ? <Breadcrumb.CurrentLink>{String(item.label)}</Breadcrumb.CurrentLink> : <Breadcrumb.Link>{String(item.label)}</Breadcrumb.Link>}{index < items.length - 1 && <Breadcrumb.Separator>{String(p.separator ?? '/')}</Breadcrumb.Separator>}</Breadcrumb.Item>)}</Breadcrumb.List></Breadcrumb.Root>;
  if (name === 'Pagination') return <Pagination.Root count={Number(p.count ?? 100)} pageSize={Number(p.pageSize ?? 10)} defaultPage={Number(p.defaultPage ?? 1)} siblingCount={Number(p.siblingCount ?? 1)}><Flex align="center" gap="1" height="100%"><Pagination.PrevTrigger asChild><IconButton size={p.size ?? 'sm'} variant={p.variant ?? 'outline'} aria-label="上一页"><ChevronLeft /></IconButton></Pagination.PrevTrigger><Pagination.Items render={(page) => <IconButton size={p.size ?? 'sm'} variant={{ base: 'ghost', _selected: p.variant === 'solid' ? 'solid' : 'outline' } as any} aria-label={`第 ${page.value} 页`}>{page.value}</IconButton>} /><Pagination.NextTrigger asChild><IconButton size={p.size ?? 'sm'} variant={p.variant ?? 'outline'} aria-label="下一页"><ChevronRight /></IconButton></Pagination.NextTrigger></Flex></Pagination.Root>;
  if (name === 'Steps') return <Steps.Root width="100%" height="100%" count={items.length} defaultStep={Number(p.defaultStep ?? 1)} orientation={p.orientation ?? 'horizontal'} size={p.size} variant={p.variant}><Steps.List>{items.map((item, index) => <Steps.Item key={String(item.key)} index={index}><Steps.Trigger><Steps.Indicator><Steps.Status complete={<Check />} incomplete={<Steps.Number />} /></Steps.Indicator><Box><Steps.Title>{String(item.label)}</Steps.Title>{item.description && <Steps.Description>{String(item.description)}</Steps.Description>}</Box></Steps.Trigger><Steps.Separator /></Steps.Item>)}</Steps.List></Steps.Root>;
  if (name === 'Tabs') return <Tabs.Root {...fill} defaultValue={String(p.defaultValue ?? items[0]?.key ?? '')} variant={p.variant} size={p.size} fitted={Boolean(p.fitted)} justify={p.justify}><Tabs.List>{items.map((item) => <Tabs.Trigger key={String(item.key)} value={String(item.key)}>{String(item.label)}</Tabs.Trigger>)}<Tabs.Indicator /></Tabs.List>{items.map((item) => <Tabs.Content key={String(item.key)} value={String(item.key)} padding="3">{slotContent[`tab-${String(item.key)}`] ?? <Text color="fg.muted">{String(item.label)}内容</Text>}</Tabs.Content>)}</Tabs.Root>;
  if (name === 'Accordion') return <Accordion.Root {...fill} defaultValue={Array.isArray(p.defaultValue) ? p.defaultValue.map(String) : [String(p.defaultValue ?? items[0]?.key ?? '')]} multiple={Boolean(p.multiple)} collapsible={p.collapsible !== false} variant={p.variant} size={p.size}>{items.map((item) => <Accordion.Item key={String(item.key)} value={String(item.key)}><Accordion.ItemTrigger><Text flex="1">{String(item.label)}</Text><Accordion.ItemIndicator /></Accordion.ItemTrigger><Accordion.ItemContent><Accordion.ItemBody>{slotContent[`panel-${String(item.key)}`] ?? <Text color="fg.muted">{String(item.label)}的详细内容。</Text>}</Accordion.ItemBody></Accordion.ItemContent></Accordion.Item>)}</Accordion.Root>;
  if (name === 'Collapsible') return <Collapsible.Root {...fill} defaultOpen={Boolean(p.defaultOpen)} disabled={!preview}><Collapsible.Trigger asChild><Button variant="ghost" width="100%" justifyContent="space-between">{component.content}<Collapsible.Indicator>⌄</Collapsible.Indicator></Button></Collapsible.Trigger><Collapsible.Content borderTopWidth="1px" padding="3">{slotContent.content ?? <Text color="fg.muted">这里是可以继续设计的折叠内容。</Text>}</Collapsible.Content></Collapsible.Root>;
  if (name === 'Carousel') {
    const renderSlide = (index: number) => {
      if (p.kind === 'story') return <Stack height="100%" minHeight="36" justify="end" padding="5" borderRadius="lg" background="linear-gradient(135deg, #111827, #4338ca)" color="white"><Text textStyle="xs" opacity=".7">PRODUCT STORY · 0{index + 1}</Text><Heading size="lg">让灵感成为可编辑页面</Heading><Text textStyle="sm" opacity=".8">从内容叙事到视觉层级，保留每一次设计判断。</Text></Stack>;
      if (p.kind === 'metrics') return <Stack height="100%" minHeight="36" justify="center" align="center" padding="4" borderWidth="1px" borderRadius="lg" background="bg.panel"><Text textStyle="xs" color="fg.muted">{['页面', '组件', '批注', '协作者'][index % 4]}</Text><Heading size="2xl" color={['blue.600', 'purple.600', 'teal.600', 'orange.600'][index % 4]}>{[12, 86, 7, 4, 28, 96][index]}</Heading></Stack>;
      if (p.kind === 'campaign') return <Flex height="100%" minHeight="36" align="center" justify="space-between" gap="4" padding="5" borderRadius="lg" background={['orange.50', 'purple.50', 'blue.50', 'teal.50'][index % 4]}><Stack gap="1"><Text textStyle="xs" color="fg.muted">AUTOPLAY CAMPAIGN</Text><Heading size="md">{['发布新品页面', '展示品牌故事', '连接设计系统', '邀请团队协作'][index]}</Heading></Stack><Center width="14" height="14" flex="none" borderRadius="full" background={['orange.500', 'purple.500', 'blue.500', 'teal.500'][index]} color="white" fontWeight="bold">{index + 1}</Center></Flex>;
      return <Center height="100%" minHeight="36" borderRadius="lg" background={['blue.100', 'purple.100', 'teal.100', 'orange.100'][index % 4]} color={['blue.800', 'purple.800', 'teal.800', 'orange.800'][index % 4]}><Stack align="center" gap="1"><Text fontSize="4xl" fontWeight="bold">{index + 1}</Text><Text textStyle="xs">循环作品画廊</Text></Stack></Center>;
    };
    return <Carousel.Root {...fill} slideCount={Number(p.slideCount ?? 5)} slidesPerPage={Number(p.slidesPerPage ?? 1)} defaultPage={Number(p.defaultPage ?? 0)} loop={Boolean(p.loop)} autoplay={Boolean(p.autoplay)}><Carousel.ItemGroup>{Array.from({ length: Number(p.slideCount ?? 5) }, (_, index) => <Carousel.Item key={index} index={index} padding="1">{slotContent[`slide-${index + 1}`] ?? renderSlide(index)}</Carousel.Item>)}</Carousel.ItemGroup><Carousel.Control justifyContent="center" gap="3"><Carousel.PrevTrigger asChild><IconButton aria-label="上一张" size="xs" variant="ghost" disabled={!preview}><ChevronLeft /></IconButton></Carousel.PrevTrigger><Carousel.Indicators /><Carousel.NextTrigger asChild><IconButton aria-label="下一张" size="xs" variant="ghost" disabled={!preview}><ChevronRight /></IconButton></Carousel.NextTrigger></Carousel.Control></Carousel.Root>;
  }

  if (name === 'Avatar') return <Avatar.Root size={p.size ?? 'lg'} variant={p.variant} shape={p.shape} colorPalette={p.colorPalette}><Avatar.Fallback name={String(p.name ?? component.content)} /><Avatar.Image src={p.src} /></Avatar.Root>;
  if (name === 'Badge') return <Badge width="fit-content" colorPalette={p.colorPalette} variant={p.variant}>{component.content}</Badge>;
  if (name === 'Card') return <Card.Root {...fill} variant={p.variant}><Card.Header><Card.Title>{String(p.title ?? '卡片')}</Card.Title></Card.Header><Card.Body>{slotContent.content ?? <Text color="fg.muted">{component.content}</Text>}</Card.Body></Card.Root>;
  if (name === 'Table') return <Table.ScrollArea width="100%" height="100%"><Table.Root variant={p.variant} size={p.size} striped={Boolean(p.striped)} interactive={Boolean(p.interactive)} stickyHeader={Boolean(p.stickyHeader)} showColumnBorder={Boolean(p.showColumnBorder)}><Table.Header><Table.Row>{runtimeStrings(p.columns).map((column) => <Table.ColumnHeader key={column}>{column}</Table.ColumnHeader>)}</Table.Row></Table.Header><Table.Body>{runtimeRows(p.rows).map((row, rowIndex) => <Table.Row key={rowIndex}>{row.map((cell, cellIndex) => <Table.Cell key={cellIndex}>{cell}</Table.Cell>)}</Table.Row>)}</Table.Body></Table.Root></Table.ScrollArea>;
  if (name === 'Stat') return <Stat.Root {...fill} size={p.size} borderWidth="1px" borderRadius="lg" padding="4"><Stat.Label>{String(p.label)}</Stat.Label><Stat.ValueText>{String(p.value)}</Stat.ValueText><Stat.HelpText color={p.direction === 'down' ? 'red.600' : p.direction === 'up' ? 'green.600' : 'fg.muted'}>{p.direction === 'up' ? <Stat.UpIndicator /> : p.direction === 'down' ? <Stat.DownIndicator /> : null}{String(p.change)}</Stat.HelpText></Stat.Root>;
  if (name === 'Timeline') return <Timeline.Root variant={p.variant} size={p.size}>{items.map((item, index) => <Timeline.Item key={String(item.key)}><Timeline.Connector><Timeline.Separator />{(index < items.length - 1 || p.showLastSeparator) && <Timeline.Indicator />}</Timeline.Connector><Timeline.Content><Timeline.Title>{String(item.label)}</Timeline.Title><Timeline.Description>{String(item.description)}</Timeline.Description></Timeline.Content></Timeline.Item>)}</Timeline.Root>;
  if (name === 'Clipboard') return <Clipboard.Root width="100%" value={String(p.value ?? '')}><Flex align="center" gap="2" width="100%">{p.kind === 'input' ? <Clipboard.Input asChild><Input readOnly /></Clipboard.Input> : p.kind === 'code' ? <Code flex="1" padding="3" overflow="hidden" whiteSpace="nowrap">{String(p.value)}</Code> : <Text flex="1" color="fg.muted" textStyle="sm">{String(p.value)}</Text>}<Clipboard.Trigger asChild><Button variant="surface" size="sm" disabled={!preview}><Clipboard.Indicator />{String(p.label ?? '复制')}</Button></Clipboard.Trigger></Flex></Clipboard.Root>;
  if (name === 'Image') return <Image width="100%" height="100%" src={sampleImage} alt={String(p.alt ?? '示例图片')} objectFit={p.fit ?? 'cover'} borderRadius={p.borderRadius ?? 'lg'} />;
  if (name === 'DataList') return <DataList.Root width="100%" orientation={p.orientation ?? 'horizontal'} size={p.size}>{[{ label: '组件数量', value: '84' }, { label: '设计状态', value: '可用' }, { label: '最后保存', value: '刚刚' }].map((entry) => <DataList.Item key={entry.label}><DataList.ItemLabel>{entry.label}</DataList.ItemLabel><DataList.ItemValue>{entry.value}</DataList.ItemValue></DataList.Item>)}</DataList.Root>;
  if (name === 'Icon') {
    const Glyph = p.icon === 'sparkle' ? Sparkles : p.icon === 'check' ? CheckCircle2 : p.icon === 'info' ? Info : Heart;
    return <Center {...fill}><Icon size={p.size ?? 'xl'} color={p.color ?? 'pink.600'}><Glyph fill={p.icon === 'heart' ? 'currentColor' : 'none'} /></Icon></Center>;
  }
  if (name === 'Marquee') return <Box {...fill} position="relative"><Marquee.Root {...fill} side={p.side ?? 'left'} reverse={Boolean(p.reverse)} speed={Number(p.speed ?? 40)} pauseOnInteraction={p.pauseOnInteraction !== false}><Marquee.Viewport>{p.edge && <Marquee.Edge side="start" />}<Marquee.Content>{['Chakra UI', 'Ant Design', 'shadcn/ui', 'AI Design'].map((label) => <Marquee.Item key={label} paddingInline="4"><Badge size="lg" variant="surface" colorPalette="purple">{label}</Badge></Marquee.Item>)}</Marquee.Content>{p.edge && <Marquee.Edge side="end" />}</Marquee.Viewport></Marquee.Root>{showcase && <Badge position="absolute" right="2" bottom="2" colorPalette={Number(p.speed ?? 40) >= 80 ? 'orange' : p.reverse ? 'blue' : 'purple'}>{p.side === 'bottom' ? '向下滚动' : p.reverse ? '向右滚动' : Number(p.speed ?? 40) >= 80 ? '快速向左' : '向左滚动'}</Badge>}</Box>;
  if (name === 'QRCode') return <Center {...fill}><QrCode.Root value={String(p.value ?? 'https://chakra-ui.com')} size="full" width={`${Number(p.size ?? 160)}px`} height={`${Number(p.size ?? 160)}px`}><QrCode.Frame fill="white"><QrCode.Pattern fill={p.color ?? '#111827'} /></QrCode.Frame>{p.overlay && <QrCode.Overlay><Center width="8" height="8" borderRadius="md" background="purple.600" color="white" fontWeight="bold">W</Center></QrCode.Overlay>}</QrCode.Root></Center>;
  if (name === 'Tag') return <Center {...fill}><Tag.Root variant={p.variant} colorPalette={p.colorPalette} size={p.size}><Tag.Label>{component.content}</Tag.Label>{p.closable && <Tag.EndElement><Tag.CloseTrigger disabled={!preview} /></Tag.EndElement>}</Tag.Root></Center>;

  if (name === 'Alert') return <Alert.Root status={p.status} variant={p.variant} size={p.size} inline={Boolean(p.inline)} {...fill}><Alert.Indicator /><Alert.Content><Alert.Title>{String(p.title ?? '提示')}</Alert.Title><Alert.Description>{component.content}</Alert.Description></Alert.Content></Alert.Root>;
  if (name === 'Progress') return <Stack width="100%" gap="2"><Flex justify="space-between"><Text textStyle="sm">完成进度</Text><Text textStyle="sm">{Number(p.value ?? 0)}%</Text></Flex><Progress.Root value={Number(p.value ?? 0)} colorPalette={p.colorPalette} variant={p.variant} shape={p.shape} size={p.size} striped={p.striped} animated={p.animated}><Progress.Track><Progress.Range /></Progress.Track></Progress.Root></Stack>;
  if (name === 'Spinner') return <Flex {...fill} align="center" justify="center"><Spinner size={p.size ?? 'xl'} color={`${p.colorPalette ?? 'blue'}.500`} /></Flex>;
  if (name === 'Skeleton') return <Skeleton loading variant={p.variant}><SkeletonComposition kind={String(p.kind ?? 'text')} lines={Number(p.lines ?? 3)} className="chakra-skeleton-composition" /></Skeleton>;
  if (name === 'EmptyState') {
    const EmptyIcon = p.icon === 'search' ? Search : p.icon === 'warning' ? TriangleAlert : FolderOpen;
    return <EmptyState.Root {...fill} size={p.size} borderWidth="1px" borderRadius="lg"><EmptyState.Content><EmptyState.Indicator><EmptyIcon /></EmptyState.Indicator><EmptyState.Title>{String(p.title)}</EmptyState.Title><EmptyState.Description>{String(p.description)}</EmptyState.Description><Button size="sm" variant="outline">{String(p.action ?? '创建项目')}</Button></EmptyState.Content></EmptyState.Root>;
  }
  if (name === 'ProgressCircle') return <Center {...fill}><ProgressCircle.Root value={Number(p.value ?? 0)} size={p.size} colorPalette={p.colorPalette}><ProgressCircle.Circle><ProgressCircle.Track /><ProgressCircle.Range /></ProgressCircle.Circle>{p.showValue && <ProgressCircle.ValueText position="absolute" fontWeight="semibold" />}</ProgressCircle.Root></Center>;
  if (name === 'Status') return <Center {...fill}><Flex align="center" gap="2"><Status.Root colorPalette={p.colorPalette} size={p.size}><Status.Indicator /></Status.Root><Text>{String(p.label ?? '状态')}</Text></Flex></Center>;
  if (name === 'Toast') return <><Button width="100%" height="100%" colorPalette={p.type === 'error' ? 'red' : p.type === 'success' ? 'green' : 'blue'} onClick={() => preview && studioToaster.create({ type: p.type, title: String(p.title), description: String(p.description), closable: Boolean(p.closable) })}>{component.content}</Button><StudioToaster /></>;

  if (name === 'ActionBar') return <><Button width="100%" height="100%" variant="outline" colorPalette={p.kind === 'bulk' ? 'blue' : p.kind === 'layout' ? 'gray' : 'purple'} onClick={() => preview && setOpen((value) => !value)}>{component.content}</Button><ActionBar.Root open={open}><Portal><ActionBar.Positioner><ActionBar.Content>{slotContent.content ?? <><ActionBar.SelectionTrigger>{Number(p.selectedCount ?? 0)} {p.kind === 'bulk' ? '个页面' : p.kind === 'layout' ? '个图层' : '项已选择'}</ActionBar.SelectionTrigger><ActionBar.Separator />{p.kind === 'layout' && <IconButton aria-label="左对齐" size="sm" variant="ghost"><AlignLeftIcon /></IconButton>}{runtimeStrings(p.actions).filter((action) => !(p.kind === 'layout' && action === '左对齐')).map((action) => <Button key={action} size="sm" variant={action === '发布' ? 'solid' : 'outline'} colorPalette={action === '删除' ? 'red' : action === '发布' ? 'blue' : 'gray'} onClick={() => setOpen(false)}>{action === '删除' ? <Trash2 /> : action === '组合' ? <GroupIcon /> : action === '锁定' ? <Settings /> : <Share2 />}{action}</Button>)}</>}</ActionBar.Content></ActionBar.Positioner></Portal></ActionBar.Root></>;
  if (name === 'FloatingPanel') return <FloatingPanel.Root defaultOpen={preview && Boolean(p.defaultOpen)}><FloatingPanel.Trigger asChild><Button width="100%" height="100%" variant="outline" disabled={!preview}>{component.content}</Button></FloatingPanel.Trigger><Portal><FloatingPanel.Positioner><FloatingPanel.Content><FloatingPanel.Header><FloatingPanel.DragTrigger><GripHorizontal /><FloatingPanel.Title>{String(p.title ?? '浮动面板')}</FloatingPanel.Title></FloatingPanel.DragTrigger><FloatingPanel.Control><FloatingPanel.StageTrigger stage="minimized" asChild><IconButton aria-label="最小化" variant="ghost" size="2xs"><Minus /></IconButton></FloatingPanel.StageTrigger><FloatingPanel.StageTrigger stage="maximized" asChild><IconButton aria-label="最大化" variant="ghost" size="2xs"><Square /></IconButton></FloatingPanel.StageTrigger><FloatingPanel.StageTrigger stage="default" asChild><IconButton aria-label="恢复" variant="ghost" size="2xs"><Maximize2 /></IconButton></FloatingPanel.StageTrigger><FloatingPanel.CloseTrigger asChild><IconButton aria-label="关闭面板" variant="ghost" size="2xs"><X /></IconButton></FloatingPanel.CloseTrigger></FloatingPanel.Control></FloatingPanel.Header><FloatingPanel.Body>{slotContent.content ?? floatingPanelBody(p)}</FloatingPanel.Body><FloatingPanel.ResizeTriggers /></FloatingPanel.Content></FloatingPanel.Positioner></Portal></FloatingPanel.Root>;
  if (name === 'HoverCard') return <HoverCard.Root openDelay={Number(p.openDelay ?? 250)} closeDelay={Number(p.closeDelay ?? 150)} disabled={!preview}><HoverCard.Trigger asChild><Link href="#" onClick={(event) => event.preventDefault()}>{component.content}</Link></HoverCard.Trigger><Portal><HoverCard.Positioner><HoverCard.Content><HoverCard.Arrow />{slotContent.popup ?? hoverCardBody(p)}</HoverCard.Content></HoverCard.Positioner></Portal></HoverCard.Root>;
  if (name === 'OverlayManager') return <><Button width="100%" height="100%" onClick={() => preview && managedOverlay.open(`overlay-${component.id}`, { title: p.title, description: p.description, kind: p.kind, content: slotContent.content })}>{component.content}</Button><managedOverlay.Viewport /></>;
  if (name === 'ToggleTip') return <ChakraPopover.Root open={!preview ? false : undefined} positioning={{ gutter: 4 }}><ChakraPopover.Trigger asChild>{p.kind === 'icon' ? <IconButton width="100%" height="100%" aria-label="查看帮助" variant="outline"><Info /></IconButton> : <Button width="100%" height="100%" variant="outline">{p.kind === 'shortcut' ? <><Search />命令提示</> : <><Settings />锁定说明</>}</Button>}</ChakraPopover.Trigger><Portal><ChakraPopover.Positioner><ChakraPopover.Content width="auto" padding={p.kind === 'rich' ? '4' : '2 3'}>{p.showArrow && <ChakraPopover.Arrow><ChakraPopover.ArrowTip /></ChakraPopover.Arrow>}{slotContent.popup ?? toggleTipBody(p)}</ChakraPopover.Content></ChakraPopover.Positioner></Portal></ChakraPopover.Root>;
  if (name === 'Dialog' || name === 'Drawer') return <><Button ref={triggerRef} width="100%" height="100%" colorPalette="blue" onClick={() => preview && setOpen(true)}>{component.content}</Button><DesignOverlay anchorRef={triggerRef} open={open} side={name === 'Dialog' ? String(p.placement ?? 'center') : String(p.placement ?? 'right')} size={String(p.size ?? 'md')} scrollBehavior={String(p.scrollBehavior ?? 'outside')} title={String(p.title ?? name)} className="chakra-overlay" onClose={() => setOpen(false)} footer={<><Button variant="outline" onClick={() => setOpen(false)}>取消</Button><Button colorPalette="blue" onClick={() => setOpen(false)}>保存</Button></>}>{slotContent.content ?? <Stack gap="4"><Text color="fg.muted">这是 Chakra UI 的真实交互浮层，可以继续放入表单或展示组件。</Text>{p.scrollBehavior === 'inside' && Array.from({ length: 5 }, (_, index) => <Box key={index} padding="3" borderRadius="md" background="gray.50">长内容区块 {index + 1}</Box>)}<Input placeholder="输入内容" /></Stack>}</DesignOverlay></>;
  if (name === 'Popover') return <><Button ref={triggerRef} width="100%" height="100%" variant="outline" onClick={() => preview && setOpen(!open)}>{component.content}</Button><FloatingSurface anchorRef={triggerRef} open={open} placement={String(p.placement ?? 'bottom')} className="chakra-floating"><Stack gap="3"><Heading size="sm">{String(p.title ?? '气泡内容')}</Heading>{slotContent.popup ?? <Text color="fg.muted" textStyle="sm">这是可以继续设计的 Chakra Popover 内容。</Text>}<Button size="sm" onClick={() => setOpen(false)}>完成</Button></Stack></FloatingSurface></>;
  if (name === 'Tooltip') return <><Button ref={hoverRef} width="100%" height="100%" variant="outline" onMouseEnter={() => preview && setOpen(true)} onMouseLeave={() => setOpen(false)}>{component.content}</Button><FloatingSurface anchorRef={hoverRef} open={open} placement={String(p.placement ?? 'top')} className="chakra-tooltip-surface">{String(p.content)}</FloatingSurface></>;
  if (name === 'Menu') return <><Button ref={triggerRef} width="100%" height="100%" variant="outline" onClick={() => preview && setOpen(!open)}>{component.content}⌄</Button><FloatingSurface anchorRef={triggerRef} open={open} className="chakra-menu-surface">{items.map((item, index) => <div key={String(item.key)}>{p.kind === 'grouped' && (index === 0 || items[index - 1]?.group !== item.group) && <small>{String(item.group)}</small>}<button onClick={() => setOpen(false)}>{item.icon && <span>{String(item.icon)}</span>}{p.kind === 'checkbox' && <span>{item.checked ? '✓' : ''}</span>}{String(item.label)}</button></div>)}</FloatingSurface></>;

  if (name === 'LocaleProvider') return <LocaleProvider locale={String(p.locale ?? 'zh-CN')}><Stack {...fill} dir={p.direction ?? 'ltr'} padding="4" borderWidth="1px" borderRadius="lg" gap="3"><Heading size="lg">{String(p.title)}</Heading><Text color="fg.muted">{p.direction === 'rtl' ? 'يتم ترتيب المحتوى وعناصر التحكم من اليمين إلى اليسار.' : '日期、数字和布局方向会遵循当前语言环境。'}</Text><Slider.Root defaultValue={[65]} disabled={!preview}><Slider.Control><Slider.Track><Slider.Range /></Slider.Track><Slider.Thumb index={0} /></Slider.Control></Slider.Root></Stack></LocaleProvider>;
  if (name === 'FormatNumber') return <LocaleProvider locale={String(p.locale ?? 'zh-CN')}><Center {...fill}><Text textStyle="2xl" fontWeight="semibold"><FormatNumber value={Number(p.value ?? 0)} style={p.style ?? 'decimal'} currency={p.currency} notation={p.notation} maximumFractionDigits={p.maximumFractionDigits} /></Text></Center></LocaleProvider>;
  if (name === 'FormatByte') return <LocaleProvider locale={String(p.locale ?? 'zh-CN')}><Center {...fill}><Text textStyle="xl"><FormatByte value={Number(p.value ?? 0)} unitSystem={p.unitSystem ?? 'decimal'} unitDisplay={p.unitDisplay ?? 'short'} /></Text></Center></LocaleProvider>;
  if (name === 'Checkmark') return <Center {...fill}><Checkmark checked={Boolean(p.checked)} indeterminate={Boolean(p.indeterminate)} disabled={Boolean(p.disabled)} size={p.size} colorPalette={p.colorPalette} /></Center>;
  if (name === 'ClientOnly') return <ClientOnly fallback={<Center {...fill}><Spinner size="sm" /><Text ml="2">{String(p.fallback)}</Text></Center>}>{clientOnlyBody(p)}</ClientOnly>;
  if (name === 'EnvironmentProvider') return <EnvironmentProvider value={() => globalThis.document}>{environmentBody(p)}</EnvironmentProvider>;
  if (name === 'For') return <Flex {...fill} direction={p.kind === 'rows' ? 'column' : 'row'} gap="2" flexWrap="wrap"><For each={Array.from({ length: Number(p.count ?? 4) }, (_, index) => index + 1)}>{(value) => p.kind === 'tags' ? <Tag.Root key={value} colorPalette="purple" variant="subtle"><Tag.Label>标签 {value}</Tag.Label></Tag.Root> : p.kind === 'rows' ? <Flex key={value} justify="space-between" width="100%" padding="2 3" borderWidth="1px" borderRadius="md"><Text>数据行 {value}</Text><Badge>可用</Badge></Flex> : <Center key={value} flex="1" minWidth="16" minHeight="16" borderRadius="lg" background="blue.50" color="blue.700" fontWeight="bold">{value}</Center>}</For></Flex>;
  if (name === 'Presence') return <Stack {...fill} align="stretch" justify="center" gap="3"><Button size="sm" alignSelf="center" variant="outline" disabled={!preview} onClick={() => setVisible((value) => !value)}>{p.kind === 'status' ? '切换同步状态' : p.kind === 'details' ? '展开 / 收起详情' : '显示确认面板'}</Button><Presence present={visible} lazyMount={Boolean(p.lazyMount)} unmountOnExit={Boolean(p.unmountOnExit)} animationName={p.animation === 'scale' ? { _open: 'scale-fade-in', _closed: 'scale-fade-out' } : { _open: 'fade-in', _closed: 'fade-out' }} animationDuration="moderate">{presenceBody(p)}</Presence></Stack>;
  if (name === 'Portal') return <><Button ref={triggerRef} width="100%" height="100%" variant="outline" onClick={() => preview && setOpen((value) => !value)}>{component.content}</Button>{open && <Portal><Box position="fixed" zIndex="2000" top={String(p.placement).startsWith('top') ? '80px' : undefined} bottom={String(p.placement).startsWith('bottom') ? '32px' : undefined} left={String(p.placement).endsWith('center') ? '50%' : undefined} right={String(p.placement).endsWith('end') ? '32px' : undefined} transform={String(p.placement).endsWith('center') ? 'translateX(-50%)' : undefined} padding="4" borderWidth="1px" borderRadius="xl" background="bg.panel" shadow="xl">{portalBody(p)}</Box></Portal>}</>;
  if (name === 'Radiomark') return <Center {...fill}><Radiomark checked={Boolean(p.checked)} disabled={Boolean(p.disabled)} size={p.size} colorPalette={p.colorPalette} /></Center>;
  if (name === 'Show') return <Stack {...fill} align="center" justify="center" gap="3"><Button variant="outline" disabled={!preview} onClick={() => setCounter((value) => value + 1)}>当前值：{counter}</Button><Show when={counter >= Number(p.threshold ?? 0)} fallback={<Text color="fg.muted">达到 {Number(p.threshold ?? 0)} 后显示内容</Text>}><Alert.Root status="success" variant="subtle"><Alert.Indicator /><Alert.Title>{String(p.label)}</Alert.Title></Alert.Root></Show></Stack>;
  if (name === 'SkipNav') return <Box {...fill} position="relative" overflow="hidden"><SkipNavLink>{String(p.label)}</SkipNavLink>{skipNavBody(p)}</Box>;
  if (name === 'VisuallyHidden') {
    const A11yIcon = p.icon === 'settings' ? Settings : p.icon === 'check' ? CheckCircle2 : Bell;
    return <Button {...fill} variant="outline"><A11yIcon />{String(p.visibleText)}<VisuallyHidden>{String(p.hiddenText)}</VisuallyHidden></Button>;
  }
  if (name === 'Theme') return <Theme {...fill} appearance={p.appearance ?? 'dark'} colorPalette={p.colorPalette ?? 'teal'} padding="4" borderRadius="lg"><Stack gap="3"><Heading size="md">局部 {p.appearance === 'dark' ? '深色' : '浅色'}主题</Heading><Text color="fg.muted">只影响这个容器内部的组件外观。</Text><Button variant="surface" alignSelf="start">主题按钮</Button></Stack></Theme>;

  return <Box {...fill} borderWidth="1px" borderRadius="md" display="grid" placeItems="center"><Badge colorPalette="teal">Chakra UI · {name}</Badge></Box>;
}
