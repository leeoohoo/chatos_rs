// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { getUserVisiblePath } from '../../lib/domain/filesystem';
import { resolveToolFamily } from '../../lib/tools/catalog';
import { getToolDisplayName } from '../../lib/tools/displayName';
import type { TimelineStatus } from './ConversationProcessTimelineModel';

export type ToolActionKind =
  | 'read'
  | 'search'
  | 'execute'
  | 'modify'
  | 'browse'
  | 'task'
  | 'generic';

export interface ToolActionSummary {
  completed: string;
  failed: string;
  kind: ToolActionKind;
  pending: string;
}

const readRecord = (value: unknown): Record<string, unknown> | null => (
  value && typeof value === 'object' && !Array.isArray(value)
    ? value as Record<string, unknown>
    : null
);

const parseArguments = (value: unknown): Record<string, unknown> => {
  const record = readRecord(value);
  if (record) {
    return record;
  }
  if (typeof value !== 'string' || !value.trim()) {
    return {};
  }
  try {
    return readRecord(JSON.parse(value)) || {};
  } catch {
    return { input: value.trim() };
  }
};

const stringValue = (record: Record<string, unknown>, keys: string[]): string => {
  for (const key of keys) {
    const value = record[key];
    if (typeof value === 'string' && value.trim()) {
      return value.trim();
    }
  }
  return '';
};

const stringList = (record: Record<string, unknown>, keys: string[]): string[] => {
  for (const key of keys) {
    const value = record[key];
    if (!Array.isArray(value)) {
      continue;
    }
    const items = value
      .map((item) => (typeof item === 'string' ? item.trim() : ''))
      .filter(Boolean);
    if (items.length > 0) {
      return items;
    }
  }
  return [];
};

const clipped = (value: string, limit = 160): string => {
  const normalized = value.replace(/\s+/g, ' ').trim();
  return normalized.length > limit ? `${normalized.slice(0, limit - 1)}…` : normalized;
};

const visiblePath = (value: string): string => clipped(getUserVisiblePath(value) || value || '项目目录');

const quoted = (value: string): string => `「${clipped(value, 120)}」`;

const action = (
  kind: ToolActionKind,
  completed: string,
  pending: string,
  failed: string,
): ToolActionSummary => ({ kind, completed, pending, failed });

const stagedOperationPaths = (record: Record<string, unknown>): string[] => {
  const operations = Array.isArray(record.operations) ? record.operations : [];
  const paths = operations
    .map((operation) => readRecord(operation))
    .filter((operation): operation is Record<string, unknown> => operation !== null)
    .map((operation) => stringValue(operation, ['path']))
    .filter(Boolean)
    .map(visiblePath);
  return Array.from(new Set(paths));
};

const changeTitle = (record: Record<string, unknown>): string => {
  const direct = stringValue(record, ['title', 'task_title', 'taskTitle', 'name']);
  if (direct) {
    return direct;
  }
  const changes = stringValue(record, ['changes']);
  if (!changes) {
    return '';
  }
  try {
    return stringValue(readRecord(JSON.parse(changes)) || {}, ['title', 'name']);
  } catch {
    return '';
  }
};

