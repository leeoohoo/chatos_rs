// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { Cpu, Network, Settings2, Shield } from 'lucide-react';

import type {
  SandboxBackendKind,
  SandboxSettings,
} from '../api';
import { sandboxBackendLabel } from './sandboxPolicyModel';

export function SandboxTechnicalDetails({
  settings,
  backend,
}: {
  settings: SandboxSettings | null;
  backend: SandboxBackendKind;
}) {
  return (
    <details className="sandboxTechnicalDetails">
      <summary><Settings2 size={15} />技术信息</summary>
      <div className="sandboxTechnicalGrid">
        <TechnicalItem
          icon={Cpu}
          label="当前运行方式"
          value={sandboxBackendLabel(backend)}
          detail="通过 Local Connector 在授权工作区内运行本机进程。"
        />
        <TechnicalItem
          icon={Network}
          label="网络隔离"
          value={settings?.effective_permissions.network.type === 'unrestricted' ? '宿主机网络' : '受策略限制'}
          detail="可默认断网并按请求审批，也可开启本机受控网络代理。"
        />
        <TechnicalItem
          icon={Shield}
          label="策略版本"
          value={settings?.policy_revision || '默认'}
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
