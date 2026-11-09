CREATE EXTENSION IF NOT EXISTS pg_trgm;

CREATE TABLE engine_memory_fact_projections (
    tenant_id TEXT NOT NULL,
    source_id TEXT NOT NULL,
    fact_id TEXT NOT NULL,
    output_kind TEXT NOT NULL CHECK (output_kind IN ('thread_summary','subject_memory')),
    output_id TEXT NOT NULL,
    scope_kind TEXT NOT NULL CHECK (scope_kind IN ('thread','project','agent')),
    scope_key TEXT NOT NULL,
    project_partition_key TEXT,
    fact_key TEXT NOT NULL,
    normalized_value_hash TEXT NOT NULL,
    searchable_text TEXT NOT NULL CHECK (octet_length(searchable_text) <= 16384),
    status TEXT NOT NULL CHECK (status IN ('active','superseded','uncertain','retired')),
    confidence TEXT NOT NULL CHECK (confidence IN (
        'explicit_user','tool_evidence','model_inference','uncertain'
    )),
    observed_at TIMESTAMPTZ,
    source_refs JSONB NOT NULL CHECK (jsonb_typeof(source_refs) = 'array'),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    retired_at TIMESTAMPTZ,
    PRIMARY KEY (tenant_id, source_id, fact_id),
    UNIQUE (tenant_id, source_id, output_kind, output_id, fact_key, normalized_value_hash)
);

CREATE INDEX engine_memory_fact_scope_idx
ON engine_memory_fact_projections(
    tenant_id, source_id, scope_kind, scope_key, status, observed_at DESC
);

CREATE INDEX engine_memory_fact_key_idx
ON engine_memory_fact_projections(
    tenant_id, source_id, fact_key, normalized_value_hash, status
);

CREATE INDEX engine_memory_fact_search_trgm_idx
ON engine_memory_fact_projections USING gin (searchable_text gin_trgm_ops);

CREATE TABLE engine_memory_fact_supersessions (
    tenant_id TEXT NOT NULL,
    source_id TEXT NOT NULL,
    newer_fact_id TEXT NOT NULL,
    superseded_fact_id TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, source_id, newer_fact_id, superseded_fact_id),
    FOREIGN KEY (tenant_id, source_id, newer_fact_id)
        REFERENCES engine_memory_fact_projections(tenant_id, source_id, fact_id),
    FOREIGN KEY (tenant_id, source_id, superseded_fact_id)
        REFERENCES engine_memory_fact_projections(tenant_id, source_id, fact_id),
    CHECK (newer_fact_id <> superseded_fact_id)
);

CREATE OR REPLACE FUNCTION engine_validate_memory_fact_output()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF NEW.output_kind = 'thread_summary' THEN
        IF NOT EXISTS (
            SELECT 1 FROM engine_summaries
            WHERE tenant_id = NEW.tenant_id AND source_id = NEW.source_id AND id = NEW.output_id
        ) THEN
            RAISE EXCEPTION 'memory fact thread summary output does not exist';
        END IF;
    ELSIF NOT EXISTS (
        SELECT 1 FROM engine_subject_memories
        WHERE tenant_id = NEW.tenant_id AND source_id = NEW.source_id AND id = NEW.output_id
    ) THEN
        RAISE EXCEPTION 'memory fact subject memory output does not exist';
    END IF;
    RETURN NEW;
END;
$$;

CREATE CONSTRAINT TRIGGER engine_memory_fact_output_exists
AFTER INSERT OR UPDATE OF output_kind, output_id
ON engine_memory_fact_projections
DEFERRABLE INITIALLY DEFERRED
FOR EACH ROW EXECUTE FUNCTION engine_validate_memory_fact_output();

CREATE OR REPLACE FUNCTION engine_validate_memory_fact_supersession()
RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE
    newer engine_memory_fact_projections%ROWTYPE;
    older engine_memory_fact_projections%ROWTYPE;
BEGIN
    SELECT * INTO newer FROM engine_memory_fact_projections
      WHERE tenant_id = NEW.tenant_id AND source_id = NEW.source_id
        AND fact_id = NEW.newer_fact_id FOR UPDATE;
    SELECT * INTO older FROM engine_memory_fact_projections
      WHERE tenant_id = NEW.tenant_id AND source_id = NEW.source_id
        AND fact_id = NEW.superseded_fact_id FOR UPDATE;
    IF newer.fact_id IS NULL OR older.fact_id IS NULL THEN
        RAISE EXCEPTION 'memory fact supersession endpoint is missing';
    END IF;
    IF newer.scope_kind <> older.scope_kind OR newer.scope_key <> older.scope_key
       OR newer.project_partition_key IS DISTINCT FROM older.project_partition_key THEN
        RAISE EXCEPTION 'memory fact supersession crosses scope';
    END IF;
    IF newer.created_at < older.created_at THEN
        RAISE EXCEPTION 'older memory fact cannot supersede newer fact';
    END IF;
    IF EXISTS (
        WITH RECURSIVE reachable(fact_id) AS (
            SELECT superseded_fact_id
            FROM engine_memory_fact_supersessions
            WHERE tenant_id = NEW.tenant_id AND source_id = NEW.source_id
              AND newer_fact_id = NEW.superseded_fact_id
            UNION
            SELECT edge.superseded_fact_id
            FROM engine_memory_fact_supersessions edge
            JOIN reachable path ON edge.newer_fact_id = path.fact_id
            WHERE edge.tenant_id = NEW.tenant_id AND edge.source_id = NEW.source_id
        )
        SELECT 1 FROM reachable WHERE fact_id = NEW.newer_fact_id
    ) THEN
        RAISE EXCEPTION 'memory fact supersession cycle';
    END IF;
    RETURN NEW;
END;
$$;

CREATE TRIGGER engine_memory_fact_supersession_validate
BEFORE INSERT ON engine_memory_fact_supersessions
FOR EACH ROW EXECUTE FUNCTION engine_validate_memory_fact_supersession();

CREATE INDEX engine_memory_fact_superseded_idx
ON engine_memory_fact_supersessions(tenant_id, source_id, superseded_fact_id);
