// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import axios, { AxiosHeaders } from 'axios';

import { clearAuthToken, getAuthToken } from './userService';

import { ADMIN_SERVICE_BASES } from '../../../shared/api/servicePaths';

const baseURL = ADMIN_SERVICE_BASES.memoryEngine;

export const client = axios.create({
  baseURL,
  timeout: 30000,
});

client.interceptors.request.use((config) => {
  const authToken = getAuthToken()?.trim();
  if (!authToken) {
    return config;
  }

  const headers = AxiosHeaders.from(config.headers);
  if (!headers.has('Authorization')) {
    headers.set('Authorization', `Bearer ${authToken}`);
  }
  config.headers = headers;
  return config;
});

client.interceptors.response.use(
  (response) => response,
  (error: unknown) => {
    if (axios.isAxiosError(error) && error.response?.status === 401) {
      clearAuthToken();
    }
    return Promise.reject(error);
  },
);
