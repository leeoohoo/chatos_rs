import { ApiRequestError } from './client/shared';

export const REMOTE_CONNECTION_ERROR_CODE_MESSAGES: Record<string, string> = {
  invalid_argument: '请求参数不合法',
  user_scope_forbidden: '请求用户范围与当前登录用户不一致',
  remote_connection_not_found: '远端连接不存在',
  remote_connection_forbidden: '无权访问该远端连接',
  remote_connection_access_internal: '远端连接访问失败',
  remote_connection_create_failed: '创建远端连接失败',
  remote_connection_update_failed: '更新远端连接失败',
  remote_connection_fetch_failed: '读取远端连接失败',
  remote_connection_delete_failed: '删除远端连接失败',
  host_key_mismatch: '主机指纹与 known_hosts 不匹配',
  host_key_untrusted: '主机指纹未受信任',
  host_key_verification_failed: '主机指纹校验失败',
  auth_failed: 'SSH 认证失败',
  dns_resolve_failed: '远端地址解析失败',
  network_timeout: '网络连接超时',
  network_unreachable: '网络不可达或连接被拒绝',
  connectivity_test_failed: '远端连通性测试失败',
  terminal_init_failed: '远端终端初始化失败',
  terminal_input_failed: '远端终端输入失败',
  terminal_resize_failed: '远端终端窗口调整失败',
  invalid_ws_message: '终端消息格式不合法',
  remote_terminal_error: '远端终端错误',
};

export const REMOTE_CONNECTION_ERROR_CODE_ACTIONS: Record<string, string> = {
  invalid_argument: '请检查主机、用户名、认证方式、端口范围与必填项。',
  user_scope_forbidden: '请刷新登录态并确认当前账号与请求 user_id 一致。',
  remote_connection_not_found: '请确认连接仍存在且当前账号有访问权限。',
  remote_connection_forbidden: '请切换到拥有该连接权限的账号后重试。',
  remote_connection_access_internal: '请稍后重试；若持续失败请检查后端访问日志。',
  host_key_mismatch:
    '请核对服务器指纹；若已确认服务器更换，可将“主机校验策略”切换为 accept_new 后重试。',
  host_key_untrusted:
    '请先将主机指纹加入 known_hosts，或将“主机校验策略”设为 accept_new 后重试。',
  host_key_verification_failed: '请检查本机 ~/.ssh/known_hosts 内容与服务器指纹配置。',
  auth_failed: '请检查用户名、认证方式、私钥/证书路径或密码是否正确。',
  dns_resolve_failed: '请检查主机名、DNS 配置或跳板机地址配置。',
  network_timeout: '请检查端口、防火墙、安全组与跳板机链路连通性。',
  network_unreachable: '请确认目标机器可达、端口已开放且网络策略允许访问。',
  connectivity_test_failed: '建议先执行“测试连接”并按返回信息逐项排查网络与认证配置。',
  terminal_init_failed: '建议断开后重连；若持续失败，请改为先执行“测试连接”排查网络与认证。',
  terminal_input_failed: '建议重连远端终端后重试输入。',
  terminal_resize_failed: '建议保持终端连接并重试窗口调整。',
  invalid_ws_message: '请升级客户端后重试，或刷新页面重建终端连接。',
  remote_terminal_error: '建议先断开重连；若持续失败请先执行“测试连接”并检查服务端日志。',
  remote_connection_create_failed: '请稍后重试；若持续失败请检查服务端日志。',
  remote_connection_update_failed: '请稍后重试；若持续失败请检查服务端日志。',
  remote_connection_fetch_failed: '请刷新列表后重试；若持续失败请检查服务端日志。',
  remote_connection_delete_failed: '请确认连接未被占用后重试；若持续失败请检查服务端日志。',
};

export const REMOTE_SFTP_ERROR_CODE_MESSAGES: Record<string, string> = {
  bad_request: '请求参数不合法',
  invalid_argument: '请求参数不合法',
  invalid_path: '路径不存在或不可访问',
  invalid_directory_name: '目录名称不合法',
  transfer_not_found: '传输任务不存在',
  transfer_not_active: '传输任务不存在或已结束',
  transfer_cancelled: '传输已取消',
  timeout: '操作超时，请稍后重试',
  local_io_error: '本地文件读写失败',
  remote_auth_failed: '远端认证失败',
  remote_path_not_found: '远端路径不存在',
  remote_permission_denied: '远端权限不足',
  remote_network_disconnected: '远端网络连接中断',
  remote_error: '远端连接或协议错误',
};

export const REMOTE_SFTP_ERROR_CODE_ACTIONS: Record<string, string> = {
  bad_request: '请检查传输方向、路径参数与请求体字段是否完整。',
  invalid_argument: '请检查传输方向、路径参数与请求体字段是否完整。',
  invalid_path: '请确认本地/远端路径存在且当前账号有访问权限。',
  invalid_directory_name: '目录名不能为 . 或 ..，且不能包含路径分隔符。',
  transfer_not_found: '请刷新任务状态后重试，避免使用已过期的传输任务 ID。',
  transfer_not_active: '该任务已结束或不存在，请重新发起新的传输任务。',
  transfer_cancelled: '如果是误取消，请重新发起传输。',
  timeout: '请检查网络连通性和远端负载，必要时稍后重试。',
  local_io_error: '请检查本地磁盘空间、文件权限与目标路径可写性。',
  remote_auth_failed: '请检查远端连接认证信息（用户名、密钥或密码）后重试。',
  remote_path_not_found: '请确认远端路径真实存在，注意大小写与软链接路径。',
  remote_permission_denied: '请确认远端账号对目标路径具备读写权限。',
  remote_network_disconnected: '请检查远端网络稳定性、跳板链路和 SSH 会话状态。',
  remote_error: '建议先重试一次；若持续失败请查看后端 SFTP/SSH 日志。',
};

