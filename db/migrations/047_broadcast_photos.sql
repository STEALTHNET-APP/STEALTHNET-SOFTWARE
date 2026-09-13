-- Keep attachments in PostgreSQL: API and worker share storage and backups.
CREATE TABLE broadcast_media (
    id uuid PRIMARY KEY,
    content_type text NOT NULL CHECK (content_type IN ('image/jpeg', 'image/png')),
    data bytea NOT NULL CHECK (octet_length(data) BETWEEN 1 AND 10485760),
    width integer NOT NULL CHECK (width > 0),
    height integer NOT NULL CHECK (height > 0),
    created_by bigint REFERENCES admins(id) ON DELETE SET NULL,
    created_at timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX broadcast_media_created ON broadcast_media(created_at);
ALTER TABLE broadcasts ADD COLUMN photo_id uuid REFERENCES broadcast_media(id);
CREATE INDEX broadcasts_photo ON broadcasts(photo_id) WHERE photo_id IS NOT NULL;
