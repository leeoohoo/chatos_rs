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
import {
  Bell, Bold, Braces, CalendarDays, Check, CheckCircle2, ChevronLeft, ChevronRight, Circle, Download, Eye, EyeOff, File,
  Folder, FolderOpen, GripHorizontal, Heart, Heading1, Heading2, Heading3, Info, Italic, List as ListIcon, ListOrdered,
  Maximize2, Minus, Play, Quote, Redo2, Search, Share2, Sparkles, Square, Strikethrough, Trash2, TriangleAlert, Undo2,
  Settings, Upload, X
} from 'lucide-react';
import type { WebDesignComponent, WebDesignTokens } from '../../src/schema';
import { DesignOverlay, FloatingSurface, SkeletonComposition, runtimeRecords, runtimeRows, runtimeStrings } from './LibraryRuntimePrimitives';

type AnyProps = Record<string, any>;

const studioToaster = createToaster({ placement: 'bottom-end', pauseOnPageIdle: true });
const managedOverlay = createOverlay<AnyProps>((props) => {
  const { title, description, kind, content, ...rootProps } = props;
  return <Dialog.Root {...rootProps}><Portal><Dialog.Backdrop /><Dialog.Positioner><Dialog.Content><Dialog.Header><Dialog.Title>{String(title ?? '浮层')}</Dialog.Title></Dialog.Header><Dialog.Body><Stack gap="4">{content ?? <><Dialog.Description>{String(description ?? '')}</Dialog.Description>{kind === 'form' && <><Input placeholder="项目名称" /><Textarea placeholder="项目说明" /></>}</>}<Button alignSelf="start" onClick={() => props.onOpenChange?.({ open: false })}>完成</Button></Stack></Dialog.Body><Dialog.CloseTrigger asChild><CloseButton /></Dialog.CloseTrigger></Dialog.Content></Dialog.Positioner></Portal></Dialog.Root>;
});
const sampleImage = `data:image/svg+xml,${encodeURIComponent('<svg xmlns="http://www.w3.org/2000/svg" width="800" height="480" viewBox="0 0 800 480"><defs><linearGradient id="g" x1="0" y1="0" x2="1" y2="1"><stop stop-color="#5D50DF"/><stop offset="1" stop-color="#8B5CF6"/></linearGradient></defs><rect width="800" height="480" rx="32" fill="url(#g)"/><circle cx="650" cy="100" r="180" fill="#fff" opacity=".12"/><rect x="80" y="90" width="320" height="30" rx="15" fill="#fff" opacity=".92"/><rect x="80" y="145" width="500" height="18" rx="9" fill="#fff" opacity=".45"/><rect x="80" y="210" width="640" height="190" rx="24" fill="#fff" opacity=".16"/><text x="105" y="315" font-family="Arial" font-size="42" font-weight="700" fill="white">Web Design Studio</text></svg>')}`;

