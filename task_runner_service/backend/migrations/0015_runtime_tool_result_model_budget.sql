ALTER TABLE runtime_settings
  ADD COLUMN tool_result_model_max_chars INTEGER NOT NULL DEFAULT 8000;

ALTER TABLE runtime_settings
  ADD COLUMN tool_results_model_total_max_chars INTEGER NOT NULL DEFAULT 48000;
