// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type {
  MessageTaskRunnerRunEvent,
  PluginArtifactCreateRequest,
  PluginArtifactDescriptor,
  PluginArtifactListResponse,
  PluginArtifactReadResponse,
  PluginArtifactUpdateRequest,
  PluginArtifactWriteOperation,
  PluginArtifactWriteResponse,
  PluginUiWorkbenchSessionResponse,
} from '../../lib/api/client/types';

const BRIDGE_PROTOCOL_VERSION = 1;
const BRIDGE_PAYLOAD_MAX_BYTES = 256 * 1024;
const BRIDGE_REQUEST_ID_MAX_BYTES = 128;
const ARTIFACT_WRITE_MAX_BYTES = 160 * 1024;
const READY_MESSAGE_TYPE = 'chatos.plugin_ui.ready';
const REQUEST_MESSAGE_TYPE = 'chatos.plugin_ui.request';
const RESPONSE_MESSAGE_TYPE = 'chatos.plugin_ui.response';
const ALLOWED_BRIDGE_METHODS = new Set([
  'host.context.read',
  'artifact.list',
  'artifact.read',
  'artifact.download',
  'artifact.create',
  'artifact.update',
]);

type UnknownRecord = Record<string, unknown>;

export interface PluginUiReadySummary {
  eventId: string;
  runId: string;
  pluginId: string;
  releaseId: string;
  artifactSha256: string;
  componentKey: string;
  adapterSessionId: string;
  snapshotSha256: string;
  title: string;
  surface: string;
  bridgeProtocolVersion: number;
  bridgeCapabilities: string[];
  artifactMimeTypes: string[];
}

export type PluginUiBridgeIncoming = {
  kind: 'ready';
  adapterSessionId: string;
  hostSessionNonce: string;
} | {
  kind: 'request';
  adapterSessionId: string;
  hostSessionNonce: string;
  requestId: string;
  method: string;
  payload: UnknownRecord;
};

export interface PluginUiBridgeResponseMessage {
  type: typeof RESPONSE_MESSAGE_TYPE;
  protocol_version: number;
  adapter_session_id: string;
  host_session_nonce: string;
  request_id: string;
  ok: boolean;
  result: unknown;
  error_code: string | null;
}

const record = (value: unknown): UnknownRecord | null => (
  value && typeof value === 'object' && !Array.isArray(value)
    ? value as UnknownRecord
    : null
);

const exactKeys = (value: UnknownRecord, keys: string[]): boolean => {
  const actual = Object.keys(value).sort();
  const expected = [...keys].sort();
  return actual.length === expected.length
    && actual.every((key, index) => key === expected[index]);
};

const boundedText = (value: unknown, limit = 256): string | null => {
  if (typeof value !== 'string' || value.length === 0 || value.length > limit) {
    return null;
  }
  return value;
};

const lowerSha256 = (value: unknown): string | null => (
  typeof value === 'string' && /^[a-f0-9]{64}$/u.test(value) ? value : null
);

const pluginArtifactId = (value: unknown): string | null => (
  typeof value === 'string' && /^pa_[a-f0-9]{32}$/u.test(value) ? value : null
);

const canonicalBase64ByteLength = (value: unknown): number | null => {
  if (
    typeof value !== 'string'
    || value.length > Math.ceil(ARTIFACT_WRITE_MAX_BYTES / 3) * 4
    || value.length % 4 !== 0
    || !/^(?:[A-Za-z0-9+/]{4})*(?:[A-Za-z0-9+/]{2}==|[A-Za-z0-9+/]{3}=)?$/u.test(value)
  ) {
    return null;
  }
  const padding = value.endsWith('==') ? 2 : value.endsWith('=') ? 1 : 0;
  const bytes = (value.length / 4) * 3 - padding;
  return bytes <= ARTIFACT_WRITE_MAX_BYTES ? bytes : null;
};

const validArtifactDisplayName = (value: unknown): string | null => {
  const displayName = boundedText(value, 512);
  if (
    !displayName
    || displayName.trim() !== displayName
    || displayName.includes('/')
    || displayName.includes('\\')
    || /[\u0000-\u001f\u007f]/u.test(displayName)
    || displayName === '.'
    || displayName === '..'
  ) {
    return null;
  }
  return displayName;
};

