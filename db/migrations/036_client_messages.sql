-- One idempotency key per explicit send from a client card.
ALTER TABLE ticket_messages ADD COLUMN request_id uuid;
ALTER TABLE ticket_messages ADD COLUMN delivery_error text;
CREATE UNIQUE INDEX ticket_messages_request_idx ON ticket_messages(request_id) WHERE request_id IS NOT NULL;
