-- Client credentials are independent of Telegram, subscription links and admins.
CREATE TABLE cabinet_credentials (
    client_id bigint PRIMARY KEY REFERENCES clients(id),
    code_hash bytea NOT NULL UNIQUE,
    registration_hash bytea UNIQUE,
    saved_at timestamptz,
    created_at timestamptz NOT NULL DEFAULT now(),
    rotated_at timestamptz NOT NULL DEFAULT now()
);
CREATE TABLE cabinet_installations (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    name text NOT NULL,
    public_url text NOT NULL UNIQUE,
    server_ip text NOT NULL,
    placement text NOT NULL CHECK (placement IN ('same','separate')),
    token_hash bytea UNIQUE,
    token_prefix text,
    last_seen_at timestamptz,
    revoked_at timestamptz,
    created_at timestamptz NOT NULL DEFAULT now()
);
CREATE TABLE cabinet_sessions (
    token_hash bytea PRIMARY KEY,
    csrf_hash bytea NOT NULL,
    client_id bigint NOT NULL REFERENCES clients(id),
    installation_id uuid NOT NULL REFERENCES cabinet_installations(id),
    expires_at timestamptz NOT NULL,
    last_seen_at timestamptz NOT NULL DEFAULT now(),
    created_at timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX cabinet_sessions_client_idx ON cabinet_sessions(client_id);
CREATE TABLE cabinet_auth_limits (
    bucket text NOT NULL,
    window_start bigint NOT NULL,
    hits integer NOT NULL,
    PRIMARY KEY(bucket,window_start)
);
CREATE TABLE cabinet_links (
    token_hash bytea PRIMARY KEY,
    client_id bigint NOT NULL REFERENCES clients(id),
    installation_id uuid NOT NULL REFERENCES cabinet_installations(id),
    expires_at timestamptz NOT NULL DEFAULT now()+interval '5 minutes',
    used_at timestamptz
);
-- No publishing defaults or invented business content. Owner fills the site.
INSERT INTO settings(key,value) VALUES ('cabinet.config','{"enabled":false,"registration_enabled":false,"shop_enabled":false,"devices_enabled":true,"referral_enabled":false,"tickets_enabled":true,"faq":[],"docs_links":[]}'::jsonb) ON CONFLICT DO NOTHING;
