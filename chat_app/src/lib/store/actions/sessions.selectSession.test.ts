import { vi } from 'vitest';

vi.mock('../helpers/sessions', () => ({
  fetchSession: vi.fn(),
}));

vi.mock('../helpers/messages', () => ({
  fetchSessionMessages: vi.fn(),
}));

import './sessions.selectSession.test/realtimeSync';
import './sessions.selectSession.test/selectionFlow';
import './sessions.selectSession.test/cache';
import './sessions.selectSession.test/pagination';