const boundedUniqueStrings = (value: unknown, limit: number): string[] | null => {
  if (!Array.isArray(value) || value.length > limit) {
    return null;
  }
  const values = value.map((item) => boundedText(item, 128));
  if (values.some((item) => item === null)) {
    return null;
  }
  const normalized = values as string[];
  return new Set(normalized).size === normalized.length ? normalized : null;
};

const serializedByteLength = (value: unknown): number | null => {
  try {
    const encoded = JSON.stringify(value);
    return typeof TextEncoder === 'undefined'
      ? encoded.length
      : new TextEncoder().encode(encoded).byteLength;
  } catch {
    return null;
  }
};

const pluginUiIframeLocation = (
  iframePath: string,
): { pathname: string; fragment: string } | null => {
  if (iframePath.startsWith('/')) {
    const [pathname, fragment, ...extra] = iframePath.split('#');
    return pathname && fragment !== undefined && extra.length === 0
      ? { pathname, fragment }
      : null;
  }
  try {
    const url = new URL(iframePath);
    const loopback = url.hostname === 'localhost'
      || url.hostname.endsWith('.localhost')
      || url.hostname === '127.0.0.1'
      || url.hostname === '[::1]';
    if (
      (url.protocol !== 'https:' && !(url.protocol === 'http:' && loopback))
      || url.username.length > 0
      || url.password.length > 0
      || url.search.length > 0
      || !url.hash.startsWith('#')
    ) {
      return null;
    }
    return { pathname: url.pathname, fragment: url.hash.slice(1) };
  } catch {
    return null;
  }
};

export const pluginUiReadySummariesFromEvents = (
  events: MessageTaskRunnerRunEvent[],
): PluginUiReadySummary[] => events.flatMap((event): PluginUiReadySummary[] => {
  if (event.event_type !== 'plugin_ui_ready') {
    return [];
  }
  const payload = record(event.payload);
  const ui = record(payload?.ui);
  const runId = boundedText(payload?.run_id);
  const pluginId = boundedText(payload?.plugin_id);
  const releaseId = boundedText(payload?.release_id);
  const artifactSha256 = lowerSha256(payload?.artifact_sha256);
  const componentKey = boundedText(payload?.component_key);
  const adapterSessionId = boundedText(payload?.adapter_session_id);
  const snapshotSha256 = lowerSha256(ui?.snapshot_sha256);
  const title = boundedText(ui?.title, 240);
  const surface = boundedText(ui?.surface, 64);
  const bridgeCapabilities = boundedUniqueStrings(ui?.bridge_capabilities, 16);
  const artifactMimeTypes = boundedUniqueStrings(ui?.artifact_mime_types, 32);
  if (
    payload?.event_schema_version !== 1
    || event.run_id !== runId
    || !boundedText(event.id)
    || !runId
    || !pluginId
    || !releaseId
    || !artifactSha256
    || !componentKey
    || !adapterSessionId
    || !snapshotSha256
    || !title
    || !surface
    || ui?.bridge_protocol_version !== BRIDGE_PROTOCOL_VERSION
    || !bridgeCapabilities
    || !artifactMimeTypes
    || bridgeCapabilities.some((capability) => !ALLOWED_BRIDGE_METHODS.has(capability))
  ) {
    return [];
  }
  return [{
    eventId: event.id,
    runId,
    pluginId,
    releaseId,
    artifactSha256,
    componentKey,
    adapterSessionId,
    snapshotSha256,
    title,
    surface,
    bridgeProtocolVersion: BRIDGE_PROTOCOL_VERSION,
    bridgeCapabilities,
    artifactMimeTypes,
  }];
});