export const buildToolActionSummary = (
  rawToolName: string,
  argumentsValue: unknown,
): ToolActionSummary => {
  const displayName = getToolDisplayName(rawToolName);
  const family = resolveToolFamily(rawToolName, displayName);
  const args = parseArguments(argumentsValue);
  const path = visiblePath(stringValue(args, [
    'path', 'file', 'file_path', 'filePath', 'directory', 'dir', 'root', 'cwd', 'workdir',
  ]));
  const query = stringValue(args, ['query', 'pattern', 'search', 'keyword', 'text']);
  const command = stringValue(args, ['command', 'cmd', 'script']);
  const url = stringValue(args, ['url']);
  const urls = stringList(args, ['urls']);
  const title = changeTitle(args);
  const connection = stringValue(args, [
    'connection', 'connection_id', 'connectionId', 'connection_name', 'connectionName',
  ]);

  if (displayName === 'read_file' && family === 'remote') {
    return action('read', `已读取远端文件 ${path}`, `正在读取远端文件 ${path}`, `读取远端文件 ${path} 失败`);
  }
  if (displayName === 'list_directory' && family === 'remote') {
    return action('read', `已读取远端目录 ${path}`, `正在读取远端目录 ${path}`, `读取远端目录 ${path} 失败`);
  }
  if (['read_file_raw', 'read_file_range', 'read_file'].includes(displayName)) {
    return action('read', `已读取 ${path}`, `正在读取 ${path}`, `读取 ${path} 失败`);
  }
  if (displayName === 'list_dir' || displayName === 'list_directory') {
    return action('read', `已读取 ${path} 目录`, `正在读取 ${path} 目录`, `读取 ${path} 目录失败`);
  }
  if (['search_text', 'search_files'].includes(displayName)) {
    const scope = path === '项目目录' ? '项目目录' : path;
    if (query) {
      const target = `${scope} 中搜索${quoted(query)}`;
      return action('search', `已在 ${target}`, `正在 ${target}`, `在 ${target} 失败`);
    }
    return action('search', `已搜索 ${scope}`, `正在搜索 ${scope}`, `搜索 ${scope} 失败`);
  }
  if (displayName === 'execute_command' || displayName === 'process') {
    const target = clipped(command || stringValue(args, ['input']) || '命令');
    return action('execute', `已执行 ${target}`, `正在执行 ${target}`, `执行 ${target} 失败`);
  }
  if (['process_poll', 'process_wait', 'process_log', 'get_recent_logs'].includes(displayName)) {
    const processId = stringValue(args, ['terminal_id', 'terminalId', 'process_id', 'processId', 'id']);
    const target = processId ? `进程 ${clipped(processId, 80)}` : '进程状态';
    return action('execute', `已查看 ${target}`, `正在查看 ${target}`, `查看 ${target} 失败`);
  }
  if (displayName === 'process_list') {
    return action('execute', '已查看进程列表', '正在查看进程列表', '查看进程列表失败');
  }
  if (displayName === 'process_kill') {
    const processId = stringValue(args, ['terminal_id', 'terminalId', 'process_id', 'processId', 'id']);
    const target = processId ? `进程 ${clipped(processId, 80)}` : '进程';
    return action('execute', `已停止 ${target}`, `正在停止 ${target}`, `停止 ${target} 失败`);
  }
  if (displayName === 'process_write') {
    const processId = stringValue(args, ['terminal_id', 'terminalId', 'process_id', 'processId', 'id']);
    const target = processId ? `进程 ${clipped(processId, 80)}` : '进程';
    return action('execute', `已向 ${target} 输入内容`, `正在向 ${target} 输入内容`, `向 ${target} 输入失败`);
  }

  if (displayName === 'stage_edit_batch') {
    const paths = stagedOperationPaths(args);
    const target = paths.length === 1
      ? paths[0]
      : paths.length > 1
        ? `${paths.length} 个文件`
        : '项目文件';
    return action('modify', `已暂存修改 ${target}`, `正在暂存修改 ${target}`, `暂存修改 ${target} 失败`);
  }
  if (displayName === 'commit_edit_session') {
    return action('modify', '已提交项目修改', '正在提交项目修改', '提交项目修改失败');
  }
  if (displayName === 'open_edit_session') {
    return action('modify', '已开始项目修改', '正在准备项目修改', '准备项目修改失败');
  }
  if (displayName === 'abort_edit_session') {
    return action('modify', '已取消项目修改', '正在取消项目修改', '取消项目修改失败');
  }
  if (displayName === 'write_file' && family === 'remote') {
    return action('modify', `已修改 ${path}`, `正在修改 ${path}`, `修改 ${path} 失败`);
  }
  if (displayName === 'browser_navigate') {
    const target = clipped(url || '网页');
    return action('browse', `已打开 ${target}`, `正在打开 ${target}`, `打开 ${target} 失败`);
  }
  if (displayName === 'browser_tabs') {
    return action('read', '已读取浏览器标签页', '正在读取浏览器标签页', '读取浏览器标签页失败');
  }
  if (displayName === 'chrome_status') {
    return action('read', '已检查 Chrome 连接状态', '正在检查 Chrome 连接状态', '检查 Chrome 连接状态失败');
  }
  if (displayName === 'chrome_tabs') {
    return action('read', '已读取授权的 Chrome 标签页', '正在读取授权的 Chrome 标签页', '读取 Chrome 标签页失败');
  }
  if (displayName === 'chrome_tab_snapshot') {
    const target = stringValue(args, ['tab_id', 'tabId']) || 'Chrome 标签页';
    return action('read', `已读取 ${clipped(target, 80)}`, `正在读取 ${clipped(target, 80)}`, `读取 ${clipped(target, 80)} 失败`);
  }
  if (displayName === 'chrome_tab_navigate') {
    const target = clipped(url || 'Chrome 页面');
    return action('browse', `已打开 ${target}`, `正在打开 ${target}`, `打开 ${target} 失败`);
  }
  if (displayName === 'chrome_tab_click') {
    const target = stringValue(args, ['target_id', 'targetId']) || 'Chrome 页面元素';
    return action('browse', `已点击 ${clipped(target, 80)}`, `正在点击 ${clipped(target, 80)}`, `点击 ${clipped(target, 80)} 失败`);
  }
  if (displayName === 'chrome_tab_type_text') {
    const target = stringValue(args, ['target_id', 'targetId']) || 'Chrome 输入框';
    return action('browse', `已在 ${clipped(target, 80)} 输入内容`, `正在向 ${clipped(target, 80)} 输入内容`, `向 ${clipped(target, 80)} 输入失败`);
  }
  if (displayName === 'chrome_tab_select') {
    const option = stringValue(args, ['option_label', 'optionLabel']) || '选项';
    return action('browse', `已选择 ${clipped(option, 100)}`, `正在选择 ${clipped(option, 100)}`, `选择 ${clipped(option, 100)} 失败`);
  }
  if (displayName === 'chrome_tab_scroll') {
    const target = stringValue(args, ['tab_id', 'tabId']) || 'Chrome 标签页';
    return action('browse', `已滚动 ${clipped(target, 80)}`, `正在滚动 ${clipped(target, 80)}`, `滚动 ${clipped(target, 80)} 失败`);
  }
  if (displayName === 'chrome_tab_history') {
    const direction = stringValue(args, ['direction']) === 'forward' ? '前进' : '后退';
    return action('browse', `已${direction}`, `正在${direction}`, `${direction}失败`);
  }
  if (displayName === 'chrome_tab_activate') {
    const target = stringValue(args, ['tab_id', 'tabId']) || 'Chrome 标签页';
    return action('browse', `已激活 ${clipped(target, 80)}`, `正在激活 ${clipped(target, 80)}`, `激活 ${clipped(target, 80)} 失败`);
  }
  if (displayName === 'chrome_tab_upload') {
    const target = stringValue(args, ['path']) || '工作区文件';
    return action('browse', `已上传 ${clipped(target, 100)}`, `正在上传 ${clipped(target, 100)}`, `上传 ${clipped(target, 100)} 失败`);
  }
  if (displayName === 'chrome_tab_download') {
    const target = stringValue(args, ['path']) || '工作区文件';
    return action('browse', `已下载到 ${clipped(target, 100)}`, `正在下载到 ${clipped(target, 100)}`, `下载到 ${clipped(target, 100)} 失败`);
  }
  if (displayName === 'chrome_tab_screenshot') {
    const target = stringValue(args, ['tab_id', 'tabId']) || 'Chrome 标签页';
    return action('read', `已截取 ${clipped(target, 80)}`, `正在截取 ${clipped(target, 80)}`, `截取 ${clipped(target, 80)} 失败`);
  }
  if (displayName === 'chrome_tab_release') {
    const target = stringValue(args, ['tab_id', 'tabId']) || 'Chrome 标签页';
    return action('browse', `已断开 ${clipped(target, 80)}`, `正在断开 ${clipped(target, 80)}`, `断开 ${clipped(target, 80)} 失败`);
  }
  if (displayName === 'browser_tab_new') {
    return action('browse', '已新建浏览器标签页', '正在新建浏览器标签页', '新建浏览器标签页失败');
  }
  if (displayName === 'browser_tab_switch') {
    const target = stringValue(args, ['tab_id', 'tabId']) || '标签页';
    return action('browse', `已切换到 ${clipped(target, 80)}`, `正在切换到 ${clipped(target, 80)}`, `切换到 ${clipped(target, 80)} 失败`);
  }
  if (displayName === 'browser_tab_close') {
    const target = stringValue(args, ['tab_id', 'tabId']) || '标签页';
    return action('browse', `已关闭 ${clipped(target, 80)}`, `正在关闭 ${clipped(target, 80)}`, `关闭 ${clipped(target, 80)} 失败`);
  }
  if (displayName === 'browser_click') {
    const target = stringValue(args, ['text', 'label', 'ref', 'selector']) || '页面元素';
    return action('browse', `已点击 ${clipped(target)}`, `正在点击 ${clipped(target)}`, `点击 ${clipped(target)} 失败`);
  }
  if (displayName === 'browser_type') {
    const target = stringValue(args, ['label', 'ref', 'selector']) || '输入框';
    return action('browse', `已在 ${clipped(target)} 输入内容`, `正在输入内容`, `输入内容失败`);
  }
  if (displayName === 'browser_scroll') {
    return action('browse', '已滚动页面', '正在滚动页面', '滚动页面失败');
  }
  if (displayName === 'browser_back') {
    return action('browse', '已返回上一页', '正在返回上一页', '返回上一页失败');
  }
  if (displayName === 'browser_press') {
    const key = stringValue(args, ['key']) || '按键';
    return action('browse', `已按下 ${clipped(key, 80)}`, `正在按下 ${clipped(key, 80)}`, `按下 ${clipped(key, 80)} 失败`);
  }
  if (displayName === 'browser_upload') {
    const target = stringValue(args, ['ref', 'selector']) || '文件元素';
    return action('browse', `已向 ${clipped(target, 80)} 上传文件`, '正在上传文件', '上传文件失败');
  }
  if (displayName === 'browser_download') {
    const target = path || '工作区文件';
    return action('browse', `已下载 ${clipped(target)}`, `正在下载 ${clipped(target)}`, `下载 ${clipped(target)} 失败`);
  }
  if (displayName === 'browser_console') {
    return action('read', '已读取浏览器控制台', '正在读取浏览器控制台', '读取浏览器控制台失败');
  }
  if (displayName === 'browser_network') {
    return action('read', '已读取浏览器网络记录', '正在读取浏览器网络记录', '读取浏览器网络记录失败');
  }
  if (displayName === 'browser_network_request') {
    return action('read', '已读取网络请求详情', '正在读取网络请求详情', '读取网络请求详情失败');
  }
  if (displayName === 'browser_har_start') {
    return action('read', '已开始 HAR 网络记录', '正在开始 HAR 网络记录', '开始 HAR 网络记录失败');
  }
  if (displayName === 'browser_har_stop') {
    const target = path || '工作区 HAR 文件';
    return action('modify', `已导出脱敏 HAR ${clipped(target)}`, `正在导出脱敏 HAR ${clipped(target)}`, `导出脱敏 HAR ${clipped(target)} 失败`);
  }
  if (displayName === 'browser_websocket_start') {
    return action('read', '已开始观察 WebSocket 帧', '正在开始观察 WebSocket 帧', '开始观察 WebSocket 帧失败');
  }
  if (displayName === 'browser_websocket_frames') {
    return action('read', '已读取 WebSocket 帧', '正在读取 WebSocket 帧', '读取 WebSocket 帧失败');
  }
  if (displayName === 'browser_websocket_stop') {
    return action('read', '已停止观察 WebSocket 帧', '正在停止观察 WebSocket 帧', '停止观察 WebSocket 帧失败');
  }
  if (displayName === 'browser_route_add') {
    const target = stringValue(args, ['pattern']) || 'URL 模式';
    return action('modify', `已添加浏览器拦截规则 ${clipped(target)}`, `正在申请并添加浏览器拦截规则 ${clipped(target)}`, `添加浏览器拦截规则 ${clipped(target)} 失败`);
  }
  if (displayName === 'browser_route_list') {
    return action('read', '已读取浏览器拦截规则', '正在读取浏览器拦截规则', '读取浏览器拦截规则失败');
  }
  if (displayName === 'browser_route_remove') {
    const target = stringValue(args, ['route_id', 'routeId']) || '拦截规则';
    return action('modify', `已移除 ${clipped(target, 80)}`, `正在移除 ${clipped(target, 80)}`, `移除 ${clipped(target, 80)} 失败`);
  }
  if (displayName === 'browser_route_clear') {
    return action('modify', '已清除浏览器拦截规则', '正在清除浏览器拦截规则', '清除浏览器拦截规则失败');
  }
  if (displayName === 'browser_cdp_command') {
    const method = stringValue(args, ['method']) || 'CDP 命令';
    return action('execute', `已执行高风险 CDP 命令 ${clipped(method, 100)}`, `正在等待审批并执行 CDP 命令 ${clipped(method, 100)}`, `执行 CDP 命令 ${clipped(method, 100)} 失败`);
  }
  if (displayName === 'browser_get_images') {
    return action('read', '已读取页面图片', '正在读取页面图片', '读取页面图片失败');
  }
  if (displayName === 'browser_research') {
    const target = query || stringValue(args, ['topic']) || '当前页面';
    return action('search', `已调研${quoted(target)}`, `正在调研${quoted(target)}`, `调研${quoted(target)}失败`);
  }
  if (displayName.startsWith('browser_')) {
    return action('browse', '已查看当前页面', '正在查看当前页面', '查看当前页面失败');
  }
  if (displayName === 'web_search') {
    const target = query || stringValue(args, ['input']) || '网页内容';
    return action('search', `已搜索网页${quoted(target)}`, `正在搜索网页${quoted(target)}`, `搜索网页${quoted(target)}失败`);
  }
  if (displayName === 'web_extract') {
    const target = urls.length > 1 ? `${urls.length} 个网页` : clipped(urls[0] || url || '网页');
    return action('read', `已读取 ${target}`, `正在读取 ${target}`, `读取 ${target} 失败`);
  }
  if (displayName === 'web_research') {
    const target = query || '网页主题';
    return action('search', `已调研${quoted(target)}`, `正在调研${quoted(target)}`, `调研${quoted(target)}失败`);
  }

  if (displayName === 'run_command' && family === 'remote') {
    const target = clipped(command || '命令');
    const scope = connection ? `在 ${clipped(connection, 80)} ` : '';
    return action('execute', `已${scope}执行 ${target}`, `正在${scope}执行 ${target}`, `${scope}执行 ${target} 失败`);
  }
  if (displayName === 'list_connections' && family === 'remote') {
    return action('read', '已读取远端连接列表', '正在读取远端连接列表', '读取远端连接列表失败');
  }
  if (displayName === 'test_connection' && family === 'remote') {
    const target = connection ? ` ${clipped(connection, 80)}` : '';
    return action('execute', `已测试远端连接${target}`, `正在测试远端连接${target}`, `测试远端连接${target}失败`);
  }
  if (displayName === 'add_task') {
    const target = title ? quoted(title) : '新任务';
    return action('task', `已创建任务${target}`, `正在创建任务${target}`, `创建任务${target}失败`);
  }
  if (displayName === 'complete_task') {
    const target = title ? quoted(title) : '任务';
    return action('task', `已完成任务${target}`, `正在完成任务${target}`, `完成任务${target}失败`);
  }
  if (displayName === 'update_task') {
    const target = title ? quoted(title) : '任务';
    return action('task', `已更新任务${target}`, `正在更新任务${target}`, `更新任务${target}失败`);
  }
  if (displayName === 'delete_task') {
    return action('task', '已删除任务', '正在删除任务', '删除任务失败');
  }
  if (displayName === 'list_tasks') {
    return action('task', '已读取任务列表', '正在读取任务列表', '读取任务列表失败');
  }

  if (displayName === 'init' && family === 'notepad') {
    return action('modify', '已初始化记事本', '正在初始化记事本', '初始化记事本失败');
  }
  if (displayName === 'list_folders' && family === 'notepad') {
    return action('read', '已读取笔记文件夹', '正在读取笔记文件夹', '读取笔记文件夹失败');
  }
  if (displayName === 'create_folder' && family === 'notepad') {
    const target = title ? quoted(title) : '文件夹';
    return action('modify', `已创建笔记文件夹${target}`, `正在创建笔记文件夹${target}`, `创建笔记文件夹${target}失败`);
  }
  if (displayName === 'rename_folder' && family === 'notepad') {
    const target = title ? quoted(title) : '文件夹';
    return action('modify', `已重命名笔记文件夹${target}`, `正在重命名笔记文件夹${target}`, `重命名笔记文件夹${target}失败`);
  }
  if (displayName === 'delete_folder' && family === 'notepad') {
    const target = title ? quoted(title) : '文件夹';
    return action('modify', `已删除笔记文件夹${target}`, `正在删除笔记文件夹${target}`, `删除笔记文件夹${target}失败`);
  }
  if (displayName === 'list_notes' && family === 'notepad') {
    return action('read', '已读取笔记列表', '正在读取笔记列表', '读取笔记列表失败');
  }
  if (displayName === 'list_tags' && family === 'notepad') {
    return action('read', '已读取笔记标签', '正在读取笔记标签', '读取笔记标签失败');
  }
  if (displayName === 'read_note') {
    const target = title ? quoted(title) : '笔记';
    return action('read', `已读取笔记${target}`, `正在读取笔记${target}`, `读取笔记${target}失败`);
  }
  if (displayName === 'search_notes') {
    const target = query || '笔记内容';
    return action('search', `已搜索笔记${quoted(target)}`, `正在搜索笔记${quoted(target)}`, `搜索笔记${quoted(target)}失败`);
  }
  if (['create_note', 'update_note'].includes(displayName)) {
    const target = title ? quoted(title) : '笔记';
    return action('modify', `已保存笔记${target}`, `正在保存笔记${target}`, `保存笔记${target}失败`);
  }
  if (displayName === 'delete_note') {
    return action('modify', '已删除笔记', '正在删除笔记', '删除笔记失败');
  }

  if (displayName === 'recommend_agent_profile' && family === 'agent') {
    return action('task', '已生成智能体配置建议', '正在生成智能体配置建议', '生成智能体配置建议失败');
  }
  if (displayName === 'create_memory_agent' && family === 'agent') {
    const target = title ? quoted(title) : '';
    return action('modify', `已创建记忆智能体${target}`, `正在创建记忆智能体${target}`, `创建记忆智能体${target}失败`);
  }
  if (displayName === 'update_memory_agent' && family === 'agent') {
    const target = title ? quoted(title) : '';
    return action('modify', `已更新记忆智能体${target}`, `正在更新记忆智能体${target}`, `更新记忆智能体${target}失败`);
  }
  if (displayName === 'preview_agent_context' && family === 'agent') {
    return action('read', '已预览智能体上下文', '正在预览智能体上下文', '预览智能体上下文失败');
  }
  if (displayName.startsWith('get_') && family === 'memory') {
    const target = stringValue(args, ['skill_ref', 'command_ref', 'plugin_ref', 'id', 'name']) || '资料';
    return action('read', `已读取 ${clipped(target)}`, `正在读取 ${clipped(target)}`, `读取 ${clipped(target)} 失败`);
  }

  if (command) {
    const target = clipped(command);
    return action('execute', `已执行 ${target}`, `正在执行 ${target}`, `执行 ${target} 失败`);
  }
  if (query) {
    return action('search', `已搜索${quoted(query)}`, `正在搜索${quoted(query)}`, `搜索${quoted(query)}失败`);
  }
  if (stringValue(args, ['path', 'file', 'directory', 'dir'])) {
    return action('read', `已读取 ${path}`, `正在读取 ${path}`, `读取 ${path} 失败`);
  }
  if (url) {
    const target = clipped(url);
    return action('browse', `已打开 ${target}`, `正在打开 ${target}`, `打开 ${target} 失败`);
  }
  return action('generic', '已完成一项操作', '正在执行一项操作', '操作失败');
};

export const toolActionText = (
  summary: ToolActionSummary,
  status: TimelineStatus,
): string => {
  if (status === 'error') {
    return summary.failed;
  }
  if (status === 'pending') {
    return summary.pending;
  }
  return summary.completed;
};
