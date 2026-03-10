import { execSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { beforeAll, describe, expect, it } from 'vitest';

import { ApiRequestError } from './client/shared';
import {
  REMOTE_CONNECTION_ERROR_CODE_ACTIONS,
  REMOTE_CONNECTION_ERROR_CODE_MESSAGES,
  REMOTE_SFTP_ERROR_CODE_ACTIONS,
  REMOTE_SFTP_ERROR_CODE_MESSAGES,
  formatRemoteConnectionErrorFeedback,
  resolveRemoteConnectionErrorFeedback,
  resolveRemoteConnectionErrorMessage,
  resolveRemoteSftpErrorFeedback,
  resolveRemoteTerminalWsErrorFeedback,
} from './remoteConnectionErrors';

interface BackendErrorCodes {
  remote_connection_codes: string[];
  remote_sftp_codes: string[];
}

const currentFilePath = fileURLToPath(import.meta.url);
const currentDirPath = path.dirname(currentFilePath);
const backendDirPath = path.resolve(currentDirPath, '../../../../chat_app_server_rs');
const backendCodesDocPath = path.resolve(
  backendDirPath,
  'docs/remote_connection_error_codes.json',
);
let backendErrorCodes: BackendErrorCodes;

describe('remoteConnectionErrors mapping', () => {
  beforeAll(() => {
    execSync('cargo run -q --bin export_remote_connection_error_codes', {
      cwd: backendDirPath,
      stdio: 'pipe',
    });
    backendErrorCodes = JSON.parse(
      readFileSync(backendCodesDocPath, 'utf8'),
    ) as BackendErrorCodes;
  });

  it('maps ApiRequestError code to message and action', () => {
    const error = new ApiRequestError('permission denied', {
      code: 'auth_failed',
      status: 401,
    });
    const feedback = resolveRemoteConnectionErrorFeedback(error, '连接失败');

    expect(feedback.code).toBe('auth_failed');
    expect(feedback.message).toBe('SSH 认证失败');
    expect(feedback.action).toContain('用户名');
  });

  it('formats feedback with action as layered text', () => {
    const formatted = formatRemoteConnectionErrorFeedback({
      code: 'host_key_mismatch',
      message: '主机指纹与 known_hosts 不匹配',
      action: '请核对服务器指纹',
    });

    expect(formatted).toBe(
      '主机指纹与 known_hosts 不匹配；建议：请核对服务器指纹',
    );
  });

  it('falls back to raw message for unknown code', () => {
    const error = new ApiRequestError('custom backend failure', {
      code: 'unknown_backend_code',
      status: 400,
    });

    const feedback = resolveRemoteConnectionErrorFeedback(error, '连接失败');
    expect(feedback.code).toBe('unknown_backend_code');
    expect(feedback.message).toBe('custom backend failure');
    expect(feedback.action).toBeUndefined();
  });

  it('maps ws payload to feedback with action', () => {
    const feedback = resolveRemoteTerminalWsErrorFeedback({
      code: 'network_timeout',
      error: 'connection timed out',
    });

    expect(feedback.code).toBe('network_timeout');
    expect(feedback.message).toBe('网络连接超时');
    expect(feedback.action).toContain('端口');
  });

  it('returns combined message when using resolveRemoteConnectionErrorMessage', () => {
    const error = new ApiRequestError('forbidden', {
      code: 'user_scope_forbidden',
      status: 403,
    });

    const message = resolveRemoteConnectionErrorMessage(error, '请求失败');
    expect(message).toContain('请求用户范围与当前登录用户不一致');
    expect(message).toContain('建议：');
  });

  it('covers critical mapping codes with non-fallback message/action', () => {
    const criticalCodes = [
      'invalid_argument',
      'user_scope_forbidden',
      'host_key_mismatch',
      'host_key_untrusted',
      'host_key_verification_failed',
      'auth_failed',
      'dns_resolve_failed',
      'network_timeout',
      'network_unreachable',
      'remote_connection_create_failed',
      'remote_connection_update_failed',
      'remote_connection_delete_failed',
      'terminal_init_failed',
      'terminal_input_failed',
      'terminal_resize_failed',
    ];

    for (const code of criticalCodes) {
      const fallbackMessage = `fallback-${code}`;
      const feedback = resolveRemoteConnectionErrorFeedback(
        new ApiRequestError('raw-message', { code, status: 400 }),
        fallbackMessage,
      );
      expect(feedback.message).not.toBe('raw-message');
      expect(feedback.message).not.toBe(fallbackMessage);
      expect(feedback.action).toBeTruthy();
    }
  });

  it('keeps frontend mapping aligned with backend exported code catalog', () => {
    const backendConnectionCodes = backendErrorCodes.remote_connection_codes;
    const missingConnectionMessageCodes = backendConnectionCodes.filter(
      (code) => !Object.prototype.hasOwnProperty.call(REMOTE_CONNECTION_ERROR_CODE_MESSAGES, code),
    );
    const missingConnectionActionCodes = backendConnectionCodes.filter(
      (code) => !Object.prototype.hasOwnProperty.call(REMOTE_CONNECTION_ERROR_CODE_ACTIONS, code),
    );

    expect(missingConnectionMessageCodes).toEqual([]);
    expect(missingConnectionActionCodes).toEqual([]);

    const backendSftpCodes = backendErrorCodes.remote_sftp_codes;
    const missingSftpMessageCodes = backendSftpCodes.filter(
      (code) => !Object.prototype.hasOwnProperty.call(REMOTE_SFTP_ERROR_CODE_MESSAGES, code),
    );
    const missingSftpActionCodes = backendSftpCodes.filter(
      (code) => !Object.prototype.hasOwnProperty.call(REMOTE_SFTP_ERROR_CODE_ACTIONS, code),
    );

    expect(missingSftpMessageCodes).toEqual([]);
    expect(missingSftpActionCodes).toEqual([]);
  });

  it('maps sftp error code to message and action', () => {
    const error = new ApiRequestError('remote disconnected', {
      code: 'remote_network_disconnected',
      status: 408,
    });
    const feedback = resolveRemoteSftpErrorFeedback(error, '传输失败');

    expect(feedback.code).toBe('remote_network_disconnected');
    expect(feedback.message).toContain('远端网络连接中断');
    expect(feedback.action).toContain('网络稳定性');
  });
});
