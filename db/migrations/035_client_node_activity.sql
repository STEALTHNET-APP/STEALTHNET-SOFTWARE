-- Latest nonzero traffic report, not a list of individual TCP connections.
CREATE TABLE client_node_activity (
    client_id bigint NOT NULL REFERENCES clients(id) ON DELETE CASCADE,
    node_id bigint NOT NULL REFERENCES nodes(id) ON DELETE CASCADE,
    last_seen_at timestamptz NOT NULL DEFAULT now(),
    upload_bytes bigint NOT NULL CHECK(upload_bytes >= 0),
    download_bytes bigint NOT NULL CHECK(download_bytes >= 0),
    PRIMARY KEY(client_id,node_id)
);
CREATE INDEX client_node_activity_recent_idx ON client_node_activity(last_seen_at DESC);
CREATE INDEX client_node_activity_node_idx ON client_node_activity(node_id,last_seen_at DESC);
