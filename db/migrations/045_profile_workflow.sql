-- User templates are data, separately versioned from deployed profiles.
CREATE TABLE profile_templates (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    name text NOT NULL CHECK (length(name) BETWEEN 1 AND 160),
    description_ru text NOT NULL DEFAULT '',
    description_en text NOT NULL DEFAULT '',
    author text NOT NULL DEFAULT '',
    source_url text,
    source_revision text,
    config jsonb NOT NULL CHECK (jsonb_typeof(config) = 'object'),
    version integer NOT NULL DEFAULT 1,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now()
);
CREATE TABLE profile_revisions (
    profile_id bigint NOT NULL REFERENCES config_profiles ON DELETE CASCADE,
    version integer NOT NULL,
    config jsonb NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (profile_id, version)
);
INSERT INTO profile_revisions(profile_id,version,config) SELECT id,version,config FROM config_profiles;
ALTER TABLE nodes ADD COLUMN reported_config_version integer, ADD COLUMN reported_users_version text, ADD COLUMN safe_engine_update boolean NOT NULL DEFAULT false;
-- A rehearsal uses an isolated node and a separate candidate profile. Production
-- profile bindings remain unchanged until an explicit publish operation.
CREATE TABLE profile_trials (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    profile_id bigint NOT NULL REFERENCES config_profiles ON DELETE CASCADE,
    candidate_id bigint NOT NULL REFERENCES config_profiles ON DELETE RESTRICT,
    node_id bigint NOT NULL REFERENCES nodes ON DELETE RESTRICT,
    base_version integer NOT NULL,
    candidate_version integer NOT NULL,
    previous_profile_id bigint REFERENCES config_profiles ON DELETE RESTRICT,
    previous_inbounds jsonb NOT NULL DEFAULT '[]',
    state text NOT NULL DEFAULT 'testing' CHECK (state IN ('testing','finished')),
    created_at timestamptz NOT NULL DEFAULT now()
);
CREATE UNIQUE INDEX profile_trial_node ON profile_trials(node_id) WHERE state='testing';
CREATE UNIQUE INDEX profile_trial_active ON profile_trials(profile_id) WHERE state='testing';
