// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

export type PluginTrustLevel = 'bundled' | 'trusted' | 'untrusted';
export type PluginMarketplaceSource =
  | 'official_registry'
  | 'admin_registry'
  | 'local_directory';
export type PluginVisibility = 'public' | 'private';
export type PluginReleaseChannel = 'stable' | 'beta' | 'canary';
export type PluginExecutionHost = 'local' | 'portable';
export type PluginRuntimeTarget = 'local_connector';
export type PluginPublisherStatus = 'pending' | 'approved' | 'rejected' | 'suspended';
export type PluginInstallStatus =
  | 'not_installed'
  | 'downloading'
  | 'verifying'
  | 'rejected'
  | 'installing'
  | 'installed'
  | 'updating'
  | 'rolling_back'
  | 'uninstalling';
export type PluginAvailabilityStatus =
  | 'unavailable'
  | 'needs_dependency'
  | 'needs_permission'
  | 'needs_auth'
  | 'ready'
  | 'partially_available'
  | 'unsupported_platform'
  | 'offline'
  | 'revoked';
export type PluginRequirementStatus =
  | 'unknown'
  | 'pending'
  | 'satisfied'
  | 'denied'
  | 'missing'
  | 'failed';
export type PluginComponentKind =
  | 'skill_collection'
  | 'mcp_server'
  | 'connected_app'
  | 'command'
  | 'agent'
  | 'hook_set'
  | 'ui_contribution';

export interface SigningKeyRef {
  key_id: string;
  publisher_id: string;
  algorithm: string;
  public_key_base64: string;
  usages?: Array<'catalog' | 'release'>;
  valid_from: string;
  valid_until?: string | null;
  revoked_at?: string | null;
}

export interface PluginMarketplaceRecord {
  id: string;
  name: string;
  owner_user_id?: string | null;
  visibility: PluginVisibility;
  source_kind: PluginMarketplaceSource;
  catalog_url?: string | null;
  enabled: boolean;
  trust_level: PluginTrustLevel;
  trusted_signing_keys: SigningKeyRef[];
  last_catalog_revision?: string | null;
  last_synced_at?: string | null;
}

export interface PluginCatalogSyncResponse {
  marketplace_id: string;
  revision: string;
  issued_at: string;
  catalog_sha256: string;
  plugin_count: number;
  release_count: number;
  component_snapshot_count: number;
  signing_key_count: number;
  synced_at: string;
}

export interface PluginAuditLogRecord {
  id: string;
  event: string;
  owner_user_id: string;
  device_id?: string | null;
  plugin_id: string;
  release_id?: string | null;
  component_key?: string | null;
  outcome: string;
  details: Record<string, unknown>;
  created_at: string;
}

export interface PluginPublisherRecord {
  id: string;
  publisher_id: string;
  marketplace_id: string;
  owner_user_id: string;
  name: string;
  website?: string | null;
  status: PluginPublisherStatus;
  signing_keys: SigningKeyRef[];
  submitted_at: string;
  reviewed_at?: string | null;
  reviewed_by?: string | null;
  review_note?: string | null;
  created_at: string;
  updated_at: string;
}

export interface PluginPublisher {
  id: string;
  name: string;
  website?: string | null;
  verified: boolean;
}

export interface PluginLicenseMetadata {
  license_id: string;
  license_url?: string | null;
  redistributable: boolean;
  reviewed_at?: string | null;
}

export interface PluginInterfaceMetadata {
  displayName: string;
  shortDescription: string;
  longDescription: string;
  developerName: string;
  category: string;
  capabilities: string[];
  websiteURL?: string | null;
  privacyPolicyURL?: string | null;
  termsOfServiceURL?: string | null;
  defaultPrompt: string[];
  brandColor?: string | null;
  composerIcon?: { path: string } | null;
  logo?: { path: string } | null;
  logoDark?: { path: string } | null;
  screenshots: Array<{ path: string }>;
}

export interface PluginCatalogRecord {
  id: string;
  plugin_key: string;
  marketplace_id: string;
  owner_user_id?: string | null;
  name: string;
  display_name: string;
  description: string;
  publisher: PluginPublisher;
  interface: PluginInterfaceMetadata;
  keywords: string[];
  visibility: PluginVisibility;
  featured: boolean;
  enabled: boolean;
  latest_release_id: string;
  license: PluginLicenseMetadata;
  created_at: string;
  updated_at: string;
}

export interface PluginCatalogListItem extends PluginCatalogRecord {
  runtime_targets: PluginRuntimeTarget[];
}

export interface PluginPermissionRequirement {
  permission: string;
  required: boolean;
  reason?: string | null;
  components: string[];
}

export interface PluginComponentDescriptor {
  component_key: string;
  kind: PluginComponentKind;
  execution_host: PluginExecutionHost;
  display_name: string;
  runtime_kind: string;
  entrypoint?: { path: string } | null;
  required: boolean;
  permissions: PluginPermissionRequirement[];
  metadata: Record<string, unknown>;
}

export interface PluginReleaseSignature {
  key_id: string;
  publisher_id: string;
  marketplace_id: string;
  algorithm: string;
  signature_base64: string;
  signed_at: string;
  manifest_sha256: string;
}

export interface PluginReleaseRecord {
  id: string;
  plugin_id: string;
  version: string;
  manifest_schema_version: number;
  normalized_manifest: Record<string, unknown>;
  artifact_ref: string;
  artifact_sha256: string;
  signature: PluginReleaseSignature;
  sbom_ref?: string | null;
  supported_platforms: string[];
  components: PluginComponentDescriptor[];
  dependencies: Record<string, unknown>;
  permissions: PluginPermissionRequirement[];
  release_channel: PluginReleaseChannel;
  published_at: string;
  revoked_at?: string | null;
}

export interface PluginComponentStatusRecord {
  component_key: string;
  kind: PluginComponentKind;
  availability_status: PluginAvailabilityStatus;
  last_error?: string | null;
  last_checked_at: string;
}

export interface PluginInstallationRecord {
  id: string;
  owner_user_id: string;
  device_id: string;
  plugin_id: string;
  release_id: string;
  version: string;
  artifact_sha256: string;
  platform: string;
  install_status: PluginInstallStatus;
  availability_status: PluginAvailabilityStatus;
  dependency_status: PluginRequirementStatus;
  permission_status: PluginRequirementStatus;
  auth_status: PluginRequirementStatus;
  component_statuses: PluginComponentStatusRecord[];
  active: boolean;
  previous_release_id?: string | null;
  installed_at: string;
  last_checked_at: string;
  last_error?: string | null;
}

export interface PluginOAuthConnectionRecord {
  id: string;
  owner_user_id: string;
  device_id: string;
  plugin_id: string;
  release_id: string;
  component_key: string;
  provider: string;
  scopes: string[];
  connected: boolean;
  expires_at?: string | null;
  account_display?: string | null;
  updated_at: string;
}
