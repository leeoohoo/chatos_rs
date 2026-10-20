import {
  keepRemoteCredentials,
  remoteConnectionInputSchema,
  type RemoteAuthenticationType,
  type RemoteConnection,
  type RemoteConnectionInput,
  type RemoteCredentialMutation,
  type RemoteCredentialFileSelection,
  type RemoteHostKeyPolicy,
  type RemoteSecretMutation,
} from "./remoteConnection";

export type RemoteJumpMode = "existing" | "manual";

export interface RemoteConnectionDraft {
  name: string;
  host: string;
  port: string;
  username: string;
  authenticationType: RemoteAuthenticationType;
  password: string;
  privateKeySelection: RemoteCredentialFileSelection | null;
  certificateSelection: RemoteCredentialFileSelection | null;
  defaultRemotePath: string;
  hostKeyPolicy: RemoteHostKeyPolicy;
  jumpEnabled: boolean;
  jumpMode: RemoteJumpMode;
  jumpConnectionId: string;
  jumpHost: string;
  jumpPort: string;
  jumpUsername: string;
  jumpPrivateKeySelection: RemoteCredentialFileSelection | null;
  jumpCertificateSelection: RemoteCredentialFileSelection | null;
  jumpPassword: string;
}

export function initialRemoteConnectionDraft(
  editingConnection: RemoteConnection | null,
  connections: RemoteConnection[],
): RemoteConnectionDraft {
  if (!editingConnection) {
    return {
      name: "",
      host: "",
      port: "22",
      username: "",
      authenticationType: "private_key",
      password: "",
      privateKeySelection: null,
      certificateSelection: null,
      defaultRemotePath: "",
      hostKeyPolicy: "strict",
      jumpEnabled: false,
      jumpMode: connections.length > 0 ? "existing" : "manual",
      jumpConnectionId: "",
      jumpHost: "",
      jumpPort: "22",
      jumpUsername: "",
      jumpPrivateKeySelection: null,
      jumpCertificateSelection: null,
      jumpPassword: "",
    };
  }
  return {
    name: editingConnection.name,
    host: editingConnection.host,
    port: String(editingConnection.port),
    username: editingConnection.username,
    authenticationType: editingConnection.authenticationType,
    password: "",
    privateKeySelection: null,
    certificateSelection: null,
    defaultRemotePath: editingConnection.defaultRemotePath ?? "",
    hostKeyPolicy: editingConnection.hostKeyPolicy,
    jumpEnabled: editingConnection.jumpEnabled,
    jumpMode: editingConnection.jumpConnectionId == null ? "manual" : "existing",
    jumpConnectionId: editingConnection.jumpConnectionId ?? "",
    jumpHost: editingConnection.jumpHost ?? "",
    jumpPort: String(editingConnection.jumpPort ?? 22),
    jumpUsername: editingConnection.jumpUsername ?? "",
    jumpPrivateKeySelection: null,
    jumpCertificateSelection: null,
    jumpPassword: "",
  };
}

