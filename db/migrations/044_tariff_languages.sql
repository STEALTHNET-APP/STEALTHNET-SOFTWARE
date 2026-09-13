-- Customer-facing plan content; prices and entitlements are shared by languages.
ALTER TABLE tariffs ADD COLUMN IF NOT EXISTS locales jsonb NOT NULL DEFAULT '{}'::jsonb;
ALTER TABLE tariffs ADD CONSTRAINT tariff_locales_object CHECK (jsonb_typeof(locales) = 'object');
