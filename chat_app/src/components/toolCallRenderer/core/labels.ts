import type { ToolFamily } from '../../../lib/tools/catalog';

export const triStateLabel = (value: boolean | null): string => (
  value === null ? 'unknown' : (value ? 'yes' : 'no')
);

export const isMeaningfulBrowserPageUrl = (url: string): boolean => {
  const normalized = url.trim().toLowerCase();
  if (!normalized) {
    return false;
  }

  return ![
    'about:blank',
    'about:srcdoc',
    'about:newtab',
    'data:,',
    'chrome://newtab/',
    'chrome://new-tab-page/',
    'edge://newtab/',
  ].includes(normalized);
};

export const getToolFamilyLabel = (family: ToolFamily): string => {
  switch (family) {
    case 'browser':
      return '浏览器自动化';
    case 'web':
      return '网页研究';
    case 'code':
      return '文件与代码';
    case 'process':
      return '进程控制';
    case 'remote':
      return '远程连接';
    case 'notepad':
      return '笔记工具';
    case 'task':
      return '任务管理';
    case 'ui':
      return '交互确认';
    case 'agent':
      return '智能体构建';
    case 'memory':
      return '记忆读取';
    default:
      return '工具调用';
  }
};

export const getToolFamilyDescription = (family: ToolFamily, displayName: string): string => {
  if (family === 'code') {
    if (displayName === 'list_dir') return '浏览目录结构、文件属性和变更范围';
    if (displayName === 'search_text' || displayName === 'search_files') return '聚合命中位置、内容片段和搜索结果';
    if (displayName === 'read_file' || displayName === 'read_file_raw' || displayName === 'read_file_range') {
      return '读取文件内容、范围信息和摘要';
    }
    if (displayName === 'delete_path') return '删除文件或目录并记录执行结果';
    return '修改工作区文件并回传变更摘要';
  }

  if (family === 'browser') {
    if (displayName === 'browser_research') return '结合当前页观察、搜索结果和提取来源';
    if (displayName === 'browser_inspect') return '观察当前页面状态、元素引用和告警';
    if (displayName === 'browser_console' || displayName === 'browser_console_eval') return '采集控制台信息或执行页面脚本';
    if (displayName === 'browser_vision') return '整理截图、视觉分析和模型元信息';
    if (displayName === 'browser_get_images') return '收集页面图片与资源尺寸信息';
    return '浏览器自动化执行与页面状态采集';
  }

  if (family === 'web') {
    if (displayName === 'web_research') return '搜索、筛选链接并整理研究结论';
    if (displayName === 'web_extract') return '提取网页正文、来源摘要和省略信息';
    return '网页搜索与内容提取结果';
  }

  if (family === 'process') {
    if (displayName === 'execute_command') return '展示命令执行状态与终端输出';
    if (displayName === 'get_recent_logs') return '展示最近终端日志与终端分组';
    if (displayName === 'process_log' || displayName === 'process_poll') return '展示进程日志窗口与运行状态';
    if (displayName === 'process_wait') return '展示等待结果、超时状态与输出';
    return '展示终端、进程状态和等待结果';
  }

  if (family === 'remote') {
    if (displayName === 'list_connections') return '展示可用 SSH 连接、目标主机和默认路径';
    if (displayName === 'test_connection') return '展示远程连通性结果与远端主机标识';
    if (displayName === 'run_command') return '展示远程 SSH 命令输出与执行状态';
    if (displayName === 'list_directory') return '展示远程目录条目与目录状态';
    if (displayName === 'read_file') return '展示远程文件内容与截断状态';
    return '展示远程连接与远程主机操作结果';
  }

  if (family === 'notepad') {
    if (displayName === 'init') return '展示笔记空间初始化状态与当前笔记数量';
    if (displayName === 'read_note') return '展示笔记元信息和正文内容';
    if (displayName === 'search_notes') return '展示命中的笔记列表与检索结果';
    if (displayName === 'list_tags') return '展示标签及其使用次数';
    return '展示文件夹、笔记、标签与检索结果';
  }

  if (family === 'task') {
    if (displayName === 'add_task') return '展示待确认的任务创建结果与任务清单';
    if (displayName === 'list_tasks') return '展示当前会话任务列表和范围';
    if (displayName === 'update_task' || displayName === 'complete_task') return '展示任务状态更新后的结果';
    if (displayName === 'delete_task') return '展示任务删除结果';
    return '展示任务确认、任务列表与状态变更';
  }

  if (family === 'ui') {
    if (displayName === 'prompt_choices') return '展示用户选择结果与状态';
    if (displayName === 'prompt_mixed_form') return '展示混合表单填写结果与选择内容';
    if (displayName === 'prompt_key_values') return '展示键值表单填写结果';
    return '展示用户确认结果、表单填写与选择结果';
  }

  if (family === 'agent') {
    if (displayName === 'recommend_agent_profile') return '展示推荐的智能体定位、描述和角色设定';
    if (displayName === 'list_available_skills') return '展示可用技能清单与来源';
    if (displayName === 'create_memory_agent' || displayName === 'update_memory_agent') {
      return '展示 Memory Agent 配置结果、技能和插件来源';
    }
    if (displayName === 'preview_agent_context') return '展示最终注入的角色上下文预览';
    return '展示智能体建议、技能列表与 Memory Agent 配置结果';
  }

  if (family === 'memory') {
    if (displayName === 'get_command_detail') return '展示命令说明、参数提示与完整内容';
    if (displayName === 'get_plugin_detail') return '展示插件信息、命令清单与关联技能';
    if (displayName === 'get_skill_detail') return '展示技能来源、说明和完整内容';
    return '展示命令、插件和技能详情内容';
  }

  return '展示工具输入、输出和运行状态';
};
