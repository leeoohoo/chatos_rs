import type { FC } from 'react';
import type { HostKeyPolicy } from '../helpers';

interface ConnectionBasicsSectionProps {
  remoteName: string;
  remoteHost: string;
  remotePort: string;
  remoteUsername: string;
  remoteHostKeyPolicy: HostKeyPolicy;
  onRemoteNameChange: (value: string) => void;
  onRemoteHostChange: (value: string) => void;
  onRemotePortChange: (value: string) => void;
  onRemoteUsernameChange: (value: string) => void;
  onRemoteHostKeyPolicyChange: (value: HostKeyPolicy) => void;
}

export const ConnectionBasicsSection: FC<ConnectionBasicsSectionProps> = ({
  remoteName,
  remoteHost,
  remotePort,
  remoteUsername,
  remoteHostKeyPolicy,
  onRemoteNameChange,
  onRemoteHostChange,
  onRemotePortChange,
  onRemoteUsernameChange,
  onRemoteHostKeyPolicyChange,
}) => (
  <div className="grid grid-cols-2 gap-3">
    <div className="col-span-2">
      <label className="text-sm text-muted-foreground">名称（可选）</label>
      <input
        value={remoteName}
        onChange={(e) => onRemoteNameChange(e.target.value)}
        className="mt-1 w-full px-3 py-2 rounded border border-border bg-background text-foreground focus:outline-none focus:ring-2 focus:ring-ring"
        placeholder="默认：user@host"
      />
    </div>
    <div>
      <label className="text-sm text-muted-foreground">主机</label>
      <input
        value={remoteHost}
        onChange={(e) => onRemoteHostChange(e.target.value)}
        className="mt-1 w-full px-3 py-2 rounded border border-border bg-background text-foreground focus:outline-none focus:ring-2 focus:ring-ring"
        placeholder="例如 1.2.3.4"
      />
    </div>
    <div>
      <label className="text-sm text-muted-foreground">端口</label>
      <input
        value={remotePort}
        onChange={(e) => onRemotePortChange(e.target.value)}
        className="mt-1 w-full px-3 py-2 rounded border border-border bg-background text-foreground focus:outline-none focus:ring-2 focus:ring-ring"
        placeholder="22"
      />
    </div>
    <div>
      <label className="text-sm text-muted-foreground">用户名</label>
      <input
        value={remoteUsername}
        onChange={(e) => onRemoteUsernameChange(e.target.value)}
        className="mt-1 w-full px-3 py-2 rounded border border-border bg-background text-foreground focus:outline-none focus:ring-2 focus:ring-ring"
        placeholder="root"
      />
    </div>
    <div>
      <label className="text-sm text-muted-foreground">主机校验策略</label>
      <select
        value={remoteHostKeyPolicy}
        onChange={(e) => onRemoteHostKeyPolicyChange(e.target.value as HostKeyPolicy)}
        className="mt-1 w-full px-3 py-2 rounded border border-border bg-background text-foreground focus:outline-none focus:ring-2 focus:ring-ring"
      >
        <option value="strict">strict</option>
        <option value="accept_new">accept_new</option>
      </select>
    </div>
  </div>
);
