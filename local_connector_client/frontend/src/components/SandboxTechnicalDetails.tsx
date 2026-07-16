// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { Cpu, HardDrive, Network, Settings2, Shield } from 'lucide-react';

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
          label="当前隔离方式"
          value={sandboxBackendLabel(backend)}
          detail={backend === 'local_process'
            ? '由操作系统沙箱限制本机进程；不是线程隔离。'
            : '任务在独立 Docker 容器中运行。'}
        />
        {backend === 'docker' ? (
          <TechnicalItem
            icon={HardDrive}
            label="Docker 环境"
            value={status.docker.installed
              ? (status.docker.running ? '运行中' : '未运行')
              : '未安装'}
            detail={status.docker.version || status.docker.error || 'Docker 模式需要'}
          />
        ) : null}
        <TechnicalItem
          icon={Network}
          label="网络隔离"
          value={status.sandbox.network_isolation === false ? '不可用' : '受策略限制'}
          detail={backend === 'docker'
            ? '受限权限使用无公网出口的 internal 网络；完整访问模式使用 bridge 网络。'
            : '默认断网；需要时进入用户或 AI 审批流程。'}
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
