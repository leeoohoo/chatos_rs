import type { RemoteConnection } from '../../../types';
import type {
  HostKeyPolicy,
  JumpHostMode,
  KeyFilePickerTarget,
  RemoteAuthType,
} from '../helpers';

export interface RemoteConnectionModalProps {
  isOpen: boolean;
  editingRemoteConnection: boolean;
  editingRemoteConnectionId: string | null;
  remoteConnections: RemoteConnection[];
  remoteName: string;
  remoteHost: string;
  remotePort: string;
  remoteUsername: string;
  remoteAuthType: RemoteAuthType;
  remotePassword: string;
  remotePrivateKeyPath: string;
  remoteCertificatePath: string;
  remoteDefaultPath: string;
  remoteHostKeyPolicy: HostKeyPolicy;
  remoteJumpEnabled: boolean;
  remoteJumpMode: JumpHostMode;
  remoteJumpConnectionId: string;
  remoteJumpHost: string;
  remoteJumpPort: string;
  remoteJumpUsername: string;
  remoteJumpPrivateKeyPath: string;
  remoteJumpCertificatePath: string;
  remoteJumpPassword: string;
  remoteError: string | null;
  remoteErrorAction: string | null;
  remoteSuccess: string | null;
  remoteTesting: boolean;
  remoteSaving: boolean;
  remoteVerificationModalOpen: boolean;
  remoteVerificationPrompt: string;
  remoteVerificationCode: string;
  onClose: () => void;
  onRemoteNameChange: (value: string) => void;
  onRemoteHostChange: (value: string) => void;
  onRemotePortChange: (value: string) => void;
  onRemoteUsernameChange: (value: string) => void;
  onRemoteAuthTypeChange: (value: RemoteAuthType) => void;
  onRemotePasswordChange: (value: string) => void;
  onRemotePrivateKeyPathChange: (value: string) => void;
  onRemoteCertificatePathChange: (value: string) => void;
  onRemoteDefaultPathChange: (value: string) => void;
  onRemoteHostKeyPolicyChange: (value: HostKeyPolicy) => void;
  onRemoteJumpEnabledChange: (value: boolean) => void;
  onRemoteJumpModeChange: (value: JumpHostMode) => void;
  onRemoteJumpConnectionIdChange: (value: string) => void;
  onRemoteJumpHostChange: (value: string) => void;
  onRemoteJumpPortChange: (value: string) => void;
  onRemoteJumpUsernameChange: (value: string) => void;
  onRemoteJumpPrivateKeyPathChange: (value: string) => void;
  onRemoteJumpCertificatePathChange: (value: string) => void;
  onRemoteJumpPasswordChange: (value: string) => void;
  onRemoteVerificationCodeChange: (value: string) => void;
  onRemoteVerificationClose: () => void;
  onRemoteVerificationSubmit: () => void;
  onOpenKeyFilePicker: (target: KeyFilePickerTarget) => void;
  onTest: () => void;
  onSave: () => void;
}
