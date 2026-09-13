-- Empty rate limit in the external squad form means unlimited.
ALTER TABLE external_squads ALTER COLUMN rate_limit_per_min DROP NOT NULL;
