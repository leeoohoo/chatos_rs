// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { Cpu, HardDrive, Settings2, Shield } from 'lucide-react';

import type {
  ConnectorStatus,
  SandboxBackendKind,
  SandboxSettings,
} from '../api';
import { sandboxBackendLabel } from './sandboxPolicyModel';

export function SandboxTechnicalDetails({
  status,
  settings,
  backend,
}: {
  status: ConnectorStatus;
  settings: SandboxSettings | null;
  backend: SandboxBackendKind;
}) {
  return (
    <details className="sandboxTechnicalDetails">
      <summary><Settings2 size={15} />技术信息</summary>
      <div className="sandboxTechnicalGrid">
        <TechnicalItem
          icon={Cpu}
          label="隔离方式"
          value={sandboxBackendLabel(backend)}
          detail={status.sandbox.isolation_note || '由客户端自动选择'}
        />
        <TechnicalItem
          icon={HardDrive}
          label="Docker"
          value={status.docker.installed
            ? (status.docker.running ? '运行中' : '未运行')
            : '未安装'}
          detail={status.docker.version || status.docker.error || '仅 Docker 模式需要'}
        />
        <TechnicalItem
          icon={Shield}
          label="策略版本"
          value={settings?.policy_revision || status.sandbox.policy_revision || '默认'}
        />
      </div>
    </details>
  );
}

function TechnicalItem({
  icon: Icon,
  label,
  value,
  detail,
}: {
  icon: typeof Shield;
  label: string;
  value: string;
  detail?: string;
}) {
  return (
    <div className="sandboxTechnicalItem">
      <Icon size={15} />
      <span>{label}</span>
      <strong>{value}</strong>
      {detail ? <small>{detail}</small> : null}
    </div>
  );
}
