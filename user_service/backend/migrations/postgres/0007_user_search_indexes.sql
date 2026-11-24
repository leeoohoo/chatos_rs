-- SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
-- Required Notice: Copyright (c) 2025 AI Chat Team

CREATE INDEX users_username_search_idx
    ON users (lower(username) text_pattern_ops);
CREATE INDEX users_display_name_search_idx
    ON users (lower(display_name) text_pattern_ops);
