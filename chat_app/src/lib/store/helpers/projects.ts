import type { Project } from '../../../types';

export const normalizeProject = (raw: any): Project => ({
  id: raw?.id,
  name: raw?.name ?? '',
  rootPath: raw?.root_path ?? raw?.rootPath ?? '',
  description: raw?.description ?? null,
  userId: raw?.user_id ?? raw?.userId ?? null,
  createdAt: new Date(raw?.created_at ?? raw?.createdAt ?? Date.now()),
  updatedAt: new Date(raw?.updated_at ?? raw?.updatedAt ?? raw?.created_at ?? raw?.createdAt ?? Date.now()),
});
