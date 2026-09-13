-- Presentation/connection overrides are independent of server access grants.
ALTER TABLE hosts ADD COLUMN options jsonb NOT NULL DEFAULT '{}'::jsonb
    CHECK (jsonb_typeof(options) = 'object');
ALTER TABLE external_squads ADD COLUMN subscription_settings jsonb NOT NULL DEFAULT '{}'::jsonb
    CHECK (jsonb_typeof(subscription_settings) = 'object');
ALTER TABLE external_squads ADD COLUMN template_overrides jsonb NOT NULL DEFAULT '{}'::jsonb
    CHECK (jsonb_typeof(template_overrides) = 'object');
ALTER TABLE external_squads ADD COLUMN host_overrides jsonb NOT NULL DEFAULT '{}'::jsonb
    CHECK (jsonb_typeof(host_overrides) = 'object');
ALTER TABLE clients ADD COLUMN external_squad_id bigint REFERENCES external_squads(id) ON DELETE SET NULL;
CREATE INDEX clients_external_squad_idx ON clients(external_squad_id) WHERE deleted_at IS NULL;