const validArtifactDescriptor = (
  value: unknown,
  ready: PluginUiReadySummary,
): PluginArtifactDescriptor | null => {
  const artifact = record(value);
  const owner = record(artifact?.owner);
  const artifactId = pluginArtifactId(artifact?.artifact_id);
  const path = boundedText(artifact?.workspace_relative_path, 4096);
  const displayName = boundedText(artifact?.display_name, 512);
  const mediaType = boundedText(artifact?.media_type, 128);
  const sha256 = lowerSha256(artifact?.sha256);
  const producerToolName = boundedText(artifact?.producer_tool_name, 256);
  const mutable = artifact?.mutable;
  if (
    !artifactId
    || !path
    || path.startsWith('/')
    || path.includes('\\')
    || path.split('/').some((segment) => !segment || segment === '.' || segment === '..')
    || !displayName
    || path.split('/').slice(-1)[0] !== displayName
    || !mediaType
    || !ready.artifactMimeTypes.includes(mediaType)
    || typeof artifact?.size_bytes !== 'number'
    || !Number.isSafeInteger(artifact.size_bytes)
    || artifact.size_bytes < 0
    || artifact.size_bytes > 64 * 1024 * 1024
    || !sha256
    || !boundedText(artifact?.created_at, 64)
    || !producerToolName
    || artifact?.downloadable !== true
    || typeof mutable !== 'boolean'
    || owner?.run_id !== ready.runId
    || owner?.plugin_id !== ready.pluginId
    || owner?.release_id !== ready.releaseId
    || !boundedText(owner?.owner_user_id)
    || !boundedText(owner?.device_id)
    || !boundedText(owner?.workspace_id)
    || owner?.artifact_sha256 !== ready.artifactSha256
    || !boundedText(owner?.component_key)
    || !boundedText(owner?.adapter_session_id)
    || (mutable && (
      artifact.size_bytes > ARTIFACT_WRITE_MAX_BYTES
      || owner?.component_key !== ready.componentKey
      || owner?.adapter_session_id !== ready.adapterSessionId
      || !['artifact.create', 'artifact.update'].includes(producerToolName)
    ))
  ) {
    return null;
  }
  return artifact as unknown as PluginArtifactDescriptor;
};

const validArtifactAccess = (
  value: unknown,
  ready: PluginUiReadySummary,
): boolean => {
  const access = record(value);
  return Boolean(
    access
    && exactKeys(access, [
      'run_id',
      'plugin_id',
      'release_id',
      'artifact_sha256',
      'component_key',
      'adapter_session_id',
      'ui_snapshot_sha256',
    ])
    && access.run_id === ready.runId
    && access.plugin_id === ready.pluginId
    && access.release_id === ready.releaseId
    && access.artifact_sha256 === ready.artifactSha256
    && access.component_key === ready.componentKey
    && access.adapter_session_id === ready.adapterSessionId
    && access.ui_snapshot_sha256 === ready.snapshotSha256
  );
};

export const validatePluginArtifactListResponse = (
  value: unknown,
  ready: PluginUiReadySummary,
): PluginArtifactListResponse | null => {
  const response = record(value);
  if (!response || !exactKeys(response, ['access', 'artifacts']) || !validArtifactAccess(response.access, ready)) {
    return null;
  }
  if (!Array.isArray(response.artifacts) || response.artifacts.length > 1024) {
    return null;
  }
  const artifacts = response.artifacts
    .map((artifact) => validArtifactDescriptor(artifact, ready));
  if (artifacts.some((artifact) => artifact === null)) {
    return null;
  }
  const valid = artifacts as PluginArtifactDescriptor[];
  if (new Set(valid.map((artifact) => artifact.artifact_id)).size !== valid.length) {
    return null;
  }
  return {
    access: response.access as PluginArtifactListResponse['access'],
    artifacts: valid,
  };
};

export const validatePluginArtifactReadResponse = (
  value: unknown,
  ready: PluginUiReadySummary,
  artifactId: string,
): PluginArtifactReadResponse | null => {
  const response = record(value);
  const artifact = validArtifactDescriptor(response?.artifact, ready);
  if (
    !response
    || !exactKeys(response, ['access', 'artifact', 'body_base64'])
    || !validArtifactAccess(response.access, ready)
    || !artifact
    || artifact.artifact_id !== artifactId
    || typeof response.body_base64 !== 'string'
    || response.body_base64.length > 256 * 1024
  ) {
    return null;
  }
  return {
    access: response.access as PluginArtifactReadResponse['access'],
    artifact,
    body_base64: response.body_base64,
  };
};

