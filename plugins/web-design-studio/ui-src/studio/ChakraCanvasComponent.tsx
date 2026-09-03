import { useRef, useState, type ReactNode } from 'react';
import {
  Accordion, Alert, Avatar, Badge, Box, Button, Card, ChakraProvider, Checkbox, Code, Container, Fieldset, Flex,
  Grid, Heading, Input, Kbd, Link, NativeSelect, Progress, RadioGroup, Separator, Skeleton, Slider, Spinner, Stack,
  Switch, Tabs, Text, Textarea, defaultSystem
} from '@chakra-ui/react';
import type { WebDesignComponent, WebDesignTokens } from '../../src/schema';
import { DesignOverlay, FloatingSurface, SimpleDataTable, SkeletonComposition, runtimeRecords, runtimeRows, runtimeStrings } from './LibraryRuntimePrimitives';

type AnyProps = Record<string, any>;

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
  const [expanded, setExpanded] = useState(Boolean(p.defaultOpen));
  const [editable, setEditable] = useState(component.content);
  const fill = { width: '100%', height: '100%' };
  const items = runtimeRecords(p.items);

  if (name === 'Box') return <Box {...fill} padding="4" borderWidth="1px" borderRadius="lg" bg="white">{slotContent.content ?? <Text color="fg.muted">Box 内容区域</Text>}</Box>;
  if (name === 'Container') return <Container {...fill} maxW={p.maxWidth ?? 'lg'} centerContent={p.centerContent} padding="4" borderWidth="1px" borderRadius="lg">{slotContent.content ?? <Text color="fg.muted">Container 内容区域</Text>}</Container>;
  if (name === 'Flex') return <Flex {...fill} direction={p.direction} gap={p.gap} align={p.align} justify={p.justify} wrap={p.wrap} padding="3" borderWidth="1px" borderRadius="lg">{slotContent.content ?? <><Button size="sm">取消</Button><Button size="sm" colorPalette="blue">确定</Button><Badge>标签</Badge></>}</Flex>;
  if (name === 'Grid' || name === 'SimpleGrid') {
    const columns = Math.max(1, Number(p.columns ?? 3));
    return <Grid {...fill} templateColumns={`repeat(${columns}, minmax(0, 1fr))`} gap={p.gap ?? 4} padding="3" borderWidth="1px" borderRadius="lg">{slotContent.content ?? Array.from({ length: columns }, (_, index) => <Box key={index} borderRadius="md" bg="blue.50" color="blue.700" display="grid" placeItems="center">{index + 1}</Box>)}</Grid>;
  }
  if (name === 'Stack') return <Stack {...fill} direction={p.direction ?? 'column'} gap={p.gap ?? 4} align={p.align} padding="3" borderWidth="1px" borderRadius="lg">{slotContent.content ?? <><Button size="sm" variant="outline">第一项</Button><Button size="sm" variant="outline">第二项</Button><Button size="sm" variant="outline">第三项</Button></>}</Stack>;
  if (name === 'Group') return <Flex {...fill} align="center" gap={p.attached ? 0 : 2} padding="2">{slotContent.content ?? <><Button size="sm" borderEndRadius={p.attached ? 0 : undefined}>上一页</Button><Button size="sm" variant="outline" borderStartRadius={p.attached ? 0 : undefined}>下一页</Button></>}</Flex>;
  if (name === 'Separator') return <Flex {...fill} align="center"><Separator width="100%" orientation={p.orientation ?? 'horizontal'} variant={p.variant ?? 'solid'} /></Flex>;
  if (name === 'ScrollArea') return <Box {...fill} overflow="auto" borderWidth="1px" borderRadius="lg" padding="3">{slotContent.content ?? <Stack gap="3">{Array.from({ length: 9 }, (_, index) => <Box key={index} padding="3" borderWidth="1px" borderRadius="md"><Text fontWeight="medium">滚动内容 {index + 1}</Text><Text textStyle="sm" color="fg.muted">这是可以向下滚动查看的 Chakra 内容。</Text></Box>)}</Stack>}</Box>;
  if (name === 'Splitter') return <Flex {...fill} borderWidth="1px" borderRadius="lg" overflow="hidden"><Box width="45%" padding="3" bg="gray.50">{slotContent['panel-1'] ?? '面板一'}</Box><Box width="5px" bg="gray.200" cursor="col-resize" /><Box flex="1" padding="3">{slotContent['panel-2'] ?? '面板二'}</Box></Flex>;

  if (name === 'Heading') return <Heading as={`h${Math.min(6, Math.max(1, Number(p.level ?? 2)))}` as any} size={p.size ?? '2xl'}>{component.content}</Heading>;
  if (name === 'Text') return <Text textStyle={p.textStyle ?? 'md'}>{component.content}</Text>;
  if (name === 'Code') return <Code variant={p.variant} colorPalette={p.colorPalette} padding="2" borderRadius="md">{component.content}</Code>;
  if (name === 'Blockquote') return <Box as="blockquote" {...fill} borderStartWidth="4px" borderColor="blue.400" padding="4" bg="blue.50" borderRadius="md"><Text fontStyle="italic">“{component.content}”</Text><Text mt="2" textStyle="sm" color="fg.muted">— {String(p.cite ?? 'Web Design Studio')}</Text></Box>;
  if (name === 'Kbd') return <Kbd>{component.content}</Kbd>;
  if (name === 'Link') return <Link colorPalette={p.colorPalette ?? 'blue'} variant={p.variant ?? 'underline'}>{component.content}</Link>;
  if (name === 'List') return <Stack as="ul" gap="2" paddingStart="5">{runtimeStrings(p.items).map((item) => <Box as="li" key={item}>{item}</Box>)}</Stack>;

  if (name === 'Button') return <Button width="100%" height="100%" variant={p.variant} colorPalette={p.colorPalette} size={p.size}>{component.content}</Button>;
  if (name === 'IconButton') return <Button width="100%" height="100%" variant={p.variant} colorPalette={p.colorPalette} size={p.size} aria-label="快捷操作">{component.content}</Button>;
  if (name === 'Input') return <Input width="100%" height="100%" placeholder={component.content} variant={p.variant} size={p.size} />;
  if (name === 'Textarea') return <Textarea width="100%" height="100%" placeholder={component.content} variant={p.variant} size={p.size} />;
  if (name === 'NativeSelect') return <NativeSelect.Root width="100%" height="100%" variant={p.variant} size={p.size}><NativeSelect.Field defaultValue={runtimeRecords(p.options)[0]?.value as string}>{runtimeRecords(p.options).map((option) => <option key={String(option.value)} value={String(option.value)}>{String(option.label)}</option>)}</NativeSelect.Field><NativeSelect.Indicator /></NativeSelect.Root>;
  if (name === 'Checkbox') return <Checkbox.Root defaultChecked={Boolean(p.defaultChecked)} colorPalette={p.colorPalette}><Checkbox.HiddenInput /><Checkbox.Control><Checkbox.Indicator /></Checkbox.Control><Checkbox.Label>{component.content}</Checkbox.Label></Checkbox.Root>;
  if (name === 'Switch') return <Switch.Root defaultChecked={Boolean(p.defaultChecked)} colorPalette={p.colorPalette}><Switch.HiddenInput /><Switch.Control><Switch.Thumb /></Switch.Control><Switch.Label>{component.content}</Switch.Label></Switch.Root>;
  if (name === 'RadioGroup') return <RadioGroup.Root defaultValue={String(p.defaultValue ?? '')}><Flex gap="5" align="center" wrap="wrap">{runtimeRecords(p.options).map((option) => <RadioGroup.Item key={String(option.value)} value={String(option.value)}><RadioGroup.ItemHiddenInput /><RadioGroup.ItemIndicator /><RadioGroup.ItemText>{String(option.label)}</RadioGroup.ItemText></RadioGroup.Item>)}</Flex></RadioGroup.Root>;
  if (name === 'Slider') return <Slider.Root width="100%" defaultValue={[Number(p.defaultValue ?? 50)]} min={Number(p.min ?? 0)} max={Number(p.max ?? 100)} colorPalette={p.colorPalette}><Slider.Control><Slider.Track><Slider.Range /></Slider.Track><Slider.Thumb index={0} /></Slider.Control></Slider.Root>;
  if (name === 'Fieldset') return <Fieldset.Root {...fill} padding="4" borderWidth="1px" borderRadius="lg"><Fieldset.Legend>{String(p.legend ?? '字段组')}</Fieldset.Legend><Fieldset.HelperText>{String(p.helperText ?? '')}</Fieldset.HelperText>{slotContent.content ?? <Stack mt="4" gap="3"><Input placeholder="姓名" /><Input placeholder="电子邮箱" /><Button alignSelf="start" colorPalette="blue">保存</Button></Stack>}</Fieldset.Root>;
  if (name === 'Editable') return <Input value={editable} onChange={(event) => setEditable(event.target.value)} readOnly={!preview} width="100%" height="100%" variant="flushed" />;

  if (name === 'Breadcrumb') return <Flex align="center" gap="2" height="100%">{items.map((item, index) => <Flex key={String(item.key)} align="center" gap="2"><Link color={index === items.length - 1 ? 'fg' : 'fg.muted'}>{String(item.label)}</Link>{index < items.length - 1 && <Text color="fg.subtle">/</Text>}</Flex>)}</Flex>;
  if (name === 'Pagination') return <Flex align="center" gap="1" height="100%"><Button size="xs" variant="outline">‹</Button>{Array.from({ length: 5 }, (_, index) => index + 1).map((page) => <Button key={page} size="xs" variant={page === Number(p.defaultPage ?? 3) ? 'solid' : 'ghost'} colorPalette="blue">{page}</Button>)}<Button size="xs" variant="outline">›</Button></Flex>;
  if (name === 'Steps') return <Flex align="start" width="100%" height="100%">{items.map((item, index) => <Flex key={String(item.key)} flex="1" align="center"><Stack align="center" gap="1"><Box width="8" height="8" borderRadius="full" display="grid" placeItems="center" bg={index <= Number(p.defaultStep ?? 1) ? 'blue.500' : 'gray.200'} color={index <= Number(p.defaultStep ?? 1) ? 'white' : 'gray.600'}>{index + 1}</Box><Text textStyle="xs">{String(item.label)}</Text></Stack>{index < items.length - 1 && <Box flex="1" height="2px" bg={index < Number(p.defaultStep ?? 1) ? 'blue.400' : 'gray.200'} mx="2" mt="4" />}</Flex>)}</Flex>;
  if (name === 'Tabs') return <Tabs.Root {...fill} defaultValue={String(p.defaultValue ?? items[0]?.key ?? '')} variant={p.variant}><Tabs.List>{items.map((item) => <Tabs.Trigger key={String(item.key)} value={String(item.key)}>{String(item.label)}</Tabs.Trigger>)}<Tabs.Indicator /></Tabs.List>{items.map((item) => <Tabs.Content key={String(item.key)} value={String(item.key)} padding="3">{slotContent[`tab-${String(item.key)}`] ?? <Text color="fg.muted">{String(item.label)}内容</Text>}</Tabs.Content>)}</Tabs.Root>;
  if (name === 'Accordion') return <Accordion.Root {...fill} defaultValue={Array.isArray(p.defaultValue) ? p.defaultValue.map(String) : [String(p.defaultValue ?? items[0]?.key ?? '')]} multiple={Boolean(p.multiple)} collapsible={p.collapsible !== false} variant={p.variant}>{items.map((item) => <Accordion.Item key={String(item.key)} value={String(item.key)}><Accordion.ItemTrigger><Text flex="1">{String(item.label)}</Text><Accordion.ItemIndicator /></Accordion.ItemTrigger><Accordion.ItemContent><Accordion.ItemBody>{slotContent[`panel-${String(item.key)}`] ?? <Text color="fg.muted">{String(item.label)}的详细内容。</Text>}</Accordion.ItemBody></Accordion.ItemContent></Accordion.Item>)}</Accordion.Root>;
  if (name === 'Collapsible') return <Box {...fill} borderWidth="1px" borderRadius="lg" overflow="hidden"><Button variant="ghost" width="100%" justifyContent="space-between" onClick={() => preview && setExpanded(!expanded)}>{component.content}<span>{expanded ? '−' : '+'}</span></Button>{expanded && <Box borderTopWidth="1px" padding="3">{slotContent.content ?? <Text color="fg.muted">这里是可以继续设计的折叠内容。</Text>}</Box>}</Box>;

  if (name === 'Avatar') return <Avatar.Root size={p.size ?? 'lg'}><Avatar.Fallback name={String(p.name ?? component.content)} /><Avatar.Image src={p.src} /></Avatar.Root>;
  if (name === 'Badge') return <Badge width="fit-content" colorPalette={p.colorPalette} variant={p.variant}>{component.content}</Badge>;
  if (name === 'Card') return <Card.Root {...fill} variant={p.variant}><Card.Header><Card.Title>{String(p.title ?? '卡片')}</Card.Title></Card.Header><Card.Body>{slotContent.content ?? <Text color="fg.muted">{component.content}</Text>}</Card.Body></Card.Root>;
  if (name === 'Table') return <SimpleDataTable className="chakra-data-table" columns={runtimeStrings(p.columns)} rows={runtimeRows(p.rows)} striped={Boolean(p.striped)} />;
  if (name === 'Stat') return <Stack {...fill} borderWidth="1px" borderRadius="lg" padding="4" gap="1"><Text textStyle="sm" color="fg.muted">{String(p.label)}</Text><Heading size="2xl">{String(p.value)}</Heading><Badge colorPalette="green" width="fit-content">{String(p.change)}</Badge></Stack>;
  if (name === 'Timeline') return <Stack gap="3">{items.map((item, index) => <Flex key={String(item.key)} gap="3"><Stack align="center" gap="0"><Box width="3" height="3" borderRadius="full" bg="blue.500" />{index < items.length - 1 && <Box width="1px" flex="1" minHeight="28px" bg="gray.200" />}</Stack><Box><Text fontWeight="medium">{String(item.label)}</Text><Text textStyle="xs" color="fg.muted">{String(item.description)}</Text></Box></Flex>)}</Stack>;

  if (name === 'Alert') return <Alert.Root status={p.status} variant={p.variant} {...fill}><Alert.Indicator /><Alert.Content><Alert.Title>{String(p.title ?? '提示')}</Alert.Title><Alert.Description>{component.content}</Alert.Description></Alert.Content></Alert.Root>;
  if (name === 'Progress') return <Stack width="100%" gap="2"><Flex justify="space-between"><Text textStyle="sm">完成进度</Text><Text textStyle="sm">{Number(p.value ?? 0)}%</Text></Flex><Progress.Root value={Number(p.value ?? 0)} colorPalette={p.colorPalette} size={p.size} striped={p.striped} animated={p.animated}><Progress.Track><Progress.Range /></Progress.Track></Progress.Root></Stack>;
  if (name === 'Spinner') return <Flex {...fill} align="center" justify="center"><Spinner size={p.size ?? 'xl'} color="blue.500" /></Flex>;
  if (name === 'Skeleton') return <Skeleton loading><SkeletonComposition kind={String(p.kind ?? 'text')} lines={Number(p.lines ?? 3)} className="chakra-skeleton-composition" /></Skeleton>;
  if (name === 'EmptyState') return <Stack {...fill} align="center" justify="center" textAlign="center" gap="2" borderWidth="1px" borderRadius="lg"><Box fontSize="3xl">∅</Box><Heading size="md">{String(p.title)}</Heading><Text color="fg.muted" textStyle="sm">{String(p.description)}</Text><Button size="sm" variant="outline">创建项目</Button></Stack>;

  if (name === 'Dialog' || name === 'Drawer') return <><Button ref={triggerRef} width="100%" height="100%" colorPalette="blue" onClick={() => preview && setOpen(true)}>{component.content}</Button><DesignOverlay anchorRef={triggerRef} open={open} side={name === 'Dialog' ? String(p.placement ?? 'center') : String(p.placement ?? 'right')} title={String(p.title ?? name)} className="chakra-overlay" onClose={() => setOpen(false)} footer={<><Button variant="outline" onClick={() => setOpen(false)}>取消</Button><Button colorPalette="blue" onClick={() => setOpen(false)}>保存</Button></>}>{slotContent.content ?? <Stack gap="4"><Text color="fg.muted">这是 Chakra UI 的真实交互浮层，可以继续放入表单或展示组件。</Text><Input placeholder="输入内容" /></Stack>}</DesignOverlay></>;
  if (name === 'Popover') return <><Button ref={triggerRef} width="100%" height="100%" variant="outline" onClick={() => preview && setOpen(!open)}>{component.content}</Button><FloatingSurface anchorRef={triggerRef} open={open} className="chakra-floating"><Stack gap="3"><Heading size="sm">{String(p.title ?? '气泡内容')}</Heading>{slotContent.popup ?? <Text color="fg.muted" textStyle="sm">这是可以继续设计的 Chakra Popover 内容。</Text>}<Button size="sm" onClick={() => setOpen(false)}>完成</Button></Stack></FloatingSurface></>;
  if (name === 'Tooltip') return <><Button ref={hoverRef} width="100%" height="100%" variant="outline" onMouseEnter={() => preview && setOpen(true)} onMouseLeave={() => setOpen(false)}>{component.content}</Button><FloatingSurface anchorRef={hoverRef} open={open} className="chakra-tooltip-surface">{String(p.content)}</FloatingSurface></>;
  if (name === 'Menu') return <><Button ref={triggerRef} width="100%" height="100%" variant="outline" onClick={() => preview && setOpen(!open)}>{component.content}⌄</Button><FloatingSurface anchorRef={triggerRef} open={open} className="chakra-menu-surface">{items.map((item) => <button key={String(item.key)} onClick={() => setOpen(false)}>{String(item.label)}</button>)}</FloatingSurface></>;

  return <Box {...fill} borderWidth="1px" borderRadius="md" display="grid" placeItems="center"><Badge colorPalette="teal">Chakra UI · {name}</Badge></Box>;
}
