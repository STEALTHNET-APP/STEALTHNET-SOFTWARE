[All guides](README.md) · [Русский](../ru/development.md) / [English](../en/development.md)

# Development and architecture

The project is a Rust workspace backed by PostgreSQL. The admin UI uses JavaScript/HTML/CSS without a frontend bundler. Customer assets are embedded in sn-cabinet at compile time; changing web/app requires rebuilding the installed gateway.

| Directory | Purpose |
|---|---|
| crates/core | Database, authentication, shared rules and config validation |
| crates/api | Panel API, sn-admin and sn-cabinet gateway |
| crates/sub | Subscription formats and connection page |
| crates/payments | Providers, invoices, payment application and add-ons |
| crates/bot | Telegram bot |
| crates/node | Server agent and Xray integration |
| crates/worker | Background jobs, broadcasts and notifications |
| web | Admin and customer interfaces |
| db/migrations | Ordered PostgreSQL migrations |

## Local development

Use a separate database and configuration based on .env.example. Run `bash deploy/migrate.sh` with DATABASE_URL configured. Never seed a production database. Build with `cargo build --locked --workspace`. Create an owner through sn-admin using password stdin, not a password argument.

```bash
make test-release
cargo test --locked --workspace --lib --bins
cargo build --release --locked --workspace
```

Database/Telegram integration tests use separate variables and test-database restrictions. Do not point them at production. Use the [release workflow](releases.md) for Linux packages, architecture checks and manifests.

## Payment extensions

Configure the generic HTTP provider when the provider matches its supported contract. Nonstandard signatures or multi-stage APIs require a compiled Rust module. Implement the provider-registry contract; validate signatures on original request bytes, external transaction identity, amount and currency. Applying payment and marking it successful must be transactional and idempotent.

The registry, provider trait and existing adapters are in crates/payments. Test valid/invalid signatures, duplicate callbacks, mismatched amounts/currencies, out-of-order statuses and refunds before enabling a module.

## Changes and translations

Add new numbered migrations; do not edit migrations already released. UI changes require matching RU/EN strings, help and section documentation. Check mobile/desktop and light/dark themes. Describe proposed features as proposals rather than completed functionality.

See [CONTRIBUTING.md](../../CONTRIBUTING.md).
