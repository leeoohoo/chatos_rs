import { createContext, useContext, useMemo, useState, type ReactNode } from 'react';

export type AdminLocale = 'zh-CN' | 'en-US';

type MessageKey = keyof typeof messages['zh-CN'];

type AdminI18nContextValue = {
  locale: AdminLocale;
  setLocale: (locale: AdminLocale) => void;
  t: (key: MessageKey) => string;
};

export const ADMIN_LOCALE_STORAGE_KEY = 'chatos_admin_locale';
const LEGACY_LOCALE_STORAGE_KEYS = ['chat_ui_locale', 'plugin_management_service_locale'] as const;

const messages = {
  'zh-CN': {
    'app.title': 'ChatOS',
    'app.subtitle': '统一管理端',
    'app.header': '平台管理',
    'app.environment': '环境',
    'auth.username': '用户名',
    'auth.usernameRequired': '请输入用户名',
    'auth.password': '密码',
    'auth.passwordRequired': '请输入密码',
    'auth.login': '登录',
    'auth.loginSuccess': '登录成功',
    'auth.loginFailed': '登录失败',
    'auth.noAccess': '当前账号没有管理端访问权限',
    'auth.logout': '退出',
    'auth.logoutSuccess': '已退出登录',
    'login.title': 'ChatOS 统一管理端',
    'login.subtitle': '一次登录，管理全部平台服务',
    'language.label': '语言',
    'language.zh': '中文',
    'language.en': 'English',
    'nav.users': '用户与模型',
    'nav.userModels': '模型配置',
    'nav.userAccounts': '用户账号',
    'nav.agentAccounts': 'Agent 账号',
    'nav.userSettings': '用户设置',
    'nav.projects': '项目管理',
    'nav.projectList': '项目列表',
    'nav.projectConfig': '项目配置',
    'nav.taskRunner': '任务执行',
    'nav.tasks': '任务',
    'nav.runs': '运行记录',
    'nav.prompts': 'Prompt',
    'nav.executionProjects': '执行项目',
    'nav.taskMcp': 'MCP 与工具',
    'nav.tooling': '工具运行时',
    'nav.executionUsers': '执行用户',
    'nav.executionSettings': '执行设置',
    'nav.plugins': '插件与 MCP',
    'nav.mcpCatalog': 'MCP 目录',
    'nav.pluginCatalog': '插件目录',
    'nav.pluginReleases': '插件版本',
    'nav.marketplaces': '市场源',
    'nav.publishers': '发布者',
    'nav.systemAgents': '系统 Agent',
    'nav.runtime': '运行时预览',
    'nav.diagnostics': '诊断',
    'nav.audit': '审计',
    'nav.memory': '记忆引擎',
    'nav.overview': '总览',
    'nav.memoryData': '记忆数据',
    'nav.sources': '来源',
    'nav.policies': '策略',
    'nav.config': '配置与运维',
    'nav.configManagement': '配置管理',
    'nav.releaseHistory': '发布历史',
    'nav.queueOperations': '队列运维',
    'nav.instances': '服务实例',
    'nav.auditLog': '审计日志',
    'error.moduleTitle': '模块加载失败',
    'error.moduleDescription': '当前模块发生异常，其他管理模块不受影响。',
    'error.retry': '重新加载模块',
    'access.deniedTitle': '无权访问此模块',
    'access.deniedDescription': '当前账号不具备此模块要求的超级管理员权限。',
    'access.back': '返回可访问模块',
  },
  'en-US': {
    'app.title': 'ChatOS',
    'app.subtitle': 'Unified Admin',
    'app.header': 'Platform Administration',
    'app.environment': 'Environment',
    'auth.username': 'Username',
    'auth.usernameRequired': 'Enter your username',
    'auth.password': 'Password',
    'auth.passwordRequired': 'Enter your password',
    'auth.login': 'Sign in',
    'auth.loginSuccess': 'Signed in',
    'auth.loginFailed': 'Sign-in failed',
    'auth.noAccess': 'This account cannot access the administration console',
    'auth.logout': 'Sign out',
    'auth.logoutSuccess': 'Signed out',
    'login.title': 'ChatOS Unified Admin',
    'login.subtitle': 'Sign in once to manage every platform service',
    'language.label': 'Language',
    'language.zh': '中文',
    'language.en': 'English',
    'nav.users': 'Users & Models',
    'nav.userModels': 'Model Configuration',
    'nav.userAccounts': 'User Accounts',
    'nav.agentAccounts': 'Agent Accounts',
    'nav.userSettings': 'User Settings',
    'nav.projects': 'Project Management',
    'nav.projectList': 'Projects',
    'nav.projectConfig': 'Project Configuration',
    'nav.taskRunner': 'Task Runner',
    'nav.tasks': 'Tasks',
    'nav.runs': 'Runs',
    'nav.prompts': 'Prompts',
    'nav.executionProjects': 'Execution Projects',
    'nav.taskMcp': 'MCP & Tools',
    'nav.tooling': 'Tool Runtime',
    'nav.executionUsers': 'Execution Users',
    'nav.executionSettings': 'Execution Settings',
    'nav.plugins': 'Plugins & MCP',
    'nav.mcpCatalog': 'MCP Catalog',
    'nav.pluginCatalog': 'Plugin Catalog',
    'nav.pluginReleases': 'Plugin Releases',
    'nav.marketplaces': 'Marketplaces',
    'nav.publishers': 'Publishers',
    'nav.systemAgents': 'System Agents',
    'nav.runtime': 'Runtime Preview',
    'nav.diagnostics': 'Diagnostics',
    'nav.audit': 'Audit',
    'nav.memory': 'Memory Engine',
    'nav.overview': 'Overview',
    'nav.memoryData': 'Memory Data',
    'nav.sources': 'Sources',
    'nav.policies': 'Policies',
    'nav.config': 'Configuration & Operations',
    'nav.configManagement': 'Configuration',
    'nav.releaseHistory': 'Release History',
    'nav.queueOperations': 'Queue Operations',
    'nav.instances': 'Service Instances',
    'nav.auditLog': 'Audit Log',
    'error.moduleTitle': 'Module failed to load',
    'error.moduleDescription': 'This module encountered an error. Other administration modules remain available.',
    'error.retry': 'Reload module',
    'access.deniedTitle': 'Access denied',
    'access.deniedDescription': 'This module requires super administrator access.',
    'access.back': 'Go to an available module',
  },
} as const;

