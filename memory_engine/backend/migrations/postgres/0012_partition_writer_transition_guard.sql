-- SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
-- Required Notice: Copyright (c) 2025 AI Chat Team

-- 0011 establishes the compatible partition state. Keep the cutover monotonic
-- even for operator SQL that bypasses the application repository.
CREATE FUNCTION engine_guard_partition_writer_transition()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
BEGIN
    IF (NEW.migration_state, NEW.authoritative_writer)
       IS NOT DISTINCT FROM
       (OLD.migration_state, OLD.authoritative_writer) THEN
        RETURN NEW;
    END IF;

    IF OLD.migration_state = 'backfilling'
       AND OLD.authoritative_writer = 'v1'
       AND NEW.migration_state = 'ready'
       AND NEW.authoritative_writer = 'v1' THEN
        RETURN NEW;
    END IF;

    IF OLD.migration_state = 'ready'
       AND OLD.authoritative_writer = 'v1'
       AND NEW.migration_state = 'authoritative'
       AND NEW.authoritative_writer = 'v2' THEN
        RETURN NEW;
    END IF;

    RAISE EXCEPTION 'invalid partition writer transition: %/% -> %/%',
        OLD.migration_state,
        OLD.authoritative_writer,
        NEW.migration_state,
        NEW.authoritative_writer
        USING ERRCODE = 'check_violation';
END;
$$;

CREATE TRIGGER engine_thread_partition_writer_transition_guard
BEFORE UPDATE OF migration_state, authoritative_writer
ON engine_thread_summary_partitions
FOR EACH ROW
EXECUTE FUNCTION engine_guard_partition_writer_transition();
