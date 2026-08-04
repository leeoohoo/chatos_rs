// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

export interface LocalRuntimeBridgeResponse {
  status: number;
  ok: boolean;
  headers?: Record<string, string | string[] | undefined>;
  body?: string;
}

export interface LocalRuntimeBridgeRequest {
  endpoint: string;
  method?: string;
  headers?: Record<string, string>;
  body?: string | null;
}