export function normalizeAdminLocale(value: string | null | undefined): AdminLocale | null {
  if (value === 'zh-CN' || value === 'en-US') return value;
  return null;
}

export function readStoredAdminLocale(storage: Pick<Storage, 'getItem' | 'setItem'>): AdminLocale {
  const current = normalizeAdminLocale(storage.getItem(ADMIN_LOCALE_STORAGE_KEY));
  if (current) return current;
  for (const key of LEGACY_LOCALE_STORAGE_KEYS) {
    const legacy = normalizeAdminLocale(storage.getItem(key));
    if (legacy) {
      storage.setItem(ADMIN_LOCALE_STORAGE_KEY, legacy);
      return legacy;
    }
  }
  return 'zh-CN';
}

function persistLocale(locale: AdminLocale) {
  try {
    localStorage.setItem(ADMIN_LOCALE_STORAGE_KEY, locale);
    for (const key of LEGACY_LOCALE_STORAGE_KEYS) localStorage.setItem(key, locale);
  } catch {
    // The in-memory locale remains authoritative when storage is unavailable.
  }
}

const AdminI18nContext = createContext<AdminI18nContextValue | null>(null);

export function AdminI18nProvider({ children }: { children: ReactNode }) {
  const [locale, setLocaleState] = useState<AdminLocale>(() => {
    try {
      return readStoredAdminLocale(localStorage);
    } catch {
      return 'zh-CN';
    }
  });
  const value = useMemo<AdminI18nContextValue>(() => ({
    locale,
    setLocale: (nextLocale) => {
      persistLocale(nextLocale);
      setLocaleState(nextLocale);
    },
    t: (key) => messages[locale][key],
  }), [locale]);
  return <AdminI18nContext.Provider value={value}>{children}</AdminI18nContext.Provider>;
}

export function useAdminI18n(): AdminI18nContextValue {
  const value = useContext(AdminI18nContext);
  if (!value) throw new Error('useAdminI18n must be used inside AdminI18nProvider');
  return value;
}