export function buildRemoteConnectionInput(
  draft: RemoteConnectionDraft,
  editingConnection: RemoteConnection | null,
  connections: RemoteConnection[],
): RemoteConnectionInput {
  const host = required(draft.host, "请输入远端主机地址。");
  const username = required(draft.username, "请输入登录用户名。");
  const port = parsedPort(draft.port, "端口必须在 1 到 65535 之间。");
  const credentials: RemoteCredentialMutation = keepRemoteCredentials();

  if (draft.authenticationType === "password") {
    credentials.password = secretMutation(draft.password, editingConnection?.hasPassword === true, "请输入登录密码。");
    credentials.privateKeyPath = { action: "clear" };
    credentials.certificatePath = { action: "clear" };
  } else {
    credentials.password = { action: "clear" };
    credentials.privateKeyPath = fileMutation(
      draft.privateKeySelection,
      editingConnection?.hasPrivateKeyPath === true,
      "请选择私钥文件。",
    );
    credentials.certificatePath = draft.authenticationType === "private_key_cert"
      ? fileMutation(
        draft.certificateSelection,
        editingConnection?.hasCertificatePath === true,
        "请选择 SSH 证书文件。",
      )
      : { action: "clear" };
  }

  let jumpConnectionId: string | null = null;
  let jumpHost: string | null = null;
  let jumpPort: number | null = null;
  let jumpUsername: string | null = null;
  if (!draft.jumpEnabled) {
    credentials.jumpPassword = { action: "clear" };
    credentials.jumpPrivateKeyPath = { action: "clear" };
    credentials.jumpCertificatePath = { action: "clear" };
  } else if (draft.jumpMode === "existing") {
    const selected = connections.find((connection) => (
      connection.id === draft.jumpConnectionId && connection.id !== editingConnection?.id
    ));
    if (!selected) throw new Error("请选择一条已有连接作为跳板机。");
    jumpConnectionId = selected.id;
    credentials.jumpPassword = { action: "clear" };
    credentials.jumpPrivateKeyPath = { action: "clear" };
    credentials.jumpCertificatePath = { action: "clear" };
  } else {
    jumpHost = required(draft.jumpHost, "启用跳板机后，需要填写跳板机地址和用户名。");
    jumpUsername = required(draft.jumpUsername, "启用跳板机后，需要填写跳板机地址和用户名。");
    jumpPort = parsedPort(draft.jumpPort, "跳板机端口必须在 1 到 65535 之间。");
    credentials.jumpPassword = optionalSecretMutation(draft.jumpPassword);
    credentials.jumpPrivateKeyPath = optionalFileMutation(
      draft.jumpPrivateKeySelection,
      editingConnection?.hasJumpPrivateKeyPath === true,
    );
    credentials.jumpCertificatePath = optionalFileMutation(
      draft.jumpCertificateSelection,
      editingConnection?.hasJumpCertificatePath === true,
    );
    const hasJumpPrivateKey = credentials.jumpPrivateKeyPath.action !== "clear";
    if (credentials.jumpCertificatePath.action !== "clear" && !hasJumpPrivateKey) {
      throw new Error("跳板机证书必须与跳板机私钥一起使用。");
    }
  }

  return remoteConnectionInputSchema.parse({
    name: optional(draft.name),
    host,
    port,
    username,
    authenticationType: draft.authenticationType,
    defaultRemotePath: optional(draft.defaultRemotePath),
    hostKeyPolicy: draft.hostKeyPolicy,
    jumpEnabled: draft.jumpEnabled,
    jumpConnectionId,
    jumpHost,
    jumpPort,
    jumpUsername,
    credentials,
  });
}

function fileMutation(
  selection: RemoteCredentialFileSelection | null,
  hasSavedValue: boolean,
  missingMessage: string,
) {
  if (selection) return { action: "replace" as const, value: selection.selectionId };
  if (hasSavedValue) return { action: "keep" as const };
  throw new Error(missingMessage);
}

function optionalFileMutation(
  selection: RemoteCredentialFileSelection | null,
  hasSavedValue: boolean,
) {
  if (selection) return { action: "replace" as const, value: selection.selectionId };
  return hasSavedValue ? { action: "keep" as const } : { action: "clear" as const };
}

function secretMutation(value: string, hasSavedValue: boolean, missingMessage: string): RemoteSecretMutation {
  const replacement = optional(value);
  if (replacement) return { action: "replace", value: replacement };
  if (hasSavedValue) return { action: "keep" };
  throw new Error(missingMessage);
}

function optionalSecretMutation(value: string): RemoteSecretMutation {
  const replacement = optional(value);
  return replacement ? { action: "replace", value: replacement } : { action: "clear" };
}

function parsedPort(value: string, message: string) {
  const port = Number(value.trim());
  if (!Number.isInteger(port) || port < 1 || port > 65_535) throw new Error(message);
  return port;
}

function required(value: string, message: string) {
  const normalized = value.trim();
  if (!normalized) throw new Error(message);
  return normalized;
}

function optional(value: string) {
  const normalized = value.trim();
  return normalized || null;
}
