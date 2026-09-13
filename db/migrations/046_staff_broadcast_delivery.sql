-- Observable delivery state and bounded retries; no change to already sent rows.
ALTER TYPE broadcast_status ADD VALUE IF NOT EXISTS 'failed';
ALTER TABLE broadcasts ADD COLUMN last_error text;
ALTER TABLE broadcasts ADD COLUMN retry_at timestamptz;
ALTER TABLE broadcasts ADD COLUMN recipients_prepared boolean NOT NULL DEFAULT false;
UPDATE broadcasts b SET recipients_prepared=true WHERE EXISTS
    (SELECT 1 FROM broadcast_deliveries d WHERE d.broadcast_id=b.id);
ALTER TABLE broadcast_deliveries ADD COLUMN attempts integer NOT NULL DEFAULT 0;
CREATE TABLE service_heartbeats (
    service text PRIMARY KEY,
    last_seen_at timestamptz NOT NULL DEFAULT now(),
    details jsonb NOT NULL DEFAULT '{}'
);
