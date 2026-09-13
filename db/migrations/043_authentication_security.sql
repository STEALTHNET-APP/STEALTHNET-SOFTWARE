-- Shared across API processes; raw IPs and attempted account names are not stored.
CREATE TABLE admin_auth_limits (
    bucket bytea NOT NULL,
    window_start bigint NOT NULL,
    hits integer NOT NULL CHECK (hits > 0),
    PRIMARY KEY (bucket, window_start)
);
CREATE INDEX admin_auth_limits_window_idx ON admin_auth_limits(window_start);

-- A successful password + TOTP login consumes its time window atomically.
ALTER TABLE admins ADD COLUMN totp_last_used_step bigint;
