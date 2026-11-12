-- Model budgets are exclusively published by Configuration Center.
-- Remove the independent per-user copies so a restart or import cannot resurrect them.
UPDATE user_model_configs SET data = data - ARRAY[
    'model_capability_revision', 'context_window_tokens', 'output_reserve_tokens',
    'protocol_overhead_estimator_id', 'tokenizer_id', 'responses_compaction_threshold'
]::text[] WHERE data ?| ARRAY[
    'model_capability_revision', 'context_window_tokens', 'output_reserve_tokens',
    'protocol_overhead_estimator_id', 'tokenizer_id', 'responses_compaction_threshold'
]::text[];
