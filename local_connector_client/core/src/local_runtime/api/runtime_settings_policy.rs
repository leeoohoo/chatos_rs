// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

const MIN_MEMORY_MESSAGE_THRESHOLD: i64 = 4;
const MAX_MEMORY_MESSAGE_THRESHOLD: i64 = 2_000;
const MIN_MEMORY_CHARACTER_THRESHOLD: i64 = 4_000;
const MAX_MEMORY_CHARACTER_THRESHOLD: i64 = 2_000_000;
const MIN_MEMORY_RECALL_LIMIT: i64 = 2;
const MAX_MEMORY_RECALL_LIMIT: i64 = 50;

pub(super) fn clamp_memory_message_threshold(value: i64) -> i64 {
    value.clamp(MIN_MEMORY_MESSAGE_THRESHOLD, MAX_MEMORY_MESSAGE_THRESHOLD)
}

pub(super) fn clamp_memory_character_threshold(value: i64) -> i64 {
    value.clamp(
        MIN_MEMORY_CHARACTER_THRESHOLD,
        MAX_MEMORY_CHARACTER_THRESHOLD,
    )
}

pub(super) fn clamp_memory_recall_limit(value: i64) -> i64 {
    value.clamp(MIN_MEMORY_RECALL_LIMIT, MAX_MEMORY_RECALL_LIMIT)
}

#[cfg(test)]
mod tests {
    use super::{
        clamp_memory_character_threshold, clamp_memory_message_threshold, clamp_memory_recall_limit,
    };

    #[test]
    fn clamps_local_memory_policy_thresholds() {
        assert_eq!(clamp_memory_message_threshold(0), 4);
        assert_eq!(clamp_memory_message_threshold(99), 99);
        assert_eq!(clamp_memory_character_threshold(10), 4_000);
        assert_eq!(clamp_memory_character_threshold(64_000), 64_000);
        assert_eq!(clamp_memory_recall_limit(0), 2);
        assert_eq!(clamp_memory_recall_limit(12), 12);
    }
}