const DETAIL_REQUIRED_CODES = new Set<string>([
  'remote_connection_access_internal',
  'remote_connection_create_failed',
  'remote_connection_update_failed',
  'remote_connection_fetch_failed',
  'remote_connection_delete_failed',
  'connectivity_test_failed',
  'remote_terminal_error',
]);

const SFTP_DETAIL_REQUIRED_CODES = new Set<string>([
  'timeout',
  'local_io_error',
  'remote_auth_failed',
  'remote_network_disconnected',
  'remote_error',
]);

export interface RemoteConnectionErrorFeedback {
  code: string;
  message: string;
  action?: string;
}

const normalizeRawMessage = (value: unknown, fallback: string): string => {
  if (typeof value === 'string' && value.trim().length > 0) {
    return value.trim();
  }
  if (value instanceof Error && value.message.trim().length > 0) {
    return value.message.trim();
  }
  if (value && typeof value === 'object' && typeof (value as any).message === 'string') {
    const text = (value as any).message.trim();
    if (text.length > 0) {
      return text;
    }
  }
  return fallback;
};

const resolveCodeLabel = (code: string, raw: string): string => {
  const mapped = REMOTE_CONNECTION_ERROR_CODE_MESSAGES[code];
  if (!mapped) {
    return raw;
  }
  if (DETAIL_REQUIRED_CODES.has(code)) {
    return `${mapped}: ${raw}`;
  }
  return mapped;
};

const resolveSftpCodeLabel = (code: string, raw: string): string => {
  const mapped = REMOTE_SFTP_ERROR_CODE_MESSAGES[code];
  if (!mapped) {
    return raw;
  }
  if (SFTP_DETAIL_REQUIRED_CODES.has(code)) {
    return `${mapped}: ${raw}`;
  }
  return mapped;
};

const extractErrorCode = (error: unknown): string => {
  if (error instanceof ApiRequestError) {
    return typeof error.code === 'string' ? error.code : '';
  }
  if (error && typeof error === 'object' && typeof (error as any).code === 'string') {
    return (error as any).code;
  }
  return '';
};

export const resolveRemoteConnectionErrorFeedback = (
  error: unknown,
  fallback: string,
): RemoteConnectionErrorFeedback => {
  const code = extractErrorCode(error);
  const raw = normalizeRawMessage(error, fallback);
  const message = resolveCodeLabel(code, raw);
  const action = REMOTE_CONNECTION_ERROR_CODE_ACTIONS[code];
  if (action) {
    return { code, message, action };
  }
  return { code, message };
};

export const formatRemoteConnectionErrorFeedback = (
  feedback: RemoteConnectionErrorFeedback,
): string => {
  if (feedback.action) {
    return `${feedback.message}；建议：${feedback.action}`;
  }
  return feedback.message;
};

export const resolveRemoteConnectionErrorMessage = (
  error: unknown,
  fallback: string,
): string => {
  const feedback = resolveRemoteConnectionErrorFeedback(error, fallback);
  return formatRemoteConnectionErrorFeedback(feedback);
};

export const resolveRemoteTerminalWsErrorFeedback = (
  payload: any,
  fallback = '远端终端错误',
): RemoteConnectionErrorFeedback => {
  const code = typeof payload?.code === 'string' ? payload.code : '';
  const raw = normalizeRawMessage(payload?.error, fallback);
  const message = resolveCodeLabel(code, raw);
  const action = REMOTE_CONNECTION_ERROR_CODE_ACTIONS[code];
  if (action) {
    return { code, message, action };
  }
  return { code, message };
};

export const resolveRemoteTerminalWsErrorMessage = (
  payload: any,
  fallback = '远端终端错误',
): string => {
  const feedback = resolveRemoteTerminalWsErrorFeedback(payload, fallback);
  return formatRemoteConnectionErrorFeedback(feedback);
};

export const resolveRemoteSftpErrorFeedback = (
  error: unknown,
  fallback: string,
): RemoteConnectionErrorFeedback => {
  const code = extractErrorCode(error);
  const raw = normalizeRawMessage(error, fallback);
  const message = resolveSftpCodeLabel(code, raw);
  const action = REMOTE_SFTP_ERROR_CODE_ACTIONS[code];
  if (action) {
    return { code, message, action };
  }
  return { code, message };
};

export const resolveRemoteSftpErrorMessage = (
  error: unknown,
  fallback: string,
): string => {
  const feedback = resolveRemoteSftpErrorFeedback(error, fallback);
  return formatRemoteConnectionErrorFeedback(feedback);
};