export const validatePluginArtifactWriteResponse = (
  value: unknown,
  ready: PluginUiReadySummary,
  operation: PluginArtifactWriteOperation,
  expectedArtifactId: string | null,
  expectedCreateMetadata: Pick<PluginArtifactCreateRequest, 'display_name' | 'media_type'> | null,
  bodyBase64: string,
): PluginArtifactWriteResponse | null => {
  const response = record(value);
  const artifact = validArtifactDescriptor(response?.artifact, ready);
  const bodyBytes = canonicalBase64ByteLength(bodyBase64);
  if (
    !response
    || !exactKeys(response, ['access', 'operation', 'artifact'])
    || !validArtifactAccess(response.access, ready)
    || response.operation !== operation
    || !artifact
    || artifact.mutable !== true
    || bodyBytes === null
    || artifact.size_bytes !== bodyBytes
    || (operation === 'create' && (
      expectedArtifactId !== null
      || !expectedCreateMetadata
      || artifact.display_name !== expectedCreateMetadata.display_name
      || artifact.media_type !== expectedCreateMetadata.media_type
      || artifact.producer_tool_name !== 'artifact.create'
    ))
    || (operation === 'update' && (
      expectedCreateMetadata !== null
      || artifact.artifact_id !== expectedArtifactId
      || artifact.producer_tool_name !== 'artifact.update'
    ))
  ) {
    return null;
  }
  return {
    access: response.access as PluginArtifactWriteResponse['access'],
    operation,
    artifact,
  };
};

export const validatePluginUiWorkbenchSession = (
  value: unknown,
  ready: PluginUiReadySummary,
): PluginUiWorkbenchSessionResponse | null => {
  const session = record(value);
  const context = record(session?.host_context);
  const sessionId = boundedText(session?.session_id, 80);
  const hostSessionNonce = boundedText(session?.host_session_nonce, 80);
  const iframePath = boundedText(session?.iframe_path, 4096);
  const expiresAt = boundedText(session?.expires_at, 64);
  const capabilities = boundedUniqueStrings(session?.bridge_capabilities, 16);
  const iframeLocation = iframePath ? pluginUiIframeLocation(iframePath) : null;
  const fragmentParams = iframeLocation
    ? new URLSearchParams(iframeLocation.fragment)
    : null;
  if (
    !sessionId
    || !/^pui_[a-f0-9]{64}$/u.test(sessionId)
    || !hostSessionNonce
    || !/^puih_[a-f0-9]{64}$/u.test(hostSessionNonce)
    || !iframePath
    || iframePath.includes('\\')
    || iframePath.includes('?')
    || !iframeLocation
    || !iframeLocation.pathname.startsWith(`/api/plugin-ui/workbench/${sessionId}/`)
    || !fragmentParams?.has('chatos_plugin_ui_v1')
    || fragmentParams.get('protocol_version') !== String(BRIDGE_PROTOCOL_VERSION)
    || fragmentParams.get('adapter_session_id') !== ready.adapterSessionId
    || fragmentParams.get('host_session_nonce') !== hostSessionNonce
    || [...fragmentParams.keys()].some((key) => ![
      'chatos_plugin_ui_v1',
      'protocol_version',
      'adapter_session_id',
      'host_session_nonce',
    ].includes(key))
    || session?.bridge_protocol_version !== ready.bridgeProtocolVersion
    || session?.adapter_session_id !== ready.adapterSessionId
    || typeof session?.expires_in !== 'number'
    || !Number.isInteger(session.expires_in)
    || session.expires_in <= 0
    || session.expires_in > 300
    || !expiresAt
    || !capabilities
    || capabilities.some((capability) => !ready.bridgeCapabilities.includes(capability))
    || context?.run_id !== ready.runId
    || context?.plugin_id !== ready.pluginId
    || context?.release_id !== ready.releaseId
    || context?.component_key !== ready.componentKey
    || context?.title !== ready.title
    || context?.surface !== ready.surface
  ) {
    return null;
  }
  return {
    session_id: sessionId,
    expires_in: session.expires_in as number,
    expires_at: expiresAt,
    iframe_path: iframePath,
    bridge_protocol_version: ready.bridgeProtocolVersion,
    adapter_session_id: ready.adapterSessionId,
    host_session_nonce: hostSessionNonce,
    bridge_capabilities: [...capabilities],
    host_context: {
      run_id: ready.runId,
      plugin_id: ready.pluginId,
      release_id: ready.releaseId,
      component_key: ready.componentKey,
      title: ready.title,
      surface: ready.surface,
    },
  };
};

