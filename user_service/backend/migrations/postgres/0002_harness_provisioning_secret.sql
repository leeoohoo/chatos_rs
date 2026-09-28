-- Rename the ambiguous legacy retry field without changing its value. Records
-- without the generated credential marker remain rejected by the application.
UPDATE harness_provisioning
SET data = CASE
    WHEN data ? 'encrypted_provisioning_secret' THEN data - 'encrypted_password'
    ELSE jsonb_set(
        data - 'encrypted_password',
        '{encrypted_provisioning_secret}',
        data -> 'encrypted_password',
        true
    )
END
WHERE data ? 'encrypted_password';
