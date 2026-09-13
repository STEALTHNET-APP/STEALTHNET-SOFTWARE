[All guides](README.md) · [Русский](../ru/branding.md) / [English](../en/branding.md)

# Branding, languages and SEO

Open **Customer website & Mini App**. The customer website, Mini App and connection page share branding; the administrator navigation is a separate interface.

## Configure

1. Enter the service name, HTTPS light/dark logo URLs and favicon. Assets must be accessible without authentication.
2. Select primary and secondary colors. Check button text contrast in both themes.
3. Enter storefront headings/descriptions, section titles, connection steps, FAQ, documents and support details.
4. Enter English copy, SEO and instructions under English. Plan English names/descriptions are configured per plan.
5. Save, reload the form and check persisted values.
6. Open the public storefront on desktop and phone, then check the account and Mini App. Switch RU/EN and light/dark themes.

## Language behavior

An explicit `?lang=ru` or `?lang=en` takes precedence, followed by the saved preference, Telegram language and browser language. Preferences persist on the current domain. Interface translations are separate from owner content; service names and customer data are not automatically translated.

Complete the required English fields before publishing EN. Missing English content produces an explicit unpublished-content message; the site does not invent marketing copy or prices. Saving English content must preserve the Russian version.

## SEO and images

Check document title, description, favicon and language links. The gateway renders canonical/hreflang metadata and a sitemap for published content. Private account data is not part of the storefront.

Use your own brand assets. Public screenshots must not contain access codes, full subscription URLs, payment keys, customer IPs or conversations.

## What updates preserve

`make update` changes release code and applies new migrations once. Customers, subscriptions, payments, plans, profiles, branding and texts live in PostgreSQL; updates do not run `db/seed.sql` or recreate the owner. The `.env` file and `CABINET_CODE_KEY` remain unchanged. Updates do not rewrite the existing reverse proxy configuration or systemd customizations.

Store your images in `/opt/stealthnet-software/shared/public/`, for example `logo.svg`. With the standard Caddy installation, use `https://panel.example.com/custom/logo.svg` in admin branding settings. Releases never replace `shared/`, and the update backup includes it. This directory is public: never put keys or configuration files there. Configure an equivalent `/custom/` route when using an external proxy.

`releases/` and `current/web` contain versioned application code; do not put operator files or manual edits there. Editing release source/CSS is different from changing branding settings. Previous releases stay on disk; failed readiness returns to the previous binaries. Reverting binaries does not undo applied migrations; data rollback uses the saved database dump.
