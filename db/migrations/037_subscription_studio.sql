-- Multiple named templates per format, without changing existing assignments.
ALTER TABLE subscription_templates DROP CONSTRAINT subscription_templates_code_key;
ALTER TABLE subscription_templates ADD COLUMN is_default boolean NOT NULL DEFAULT false;
CREATE UNIQUE INDEX subscription_templates_name_idx ON subscription_templates(code,title);
CREATE UNIQUE INDEX subscription_templates_default_idx ON subscription_templates(code) WHERE is_default;

-- NULL conditions preserve the legacy substring rules exactly.
ALTER TABLE response_rules ADD COLUMN conditions jsonb;
ALTER TABLE response_rules ADD COLUMN operator text NOT NULL DEFAULT 'AND' CHECK (operator IN ('AND','OR'));
ALTER TABLE response_rules ADD COLUMN description text NOT NULL DEFAULT '';
ALTER TABLE response_rules ADD COLUMN response_headers jsonb NOT NULL DEFAULT '[]';
ALTER TABLE response_rules ADD COLUMN disable_hwid_check boolean NOT NULL DEFAULT false;
