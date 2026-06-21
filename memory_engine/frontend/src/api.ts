import { adminApi } from './api/admin';
import { threadsApi } from './api/threads';

export const api = {
  ...adminApi,
  ...threadsApi,
};
