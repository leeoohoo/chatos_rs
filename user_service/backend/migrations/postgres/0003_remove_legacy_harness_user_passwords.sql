-- Remove reversible ChatOS user passwords retained by pre-3.0.7 Harness
-- provisioning records. The marker keeps login and retry paths fail-closed.
UPDATE harness_provisioning
SET status = 'failed',
    data = jsonb_set(
        jsonb_set(
            jsonb_set(
                data - 'encrypted_provisioning_secret',
                '{credential_kind}',
                to_jsonb('legacy_user_password_removed_v1'::text),
                true
            ),
            '{status}',
            to_jsonb('failed'::text),
            true
        ),
        '{last_error}',
        to_jsonb('legacy Harness provisioning credential requires administrator recovery'::text),
        true
    )
WHERE data ? 'encrypted_provisioning_secret'
  AND COALESCE(data ->> 'credential_kind', '') = '';
