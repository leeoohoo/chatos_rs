-- SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
-- Required Notice: Copyright (c) 2025 AI Chat Team

-- A finalizer's logical owner is stable across worker restarts, while every
-- claim receives a fresh fencing token. Permit only that exact re-entry shape;
-- a different owner must still wait for lease expiry.
CREATE OR REPLACE FUNCTION engine_guard_generation_transition()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
BEGIN
    IF (NEW.job_run_id,NEW.generation_step_key,NEW.generation_kind,NEW.input_digest,
        NEW.input_manifest,NEW.config_snapshot,NEW.project_partition_key)
       IS DISTINCT FROM
       (OLD.job_run_id,OLD.generation_step_key,OLD.generation_kind,OLD.input_digest,
        OLD.input_manifest,OLD.config_snapshot,OLD.project_partition_key) THEN
        RAISE EXCEPTION 'generation frozen input and configuration are immutable'
            USING ERRCODE='check_violation';
    END IF;

    IF OLD.status='committed' OR OLD.status='abandoned' THEN
        IF NEW IS DISTINCT FROM OLD THEN
            RAISE EXCEPTION 'terminal generation state is immutable'
                USING ERRCODE='check_violation';
        END IF;
        RETURN NEW;
    END IF;

    IF OLD.status=NEW.status THEN
        IF OLD.status='running'
           AND NEW.lease_token IS DISTINCT FROM OLD.lease_token
           AND (OLD.lease_expires_at IS NULL OR OLD.lease_expires_at>now())
           AND (
                NULLIF(OLD.lease_owner,'') IS NULL
                OR NEW.lease_owner IS DISTINCT FROM OLD.lease_owner
                OR NULLIF(NEW.lease_token,'') IS NULL
                OR NEW.attempt_count IS DISTINCT FROM OLD.attempt_count+1
                OR NEW.lease_acquired_at IS NULL
                OR NEW.lease_expires_at IS NULL
                OR NEW.lease_expires_at<=now()
           ) THEN
            RAISE EXCEPTION 'active generation lease cannot be replaced by a different owner'
                USING ERRCODE='check_violation';
        END IF;
        RETURN NEW;
    END IF;

    IF (OLD.status='prepared' AND NEW.status='running')
       OR (OLD.status='running' AND NEW.status IN ('prepared','committed','abandoned'))
       OR (OLD.status='prepared' AND NEW.status='abandoned') THEN
        RETURN NEW;
    END IF;

    RAISE EXCEPTION 'invalid generation state transition: % -> %',OLD.status,NEW.status
        USING ERRCODE='check_violation';
END;
$$;
