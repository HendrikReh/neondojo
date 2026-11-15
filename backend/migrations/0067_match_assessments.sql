CREATE TABLE match_assessments (
    id uuid PRIMARY KEY,
    report_id uuid NOT NULL,
    report_version bigint NOT NULL,
    language text NOT NULL CHECK (language IN ('de','en','es','fr','it','pl')),
    prompt_version integer NOT NULL,
    model text NOT NULL,
    status text NOT NULL CHECK (status IN ('queued','running','completed','failed')),
    result jsonb,
    error_code text,
    lease_token uuid,
    lease_until timestamptz,
    created_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (report_id, report_version, language, prompt_version),
    CHECK ((status = 'completed') = (result IS NOT NULL)),
    CHECK ((status = 'failed') = (error_code IS NOT NULL)),
    CHECK ((status = 'running') = (lease_token IS NOT NULL AND lease_until IS NOT NULL))
);
-- Legacy reports can use their session ID, so report_id has no single FK target.
CREATE INDEX match_assessments_queue ON match_assessments(created_at, id)
    WHERE status IN ('queued','running');
