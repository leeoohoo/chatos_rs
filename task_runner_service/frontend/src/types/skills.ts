// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

export type SkillSource = 'manual' | 'url' | 'registry' | 'bundled';

export type SkillInstallStatus = 'installed' | 'disabled' | 'failed';

export type SkillScope = 'user' | 'admin_global';

export interface SkillRecord {
  id: string;
  name: string;
  display_name: string;
  description?: string | null;
  content: string;
  locale: string;
  tags: string[];
  source: SkillSource;
  source_url?: string | null;
  source_registry?: string | null;
  source_package_id?: string | null;
  version?: string | null;
  checksum?: string | null;
  package_root?: string | null;
  package_manifest?: SkillPackageFile[];
  package_file_count?: number;
  package_total_bytes?: number;
  source_repo?: string | null;
  source_ref?: string | null;
  source_path?: string | null;
  install_status: SkillInstallStatus;
  enabled: boolean;
  auto_inject: boolean;
  scope: SkillScope;
  creator_user_id?: string | null;
  creator_username?: string | null;
  creator_display_name?: string | null;
  owner_user_id?: string | null;
  owner_username?: string | null;
  owner_display_name?: string | null;
  created_at: string;
  updated_at: string;
  installed_at?: string | null;
}

export interface SkillPackageFile {
  path: string;
  size_bytes: number;
  source_url?: string | null;
}

export interface SkillListFilters {
  keyword?: string;
  enabled?: boolean;
  auto_inject?: boolean;
  source?: SkillSource;
  locale?: string;
}

export interface CreateSkillPayload {
  name?: string;
  display_name: string;
  description?: string;
  content?: string;
  locale?: string;
  tags?: string[];
  source_url?: string;
  enabled?: boolean;
  auto_inject?: boolean;
}

export type UpdateSkillPayload = Partial<CreateSkillPayload>;

export interface SkillMarketplaceQuery {
  keyword?: string;
  locale?: string;
  tag?: string;
  limit?: number;
  offset?: number;
}

export interface SkillMarketplaceEntry {
  registry: string;
  package_id: string;
  name: string;
  display_name: string;
  description: string;
  locale: string;
  tags: string[];
  version?: string | null;
  source_url?: string | null;
  checksum?: string | null;
  package_file_count?: number;
  package_total_bytes?: number;
  installed_skill_id?: string | null;
  installed: boolean;
  preview_content?: string | null;
}

export interface InstallSkillPayload {
  registry?: string;
  package_id: string;
  enabled?: boolean;
  auto_inject?: boolean;
}