export const parsePluginUiBridgeIncoming = (value: unknown): PluginUiBridgeIncoming | null => {
  const bytes = serializedByteLength(value);
  if (bytes === null || bytes > BRIDGE_PAYLOAD_MAX_BYTES) {
    return null;
  }
  const message = record(value);
  if (!message) {
    return null;
  }
  const adapterSessionId = boundedText(message.adapter_session_id);
  const hostSessionNonce = boundedText(message.host_session_nonce, 80);
  if (
    message.protocol_version !== BRIDGE_PROTOCOL_VERSION
    || !adapterSessionId
    || !hostSessionNonce
  ) {
    return null;
  }
  if (message.type === READY_MESSAGE_TYPE) {
    return exactKeys(message, [
      'type',
      'protocol_version',
      'adapter_session_id',
      'host_session_nonce',
    ]) ? {
        kind: 'ready',
        adapterSessionId,
        hostSessionNonce,
      } : null;
  }
  if (message.type !== REQUEST_MESSAGE_TYPE || !exactKeys(message, [
    'type',
    'protocol_version',
    'adapter_session_id',
    'host_session_nonce',
    'request_id',
    'method',
    'payload',
  ])) {
    return null;
  }
  const requestId = boundedText(message.request_id, BRIDGE_REQUEST_ID_MAX_BYTES);
  const method = boundedText(message.method, 64);
  const payload = record(message.payload);
  if (
    !requestId
    || !/^[A-Za-z0-9._:-]+$/u.test(requestId)
    || !method
    || !ALLOWED_BRIDGE_METHODS.has(method)
    || !payload
  ) {
    return null;
  }
  if (['host.context.read', 'artifact.list'].includes(method) && Object.keys(payload).length !== 0) {
    return null;
  }
  if (
    ['artifact.read', 'artifact.download'].includes(method)
    && (
      !exactKeys(payload, ['artifact_id'])
      || !pluginArtifactId(payload.artifact_id)
    )
  ) {
    return null;
  }
  if (method === 'artifact.create') {
    const displayName = validArtifactDisplayName(payload.display_name);
    const mediaType = boundedText(payload.media_type, 128);
    if (
      !exactKeys(payload, ['display_name', 'media_type', 'body_base64'])
      || !displayName
      || !mediaType
      || canonicalBase64ByteLength(payload.body_base64) === null
    ) {
      return null;
    }
    return {
      kind: 'request',
      adapterSessionId,
      hostSessionNonce,
      requestId,
      method,
      payload: {
        display_name: displayName,
        media_type: mediaType,
        body_base64: payload.body_base64 as string,
      } satisfies PluginArtifactCreateRequest,
    };
  }
  if (method === 'artifact.update') {
    const artifactId = pluginArtifactId(payload.artifact_id);
    const expectedSha256 = lowerSha256(payload.expected_sha256);
    if (
      !exactKeys(payload, ['artifact_id', 'expected_sha256', 'body_base64'])
      || !artifactId
      || !expectedSha256
      || canonicalBase64ByteLength(payload.body_base64) === null
    ) {
      return null;
    }
    return {
      kind: 'request',
      adapterSessionId,
      hostSessionNonce,
      requestId,
      method,
      payload: {
        artifact_id: artifactId,
        expected_sha256: expectedSha256,
        body_base64: payload.body_base64 as string,
      } satisfies PluginArtifactUpdateRequest & { artifact_id: string },
    };
  }
  return {
    kind: 'request',
    adapterSessionId,
    hostSessionNonce,
    requestId,
    method,
    payload,
  };
};

export const isPluginUiBridgeMessageSource = (
  origin: string,
  source: MessageEventSource | null,
  expectedSource: WindowProxy | null,
): boolean => Boolean(expectedSource) && origin === 'null' && source === expectedSource;

export const pluginUiBridgeResponse = (
  session: PluginUiWorkbenchSessionResponse,
  requestId: string,
  ok: boolean,
  result: unknown,
  errorCode: string | null,
): PluginUiBridgeResponseMessage => ({
  type: RESPONSE_MESSAGE_TYPE,
  protocol_version: BRIDGE_PROTOCOL_VERSION,
  adapter_session_id: session.adapter_session_id,
  host_session_nonce: session.host_session_nonce,
  request_id: requestId,
  ok,
  result,
  error_code: errorCode,
});
