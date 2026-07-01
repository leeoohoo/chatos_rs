// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import axios, { AxiosHeaders } from 'axios';

import { getAuthToken } from './userService';

const resolveDefaultBaseUrl = () => {
  const host =
    typeof window !== 'undefined' && window.location.hostname
      ? window.location.hostname
      : '127.0.0.1';
  const port = import.meta.env.VITE_MEMORY_ENGINE_PORT ?? '7081';
  return `http://${host}:${port}/api/memory-engine/v1`;
};

const baseURL = import.meta.env.VITE_MEMORY_ENGINE_API_BASE ?? resolveDefaultBaseUrl();
const operatorToken = import.meta.env.VITE_MEMORY_ENGINE_OPERATOR_TOKEN;

export const client = axios.create({
  baseURL,
  timeout: 30000,
});

if (operatorToken?.trim()) {
  client.defaults.headers.common['x-memory-operator-token'] = operatorToken.trim();
}

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
