-- Login still uses the hash. Authenticated reveal uses an encrypted copy;
-- its independent key lives in the panel environment, never in this database.
ALTER TABLE cabinet_credentials ADD COLUMN code_sealed bytea;