function StudioToaster() {
  return <Portal><ChakraToaster toaster={studioToaster}>{(toast) => <Toast.Root width="sm"><Toast.Indicator /><Stack gap="1" flex="1"><Toast.Title>{toast.title}</Toast.Title><Toast.Description>{toast.description}</Toast.Description></Stack>{toast.closable && <Toast.CloseTrigger />}</Toast.Root>}</ChakraToaster></Portal>;
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

function ChakraCombobox({ props, preview }: { props: AnyProps; preview: boolean }) {
  const initialItems = optionsForKind(props.dataKind);
  const [items, setItems] = useState(initialItems);
  useEffect(() => setItems(initialItems), [props.dataKind]);
  const collection = createListCollection({ items });
  return <Combobox.Root collection={collection} multiple={Boolean(props.multiple)} disabled={!preview} width="100%" onInputValueChange={(details) => {
    const query = details.inputValue.trim().toLowerCase();
    setItems(query ? initialItems.filter((item) => item.label.toLowerCase().includes(query)) : initialItems);
  }}>
    <Combobox.Label>{String(props.label ?? '选择项目')}</Combobox.Label>
    <Combobox.Control><Combobox.Input placeholder={String(props.placeholder ?? '输入并搜索')} /><Combobox.IndicatorGroup><Combobox.ClearTrigger /><Combobox.Trigger /></Combobox.IndicatorGroup></Combobox.Control>
    <Portal><Combobox.Positioner><Combobox.Content><Combobox.Empty>没有匹配项</Combobox.Empty>{collection.items.map((item) => <Combobox.Item key={item.value} item={item}>{item.label}<Combobox.ItemIndicator /></Combobox.Item>)}</Combobox.Content></Combobox.Positioner></Portal>
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

export function ChakraCanvasComponent({ component, preview, tokens, slotContent = {} }: {
  component: WebDesignComponent;
  preview: boolean;
  tokens?: WebDesignTokens;
  slotContent?: Record<string, ReactNode>;
}) {
  return <ChakraProvider value={defaultSystem}><Box width="100%" height="100%" color={tokens?.colors.text ?? '#18181b'} fontFamily={tokens?.typography.fontFamily}>
    <ChakraCanvasRenderer component={component} preview={preview} slotContent={slotContent} />
  </Box></ChakraProvider>;
}

function ChakraCanvasRenderer({ component, preview, slotContent }: { component: WebDesignComponent; preview: boolean; slotContent: Record<string, ReactNode> }) {
  const binding = component.library!;
  const p = binding.props as AnyProps;
  const name = binding.component;
  const triggerRef = useRef<HTMLButtonElement | null>(null);
  const hoverRef = useRef<HTMLButtonElement | null>(null);
  const [open, setOpen] = useState(false);
  const [visible, setVisible] = useState(Boolean(p.present));
  const [counter, setCounter] = useState(Number(p.initialCount ?? 0));
  useEffect(() => setVisible(Boolean(p.present)), [p.present]);
  useEffect(() => setCounter(Number(p.initialCount ?? 0)), [p.initialCount]);
  const fill = { width: '100%', height: '100%' };
  const items = runtimeRecords(p.items);

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
  if (name === 'Container') return <Container {...fill} maxW={p.fluid ? 'full' : p.maxWidth ?? 'lg'} centerContent={p.centerContent} fluid={p.fluid} padding="4" borderWidth="1px" borderRadius="lg">{slotContent.content ?? <Text color="fg.muted">Container 内容区域</Text>}</Container>;
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
  if (name === 'Link') return <Link colorPalette={p.colorPalette ?? 'blue'} variant={p.variant ?? 'underline'} target={p.external ? '_blank' : undefined}>{component.content}{p.external ? ' ↗' : ''}</Link>;
  if (name === 'List') return <ChakraList props={p} />;

  if (name === 'Button') return <Button width="100%" height="100%" variant={p.variant} colorPalette={p.colorPalette} size={p.size}>{component.content}</Button>;
  if (name === 'IconButton') return <IconButton width="100%" height="100%" variant={p.variant} colorPalette={p.colorPalette} size={p.size} aria-label="快捷操作">{component.content}</IconButton>;
  if (name === 'CloseButton') return <CloseButton width="100%" height="100%" variant={p.variant} colorPalette={p.colorPalette} size={p.size} aria-label="关闭" />;
  if (name === 'DownloadTrigger') return <DownloadTrigger asChild fileName={String(p.fileName ?? 'download.txt')} mimeType={p.mimeType ?? 'text/plain'} data={String(p.data ?? '')} onClick={(event) => { if (!preview) event.preventDefault(); }}><Button width="100%" height="100%" variant={p.variant} colorPalette={p.colorPalette}><Download />{component.content}</Button></DownloadTrigger>;
  if (name === 'DateInput') return <DateInput.Root width="100%" disabled={!preview} locale={String(p.locale ?? 'zh-CN')} granularity={p.granularity ?? 'day'}>
    <DateInput.Label>{String(p.label ?? '选择日期')}</DateInput.Label><DateInput.Control><DateInput.Segments /></DateInput.Control><DateInput.HiddenInput />
  </DateInput.Root>;
  if (name === 'DatePicker') return <DatePicker.Root width="100%" disabled={!preview} locale={String(p.locale ?? 'zh-CN')} selectionMode={p.selectionMode ?? 'single'} closeOnSelect={p.closeOnSelect !== false}>
    <DatePicker.Label>{String(p.label ?? '选择日期')}</DatePicker.Label><DatePicker.Control><DatePicker.Input /><DatePicker.IndicatorGroup><DatePicker.Trigger><CalendarDays /></DatePicker.Trigger></DatePicker.IndicatorGroup></DatePicker.Control>
    <Portal><DatePicker.Positioner><DatePicker.Content><DatePicker.View view="day"><DatePicker.Header /><DatePicker.DayTable /></DatePicker.View><DatePicker.View view="month"><DatePicker.Header /><DatePicker.MonthTable /></DatePicker.View><DatePicker.View view="year"><DatePicker.Header /><DatePicker.YearTable /></DatePicker.View></DatePicker.Content></DatePicker.Positioner></Portal>
  </DatePicker.Root>;
  if (name === 'Calendar') return <DatePicker.Root inline width="100%" height="100%" disabled={!preview} locale={String(p.locale ?? 'zh-CN')} size={p.size} selectionMode={p.selectionMode ?? 'single'} hideOutsideDays={Boolean(p.hideOutsideDays)} showWeekNumbers={Boolean(p.showWeekNumbers)}>
    <DatePicker.View view="day"><DatePicker.Header /><DatePicker.DayTable /></DatePicker.View><DatePicker.View view="month"><DatePicker.Header /><DatePicker.MonthTable /></DatePicker.View><DatePicker.View view="year"><DatePicker.Header /><DatePicker.YearTable /></DatePicker.View>
  </DatePicker.Root>;
  if (name === 'CheckboxCard') return <CheckboxCard.Root width="100%" height="100%" defaultChecked={Boolean(p.defaultChecked)} disabled={!preview} colorPalette={p.colorPalette} variant={p.variant} size={p.size}>
    <CheckboxCard.HiddenInput /><CheckboxCard.Control><Box flex="1"><CheckboxCard.Label>{String(p.title ?? '选择项目')}</CheckboxCard.Label><CheckboxCard.Description>{String(p.description ?? '')}</CheckboxCard.Description></Box><CheckboxCard.Indicator /></CheckboxCard.Control>
  </CheckboxCard.Root>;
  if (name === 'ColorPicker') return <ColorPicker.Root width="100%" disabled={!preview} defaultValue={parseColor(String(p.defaultValue ?? '#5D50DF'))} format={p.format ?? 'rgba'} size={p.size}>
    <ColorPicker.HiddenInput /><ColorPicker.Label>{String(p.label ?? '选择颜色')}</ColorPicker.Label><ColorPicker.Control><ColorPicker.Input /><ColorPicker.Trigger /></ColorPicker.Control>
    <Portal><ColorPicker.Positioner><ColorPicker.Content><ColorPicker.Area /><Flex gap="2" align="center"><ColorPicker.EyeDropper size="xs" variant="outline" /><ColorPicker.Sliders /></Flex></ColorPicker.Content></ColorPicker.Positioner></Portal>
  </ColorPicker.Root>;
  if (name === 'ColorSwatch') return <Center {...fill}><Box position="relative"><ColorSwatch value={String(p.value ?? '#5D50DF')} size={p.size} borderRadius={p.shape === 'full' ? 'full' : 'lg'} />{p.showCheck && <Center position="absolute" inset="0" color="white"><Check size={20} /></Center>}</Box></Center>;
  if (name === 'Field') return <Field.Root width="100%" required={Boolean(p.required)} invalid={Boolean(p.invalid)} disabled={Boolean(p.disabled) || !preview}>
    <Field.Label>{String(p.label ?? '字段')}<Field.RequiredIndicator /></Field.Label><Input placeholder={component.content} readOnly={!preview} />{p.helperText && <Field.HelperText>{String(p.helperText)}</Field.HelperText>}{p.errorText && <Field.ErrorText>{String(p.errorText)}</Field.ErrorText>}
  </Field.Root>;
  if (name === 'FileUpload') return <FileUpload.Root width="100%" maxFiles={Number(p.maxFiles ?? 1)} accept={String(p.accept ?? '*/*').split(',')} disabled={!preview}>
    <FileUpload.HiddenInput />{p.kind === 'dropzone' ? <FileUpload.Dropzone height="100%" minHeight="32"><Upload /><FileUpload.DropzoneContent><Text fontWeight="medium">{String(p.label ?? '拖入文件或点击选择')}</Text><Text textStyle="xs" color="fg.muted">支持 {String(p.accept ?? '*/*')} · 最多 {Number(p.maxFiles ?? 1)} 个</Text></FileUpload.DropzoneContent></FileUpload.Dropzone> : <><FileUpload.Trigger asChild><Button variant="outline"><Upload />{String(p.label ?? '上传文件')}</Button></FileUpload.Trigger><FileUpload.List /></>}
  </FileUpload.Root>;
  if (name === 'NumberInput') {
    const formatOptions = p.format === 'currency' ? { style: 'currency', currency: String(p.currency ?? 'CNY') } : p.format === 'percent' ? { style: 'unit', unit: 'percent' } : undefined;
    return <NumberInput.Root width="100%" defaultValue={String(p.defaultValue ?? '0')} min={Number(p.min ?? 0)} max={Number(p.max ?? 100)} step={Number(p.step ?? 1)} size={p.size} disabled={!preview} formatOptions={formatOptions as any}><NumberInput.Control /><NumberInput.Input /></NumberInput.Root>;
  }
  if (name === 'PasswordInput') return <ChakraPasswordInput props={p} preview={preview} />;
  if (name === 'PinInput') return <PinInput.Root width="100%" disabled={!preview} type={p.type ?? 'numeric'} mask={Boolean(p.mask)} size={p.size}><PinInput.HiddenInput /><PinInput.Control>{Array.from({ length: Number(p.count ?? 4) }, (_, index) => <PinInput.Input key={index} index={index} />)}</PinInput.Control></PinInput.Root>;
  if (name === 'RadioCard') return <RadioCard.Root width="100%" defaultValue={String(p.defaultValue ?? 'react')} disabled={!preview} colorPalette={p.colorPalette} variant={p.variant} size={p.size}>
    <RadioCard.Label>选择技术框架</RadioCard.Label><Flex direction={p.orientation === 'vertical' ? 'column' : 'row'} gap="2" align="stretch">{frameworkOptions.slice(0, 3).map((option) => <RadioCard.Item key={option.value} value={option.value} flex="1"><RadioCard.ItemHiddenInput /><RadioCard.ItemControl><RadioCard.ItemText>{option.label}</RadioCard.ItemText><RadioCard.ItemIndicator /></RadioCard.ItemControl></RadioCard.Item>)}</Flex>
  </RadioCard.Root>;
  if (name === 'Rating') return <RatingGroup.Root count={Number(p.count ?? 5)} defaultValue={Number(p.defaultValue ?? 3)} size={p.size} colorPalette={p.colorPalette} allowHalf={Boolean(p.allowHalf)} disabled={!preview}><RatingGroup.HiddenInput /><RatingGroup.Control /></RatingGroup.Root>;
  if (name === 'SegmentedControl') return <SegmentGroup.Root width="100%" defaultValue={String(p.defaultValue ?? '')} orientation={p.orientation ?? 'horizontal'} size={p.size} disabled={!preview}><SegmentGroup.Indicator />{items.map((item) => <SegmentGroup.Item key={String(item.value)} value={String(item.value)}><SegmentGroup.ItemText>{String(item.label)}</SegmentGroup.ItemText><SegmentGroup.ItemHiddenInput /></SegmentGroup.Item>)}</SegmentGroup.Root>;
  if (name === 'TagsInput') return <TagsInput.Root width="100%" defaultValue={runtimeStrings(p.defaultValue)} max={Number(p.max ?? 10)} size={p.size} disabled={!preview}><TagsInput.Label>{String(p.label ?? '标签')}</TagsInput.Label><TagsInput.Control><TagsInput.Items /><TagsInput.Input placeholder={String(p.placeholder ?? '添加标签…')} /></TagsInput.Control><Text textStyle="xs" color="fg.muted" textAlign="right">按回车添加标签</Text></TagsInput.Root>;
  if (name === 'Combobox') return <ChakraCombobox props={p} preview={preview} />;
  if (name === 'Listbox') {
    const options = optionsForKind(p.dataKind); const collection = createListCollection({ items: options });
    return <Listbox.Root collection={collection} width="100%" height="100%" defaultValue={runtimeStrings(p.defaultValue)} selectionMode={p.selectionMode ?? 'single'} orientation={p.orientation ?? 'vertical'} disabled={!preview}><Listbox.Label>{String(p.label ?? '选择项目')}</Listbox.Label><Listbox.Content>{collection.items.map((option) => <Listbox.Item key={option.value} item={option}><Listbox.ItemText>{option.label}</Listbox.ItemText><Listbox.ItemIndicator /></Listbox.Item>)}</Listbox.Content></Listbox.Root>;
  }
  if (name === 'Select') {
    const options = optionsForKind(p.dataKind); const collection = createListCollection({ items: options });
    return <Select.Root collection={collection} width="100%" multiple={Boolean(p.multiple)} size={p.size} variant={p.variant} disabled={!preview}><Select.HiddenSelect /><Select.Label>{String(p.label ?? '选择项目')}</Select.Label><Select.Control><Select.Trigger><Select.ValueText placeholder={String(p.placeholder ?? '请选择')} /></Select.Trigger><Select.IndicatorGroup><Select.ClearTrigger /><Select.Indicator /></Select.IndicatorGroup></Select.Control><Portal><Select.Positioner><Select.Content>{collection.items.map((option) => <Select.Item key={option.value} item={option}>{option.label}<Select.ItemIndicator /></Select.Item>)}</Select.Content></Select.Positioner></Portal></Select.Root>;
  }
  if (name === 'TreeView') return <TreeView.Root collection={fileTree} width="100%" height="100%" selectionMode={p.selectionMode ?? 'single'} defaultExpandedValue={runtimeStrings(p.defaultExpandedValue)} pointerEvents={preview ? 'auto' : 'none'}><TreeView.Label>{String(p.label ?? '树视图')}</TreeView.Label><TreeView.Tree><TreeView.Node indentGuide={p.showGuide ? <TreeView.BranchIndentGuide /> : undefined} render={({ node, nodeState }) => nodeState.isBranch ? <TreeView.BranchControl><Folder /><TreeView.BranchText>{String((node as AnyProps).name)}</TreeView.BranchText></TreeView.BranchControl> : <TreeView.Item><File /><TreeView.ItemText>{String((node as AnyProps).name)}</TreeView.ItemText></TreeView.Item>} /></TreeView.Tree></TreeView.Root>;
  if (name === 'Input') return <Input width="100%" height="100%" placeholder={component.content} variant={p.variant} size={p.size} />;
  if (name === 'Textarea') return <Textarea width="100%" height="100%" placeholder={component.content} variant={p.variant} size={p.size} rows={Number(p.rows ?? 4)} />;
  if (name === 'NativeSelect') return <NativeSelect.Root width="100%" height="100%" variant={p.variant} size={p.size}><NativeSelect.Field defaultValue={runtimeRecords(p.options)[0]?.value as string}>{runtimeRecords(p.options).map((option) => <option key={String(option.value)} value={String(option.value)}>{String(option.label)}</option>)}</NativeSelect.Field><NativeSelect.Indicator /></NativeSelect.Root>;
  if (name === 'Checkbox') return <Checkbox.Root defaultChecked={Boolean(p.defaultChecked)} colorPalette={p.colorPalette} variant={p.variant} size={p.size}><Checkbox.HiddenInput /><Checkbox.Control><Checkbox.Indicator /></Checkbox.Control><Checkbox.Label>{component.content}</Checkbox.Label></Checkbox.Root>;
  if (name === 'Switch') return <Switch.Root defaultChecked={Boolean(p.defaultChecked)} colorPalette={p.colorPalette} variant={p.variant} size={p.size}><Switch.HiddenInput /><Switch.Control><Switch.Thumb /></Switch.Control><Switch.Label>{component.content}</Switch.Label></Switch.Root>;
  if (name === 'RadioGroup') return <RadioGroup.Root defaultValue={String(p.defaultValue ?? '')} variant={p.variant} size={p.size}><Flex direction={p.orientation === 'vertical' ? 'column' : 'row'} gap={p.orientation === 'vertical' ? 3 : 5} align={p.orientation === 'vertical' ? 'start' : 'center'} wrap="wrap">{runtimeRecords(p.options).map((option) => <RadioGroup.Item key={String(option.value)} value={String(option.value)}><RadioGroup.ItemHiddenInput /><RadioGroup.ItemIndicator /><RadioGroup.ItemText>{String(option.label)}</RadioGroup.ItemText></RadioGroup.Item>)}</Flex></RadioGroup.Root>;
  if (name === 'Slider') return <Slider.Root width={p.orientation === 'vertical' ? undefined : '100%'} height={p.orientation === 'vertical' ? '100%' : undefined} defaultValue={[Number(p.defaultValue ?? 50)]} min={Number(p.min ?? 0)} max={Number(p.max ?? 100)} colorPalette={p.colorPalette} variant={p.variant} size={p.size} orientation={p.orientation ?? 'horizontal'}><Slider.Control><Slider.Track><Slider.Range /></Slider.Track><Slider.Thumb index={0} /></Slider.Control></Slider.Root>;
  if (name === 'Fieldset') return <Fieldset.Root {...fill} padding="4" borderWidth="1px" borderRadius="lg" size={p.size} disabled={Boolean(p.disabled)}><Fieldset.Legend>{String(p.legend ?? '字段组')}</Fieldset.Legend><Fieldset.HelperText>{String(p.helperText ?? '')}</Fieldset.HelperText>{slotContent.content ?? <Stack mt="4" gap="3"><Input placeholder="姓名" /><Input placeholder="电子邮箱" /><Button alignSelf="start" colorPalette="blue">保存</Button></Stack>}</Fieldset.Root>;
  if (name === 'Editable') return <Editable.Root width="100%" defaultValue={component.content} placeholder={String(p.placeholder ?? '')} size={p.size} activationMode={preview ? p.activationMode ?? 'click' : 'none'}><Editable.Area><Editable.Preview width="100%" minHeight="10" padding="2" borderBottomWidth="1px" /><Editable.Input width="100%" /></Editable.Area></Editable.Root>;

  if (name === 'Breadcrumb') return <Breadcrumb.Root variant={p.variant} size={p.size}><Breadcrumb.List>{items.map((item, index) => <Breadcrumb.Item key={String(item.key)}>{index === items.length - 1 ? <Breadcrumb.CurrentLink>{String(item.label)}</Breadcrumb.CurrentLink> : <Breadcrumb.Link>{String(item.label)}</Breadcrumb.Link>}{index < items.length - 1 && <Breadcrumb.Separator>{String(p.separator ?? '/')}</Breadcrumb.Separator>}</Breadcrumb.Item>)}</Breadcrumb.List></Breadcrumb.Root>;
  if (name === 'Pagination') return <Pagination.Root count={Number(p.count ?? 100)} pageSize={Number(p.pageSize ?? 10)} defaultPage={Number(p.defaultPage ?? 1)} siblingCount={Number(p.siblingCount ?? 1)}><Flex align="center" gap="1" height="100%"><Pagination.PrevTrigger asChild><IconButton size={p.size ?? 'sm'} variant={p.variant ?? 'outline'} aria-label="上一页"><ChevronLeft /></IconButton></Pagination.PrevTrigger><Pagination.Items render={(page) => <IconButton size={p.size ?? 'sm'} variant={{ base: 'ghost', _selected: p.variant === 'solid' ? 'solid' : 'outline' } as any} aria-label={`第 ${page.value} 页`}>{page.value}</IconButton>} /><Pagination.NextTrigger asChild><IconButton size={p.size ?? 'sm'} variant={p.variant ?? 'outline'} aria-label="下一页"><ChevronRight /></IconButton></Pagination.NextTrigger></Flex></Pagination.Root>;
  if (name === 'Steps') return <Steps.Root width="100%" height="100%" count={items.length} defaultStep={Number(p.defaultStep ?? 1)} orientation={p.orientation ?? 'horizontal'} size={p.size} variant={p.variant}><Steps.List>{items.map((item, index) => <Steps.Item key={String(item.key)} index={index}><Steps.Trigger><Steps.Indicator><Steps.Status complete={<Check />} incomplete={<Steps.Number />} /></Steps.Indicator><Box><Steps.Title>{String(item.label)}</Steps.Title>{item.description && <Steps.Description>{String(item.description)}</Steps.Description>}</Box></Steps.Trigger><Steps.Separator /></Steps.Item>)}</Steps.List></Steps.Root>;
  if (name === 'Tabs') return <Tabs.Root {...fill} defaultValue={String(p.defaultValue ?? items[0]?.key ?? '')} variant={p.variant} size={p.size} fitted={Boolean(p.fitted)} justify={p.justify}><Tabs.List>{items.map((item) => <Tabs.Trigger key={String(item.key)} value={String(item.key)}>{String(item.label)}</Tabs.Trigger>)}<Tabs.Indicator /></Tabs.List>{items.map((item) => <Tabs.Content key={String(item.key)} value={String(item.key)} padding="3">{slotContent[`tab-${String(item.key)}`] ?? <Text color="fg.muted">{String(item.label)}内容</Text>}</Tabs.Content>)}</Tabs.Root>;
  if (name === 'Accordion') return <Accordion.Root {...fill} defaultValue={Array.isArray(p.defaultValue) ? p.defaultValue.map(String) : [String(p.defaultValue ?? items[0]?.key ?? '')]} multiple={Boolean(p.multiple)} collapsible={p.collapsible !== false} variant={p.variant} size={p.size}>{items.map((item) => <Accordion.Item key={String(item.key)} value={String(item.key)}><Accordion.ItemTrigger><Text flex="1">{String(item.label)}</Text><Accordion.ItemIndicator /></Accordion.ItemTrigger><Accordion.ItemContent><Accordion.ItemBody>{slotContent[`panel-${String(item.key)}`] ?? <Text color="fg.muted">{String(item.label)}的详细内容。</Text>}</Accordion.ItemBody></Accordion.ItemContent></Accordion.Item>)}</Accordion.Root>;
  if (name === 'Collapsible') return <Collapsible.Root {...fill} defaultOpen={Boolean(p.defaultOpen)} disabled={!preview}><Collapsible.Trigger asChild><Button variant="ghost" width="100%" justifyContent="space-between">{component.content}<Collapsible.Indicator>⌄</Collapsible.Indicator></Button></Collapsible.Trigger><Collapsible.Content borderTopWidth="1px" padding="3">{slotContent.content ?? <Text color="fg.muted">这里是可以继续设计的折叠内容。</Text>}</Collapsible.Content></Collapsible.Root>;
  if (name === 'Carousel') return <Carousel.Root {...fill} slideCount={Number(p.slideCount ?? 5)} slidesPerPage={Number(p.slidesPerPage ?? 1)} loop={Boolean(p.loop)} autoplay={Boolean(p.autoplay)}><Carousel.ItemGroup>{Array.from({ length: Number(p.slideCount ?? 5) }, (_, index) => <Carousel.Item key={index} index={index} padding="1">{slotContent[`slide-${index + 1}`] ?? <Center height="100%" minHeight="36" borderRadius="lg" background={['blue.100', 'purple.100', 'teal.100', 'orange.100'][index % 4]} color={['blue.800', 'purple.800', 'teal.800', 'orange.800'][index % 4]} fontSize="4xl" fontWeight="bold">{index + 1}</Center>}</Carousel.Item>)}</Carousel.ItemGroup><Carousel.Control justifyContent="center" gap="3"><Carousel.PrevTrigger asChild><IconButton aria-label="上一张" size="xs" variant="ghost" disabled={!preview}><ChevronLeft /></IconButton></Carousel.PrevTrigger><Carousel.Indicators /><Carousel.NextTrigger asChild><IconButton aria-label="下一张" size="xs" variant="ghost" disabled={!preview}><ChevronRight /></IconButton></Carousel.NextTrigger></Carousel.Control></Carousel.Root>;

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
  if (name === 'Marquee') return <Marquee.Root {...fill} side={p.side ?? 'left'} reverse={Boolean(p.reverse)} speed={Number(p.speed ?? 40)} pauseOnInteraction={p.pauseOnInteraction !== false}><Marquee.Viewport>{p.edge && <Marquee.Edge side="start" />}<Marquee.Content>{['Chakra UI', 'Ant Design', 'shadcn/ui', 'AI Design'].map((label) => <Marquee.Item key={label} paddingInline="4"><Badge size="lg" variant="surface" colorPalette="purple">{label}</Badge></Marquee.Item>)}</Marquee.Content>{p.edge && <Marquee.Edge side="end" />}</Marquee.Viewport></Marquee.Root>;
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

  if (name === 'ActionBar') return <><Button width="100%" height="100%" variant="outline" onClick={() => preview && setOpen((value) => !value)}>{component.content}</Button><ActionBar.Root open={open}><Portal><ActionBar.Positioner><ActionBar.Content>{slotContent.content ?? <><ActionBar.SelectionTrigger>{Number(p.selectedCount ?? 0)} 项已选择</ActionBar.SelectionTrigger><ActionBar.Separator />{runtimeStrings(p.actions).map((action) => <Button key={action} size="sm" variant="outline" onClick={() => setOpen(false)}>{action === '删除' ? <Trash2 /> : <Share2 />}{action}</Button>)}</>}</ActionBar.Content></ActionBar.Positioner></Portal></ActionBar.Root></>;
  if (name === 'FloatingPanel') return <FloatingPanel.Root defaultOpen={preview && Boolean(p.defaultOpen)}><FloatingPanel.Trigger asChild><Button width="100%" height="100%" variant="outline" disabled={!preview}>{component.content}</Button></FloatingPanel.Trigger><Portal><FloatingPanel.Positioner><FloatingPanel.Content><FloatingPanel.Header><FloatingPanel.DragTrigger><GripHorizontal /><FloatingPanel.Title>{String(p.title ?? '浮动面板')}</FloatingPanel.Title></FloatingPanel.DragTrigger><FloatingPanel.Control><FloatingPanel.StageTrigger stage="minimized" asChild><IconButton aria-label="最小化" variant="ghost" size="2xs"><Minus /></IconButton></FloatingPanel.StageTrigger><FloatingPanel.StageTrigger stage="maximized" asChild><IconButton aria-label="最大化" variant="ghost" size="2xs"><Square /></IconButton></FloatingPanel.StageTrigger><FloatingPanel.StageTrigger stage="default" asChild><IconButton aria-label="恢复" variant="ghost" size="2xs"><Maximize2 /></IconButton></FloatingPanel.StageTrigger><FloatingPanel.CloseTrigger asChild><IconButton aria-label="关闭面板" variant="ghost" size="2xs"><X /></IconButton></FloatingPanel.CloseTrigger></FloatingPanel.Control></FloatingPanel.Header><FloatingPanel.Body>{slotContent.content ?? <Text color="fg.muted">拖动标题栏移动面板，也可以缩放和最大化。</Text>}</FloatingPanel.Body><FloatingPanel.ResizeTriggers /></FloatingPanel.Content></FloatingPanel.Positioner></Portal></FloatingPanel.Root>;
  if (name === 'HoverCard') return <HoverCard.Root openDelay={Number(p.openDelay ?? 250)} closeDelay={Number(p.closeDelay ?? 150)} disabled={!preview}><HoverCard.Trigger asChild><Link href="#" onClick={(event) => event.preventDefault()}>{component.content}</Link></HoverCard.Trigger><Portal><HoverCard.Positioner><HoverCard.Content><HoverCard.Arrow />{slotContent.popup ?? <Flex gap="3"><Avatar.Root><Avatar.Fallback name={String(p.title)} /></Avatar.Root><Stack gap="1"><Text fontWeight="semibold">{String(p.title)}</Text><Text textStyle="sm" color="fg.muted">{String(p.description)}</Text></Stack></Flex>}</HoverCard.Content></HoverCard.Positioner></Portal></HoverCard.Root>;
  if (name === 'OverlayManager') return <><Button width="100%" height="100%" onClick={() => preview && managedOverlay.open(`overlay-${component.id}`, { title: p.title, description: p.description, kind: p.kind, content: slotContent.content })}>{component.content}</Button><managedOverlay.Viewport /></>;
  if (name === 'ToggleTip') return <ChakraPopover.Root open={!preview ? false : undefined} positioning={{ gutter: 4 }}><ChakraPopover.Trigger asChild><Button width="100%" height="100%" variant="outline">{component.content}</Button></ChakraPopover.Trigger><Portal><ChakraPopover.Positioner><ChakraPopover.Content width="auto" padding="2 3" textStyle={p.size === 'md' ? 'sm' : 'xs'}>{p.showArrow && <ChakraPopover.Arrow><ChakraPopover.ArrowTip /></ChakraPopover.Arrow>}{slotContent.popup ?? String(p.content)}</ChakraPopover.Content></ChakraPopover.Positioner></Portal></ChakraPopover.Root>;
  if (name === 'Dialog' || name === 'Drawer') return <><Button ref={triggerRef} width="100%" height="100%" colorPalette="blue" onClick={() => preview && setOpen(true)}>{component.content}</Button><DesignOverlay anchorRef={triggerRef} open={open} side={name === 'Dialog' ? 'center' : String(p.placement ?? 'right')} title={String(p.title ?? name)} className="chakra-overlay" onClose={() => setOpen(false)} footer={<><Button variant="outline" onClick={() => setOpen(false)}>取消</Button><Button colorPalette="blue" onClick={() => setOpen(false)}>保存</Button></>}>{slotContent.content ?? <Stack gap="4"><Text color="fg.muted">这是 Chakra UI 的真实交互浮层，可以继续放入表单或展示组件。</Text><Input placeholder="输入内容" /></Stack>}</DesignOverlay></>;
  if (name === 'Popover') return <><Button ref={triggerRef} width="100%" height="100%" variant="outline" onClick={() => preview && setOpen(!open)}>{component.content}</Button><FloatingSurface anchorRef={triggerRef} open={open} placement={String(p.placement ?? 'bottom')} className="chakra-floating"><Stack gap="3"><Heading size="sm">{String(p.title ?? '气泡内容')}</Heading>{slotContent.popup ?? <Text color="fg.muted" textStyle="sm">这是可以继续设计的 Chakra Popover 内容。</Text>}<Button size="sm" onClick={() => setOpen(false)}>完成</Button></Stack></FloatingSurface></>;
  if (name === 'Tooltip') return <><Button ref={hoverRef} width="100%" height="100%" variant="outline" onMouseEnter={() => preview && setOpen(true)} onMouseLeave={() => setOpen(false)}>{component.content}</Button><FloatingSurface anchorRef={hoverRef} open={open} placement={String(p.placement ?? 'top')} className="chakra-tooltip-surface">{String(p.content)}</FloatingSurface></>;
  if (name === 'Menu') return <><Button ref={triggerRef} width="100%" height="100%" variant="outline" onClick={() => preview && setOpen(!open)}>{component.content}⌄</Button><FloatingSurface anchorRef={triggerRef} open={open} className="chakra-menu-surface">{items.map((item, index) => <div key={String(item.key)}>{p.kind === 'grouped' && (index === 0 || items[index - 1]?.group !== item.group) && <small>{String(item.group)}</small>}<button onClick={() => setOpen(false)}>{item.icon && <span>{String(item.icon)}</span>}{p.kind === 'checkbox' && <span>{item.checked ? '✓' : ''}</span>}{String(item.label)}</button></div>)}</FloatingSurface></>;

  if (name === 'LocaleProvider') return <LocaleProvider locale={String(p.locale ?? 'zh-CN')}><Stack {...fill} dir={p.direction ?? 'ltr'} padding="4" borderWidth="1px" borderRadius="lg" gap="3"><Heading size="lg">{String(p.title)}</Heading><Text color="fg.muted">{p.direction === 'rtl' ? 'يتم ترتيب المحتوى وعناصر التحكم من اليمين إلى اليسار.' : '日期、数字和布局方向会遵循当前语言环境。'}</Text><Slider.Root defaultValue={[65]} disabled={!preview}><Slider.Control><Slider.Track><Slider.Range /></Slider.Track><Slider.Thumb index={0} /></Slider.Control></Slider.Root></Stack></LocaleProvider>;
  if (name === 'FormatNumber') return <LocaleProvider locale={String(p.locale ?? 'zh-CN')}><Center {...fill}><Text textStyle="2xl" fontWeight="semibold"><FormatNumber value={Number(p.value ?? 0)} style={p.style ?? 'decimal'} currency={p.currency} notation={p.notation} maximumFractionDigits={p.maximumFractionDigits} /></Text></Center></LocaleProvider>;
  if (name === 'FormatByte') return <LocaleProvider locale={String(p.locale ?? 'zh-CN')}><Center {...fill}><Text textStyle="xl"><FormatByte value={Number(p.value ?? 0)} unitSystem={p.unitSystem ?? 'decimal'} unitDisplay={p.unitDisplay ?? 'short'} /></Text></Center></LocaleProvider>;
  if (name === 'Checkmark') return <Center {...fill}><Checkmark checked={Boolean(p.checked)} indeterminate={Boolean(p.indeterminate)} disabled={Boolean(p.disabled)} size={p.size} colorPalette={p.colorPalette} /></Center>;
  if (name === 'ClientOnly') return <ClientOnly fallback={<Center {...fill}><Spinner size="sm" /><Text ml="2">{String(p.fallback)}</Text></Center>}><Center {...fill} borderWidth="1px" borderRadius="lg"><CheckCircle2 color="#22C55E" /><Text ml="2">{p.kind === 'time' ? new Date().toLocaleTimeString() : p.kind === 'viewport' ? '客户端视口已连接' : '此内容仅在客户端渲染'}</Text></Center></ClientOnly>;
  if (name === 'EnvironmentProvider') return <EnvironmentProvider value={() => globalThis.document}><Stack {...fill} padding="4" borderWidth="1px" borderRadius="lg" gap="2"><Badge width="fit-content" colorPalette="teal">Environment</Badge><Heading size="sm">{String(p.label)}</Heading><Text textStyle="sm" color="fg.muted">Portal、浮层和交互组件会从这里取得 Window 与 Document。</Text></Stack></EnvironmentProvider>;
  if (name === 'For') return <Flex {...fill} direction={p.kind === 'rows' ? 'column' : 'row'} gap="2" flexWrap="wrap"><For each={Array.from({ length: Number(p.count ?? 4) }, (_, index) => index + 1)}>{(value) => p.kind === 'tags' ? <Tag.Root key={value} colorPalette="purple" variant="subtle"><Tag.Label>标签 {value}</Tag.Label></Tag.Root> : p.kind === 'rows' ? <Flex key={value} justify="space-between" width="100%" padding="2 3" borderWidth="1px" borderRadius="md"><Text>数据行 {value}</Text><Badge>可用</Badge></Flex> : <Center key={value} flex="1" minWidth="16" minHeight="16" borderRadius="lg" background="blue.50" color="blue.700" fontWeight="bold">{value}</Center>}</For></Flex>;
  if (name === 'Presence') return <Stack {...fill} align="center" justify="center" gap="3"><Button size="sm" variant="outline" disabled={!preview} onClick={() => setVisible((value) => !value)}>{component.content}</Button><Presence present={visible} lazyMount={Boolean(p.lazyMount)} unmountOnExit={Boolean(p.unmountOnExit)} animationName={p.animation === 'scale' ? { _open: 'scale-fade-in', _closed: 'scale-fade-out' } : { _open: 'fade-in', _closed: 'fade-out' }} animationDuration="moderate"><Box padding="4" borderRadius="lg" background="purple.100" color="purple.800">Presence 内容区域</Box></Presence></Stack>;
  if (name === 'Portal') return <><Button ref={triggerRef} width="100%" height="100%" variant="outline" onClick={() => preview && setOpen((value) => !value)}>{component.content}</Button>{open && <Portal><Box position="fixed" zIndex="2000" top={String(p.placement).startsWith('top') ? '80px' : undefined} bottom={String(p.placement).startsWith('bottom') ? '32px' : undefined} left={String(p.placement).endsWith('center') ? '50%' : undefined} right={String(p.placement).endsWith('end') ? '32px' : undefined} transform={String(p.placement).endsWith('center') ? 'translateX(-50%)' : undefined} padding="4" borderWidth="1px" borderRadius="xl" background="bg.panel" shadow="xl"><Flex align="center" gap="2"><Badge colorPalette="purple">Portal</Badge><Text>{p.kind === 'panel' ? '内容已传送到页面顶层' : p.kind === 'message' ? '这是一条传送消息' : '传送徽标'}</Text><CloseButton size="sm" onClick={() => setOpen(false)} /></Flex></Box></Portal>}</>;
  if (name === 'Radiomark') return <Center {...fill}><Radiomark checked={Boolean(p.checked)} disabled={Boolean(p.disabled)} size={p.size} colorPalette={p.colorPalette} /></Center>;
  if (name === 'Show') return <Stack {...fill} align="center" justify="center" gap="3"><Button variant="outline" disabled={!preview} onClick={() => setCounter((value) => value + 1)}>当前值：{counter}</Button><Show when={counter >= Number(p.threshold ?? 0)} fallback={<Text color="fg.muted">达到 {Number(p.threshold ?? 0)} 后显示内容</Text>}><Alert.Root status="success" variant="subtle"><Alert.Indicator /><Alert.Title>{String(p.label)}</Alert.Title></Alert.Root></Show></Stack>;
  if (name === 'SkipNav') return <Box {...fill} position="relative" overflow="auto"><SkipNavLink>{String(p.label)}</SkipNavLink><Box padding="4" background="gray.100" borderRadius="md" mb="4"><Text fontWeight="medium">{String(p.navLabel)}</Text><Text textStyle="sm" color="fg.muted">键盘用户可以跳过这块重复导航。</Text></Box><SkipNavContent><Box padding="4" background="blue.50" borderRadius="md"><Text fontWeight="medium">{String(p.contentLabel)}</Text><Text textStyle="sm">焦点会直接移动到这里。</Text></Box></SkipNavContent></Box>;
  if (name === 'VisuallyHidden') {
    const A11yIcon = p.icon === 'settings' ? Settings : p.icon === 'check' ? CheckCircle2 : Bell;
    return <Button {...fill} variant="outline"><A11yIcon />{String(p.visibleText)}<VisuallyHidden>{String(p.hiddenText)}</VisuallyHidden></Button>;
  }
  if (name === 'Theme') return <Theme {...fill} appearance={p.appearance ?? 'dark'} colorPalette={p.colorPalette ?? 'teal'} padding="4" borderRadius="lg"><Stack gap="3"><Heading size="md">局部 {p.appearance === 'dark' ? '深色' : '浅色'}主题</Heading><Text color="fg.muted">只影响这个容器内部的组件外观。</Text><Button variant="surface" alignSelf="start">主题按钮</Button></Stack></Theme>;

  return <Box {...fill} borderWidth="1px" borderRadius="md" display="grid" placeItems="center"><Badge colorPalette="teal">Chakra UI · {name}</Badge></Box>;
}
