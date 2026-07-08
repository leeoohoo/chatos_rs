// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { describe, expect, it } from 'vitest';

import {
  assertAttachmentsWithinTotalBudget,
  DEFAULT_ATTACHMENT_TOTAL_MAX_BYTES,
  estimateAttachmentTotalBytes,
  requestPayloadMaxBytesForAttachmentTotalLimit,
  resolveAttachmentTotalMaxBytes,
} from './attachments';

const fileWithSize = (size: number): File => ({ size }) as File;

describe('sendMessage attachments budget', () => {
  it('defaults attachment total limit to 20 MB', () => {
    expect(resolveAttachmentTotalMaxBytes(undefined)).toBe(20 * 1024 * 1024);
    expect(DEFAULT_ATTACHMENT_TOTAL_MAX_BYTES).toBe(20 * 1024 * 1024);
  });

  it('checks total original attachment size', () => {
    const attachments = [fileWithSize(3), fileWithSize(5)];

    expect(estimateAttachmentTotalBytes(attachments)).toBe(8);
    expect(() => assertAttachmentsWithinTotalBudget(attachments, 8)).not.toThrow();
    expect(() => assertAttachmentsWithinTotalBudget(attachments, 7)).toThrow('附件总大小');
  });

  it('reserves base64 transport overhead for payload precheck', () => {
    expect(requestPayloadMaxBytesForAttachmentTotalLimit(3)).toBe(4 + 1024 * 1024);
  });
});
