UPDATE user_model_configs
SET task_enabled = enabled,
    data = jsonb_set(data, '{task_enabled}', to_jsonb(enabled), true)
WHERE task_enabled IS NULL;

WITH provider_matches AS (
    SELECT DISTINCT ON (config.id)
           config.id AS config_id,
           provider.id AS provider_id,
           COALESCE((provider.data->>'enabled')::boolean, TRUE) AS provider_enabled
    FROM user_model_configs config
    JOIN user_model_providers provider
      ON provider.owner_user_id = config.owner_user_id
     AND (
          provider.id = config.source_provider_id
          OR (
              config.source_provider_id IS NULL
              AND provider.data->>'provider' = config.data->>'provider'
              AND RTRIM(BTRIM(COALESCE(provider.data->>'base_url', '')), '/') =
                  RTRIM(BTRIM(COALESCE(config.data->>'base_url', '')), '/')
          )
     )
    ORDER BY config.id,
             (provider.id = config.source_provider_id) DESC,
             provider.updated_at DESC,
             provider.created_at DESC,
             provider.id
)
UPDATE user_model_configs config
SET source_provider_id = COALESCE(config.source_provider_id, matched.provider_id),
    enabled = matched.provider_enabled,
    updated_at = now(),
    data = jsonb_set(
        jsonb_set(
            jsonb_set(config.data, '{source_provider_id}', to_jsonb(COALESCE(config.source_provider_id, matched.provider_id)), true),
            '{enabled}',
            to_jsonb(matched.provider_enabled),
            true
        ),
        '{updated_at}',
        to_jsonb(to_char(now() AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.US"Z"')),
        true
    )
FROM provider_matches matched
WHERE config.id = matched.config_id
  AND (
      config.source_provider_id IS NULL
      OR config.enabled IS DISTINCT FROM matched.provider_enabled
  );
