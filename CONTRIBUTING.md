[Русский](CONTRIBUTING.ru.md) · **English**

# Contributing to STEALTHNET

Start with [development](docs/en/development.md), the [section guides](docs/en/README.md) and [LICENSE](LICENSE). Submit changes you have the right to contribute under the project license. Do not copy another project's code, artwork or documentation without checking its license and preserving required notices. Referencing a protocol or comparing product behavior is different from copying an implementation.

## Before making a change

Describe the concrete problem and expected result. For a larger change, discuss data compatibility, migrations and user workflow before replacing existing behavior. Keep new customer-facing features configurable through the admin panel when appropriate. Do not add sample customers, payment keys or fabricated production metrics as fallbacks.

## A reviewable change

- Keep the change focused; explain behavior before and after it.
- Add a new migration rather than editing a published migration.
- Preserve existing owners' settings and protect backward compatibility during updates.
- Include RU/EN labels, hints, help and matching documentation pages.
- Check mobile/desktop and light/dark UI when the change affects layout.
- Test the actual failure or feature boundary; do not send test payments/messages to real customers.

Run checks appropriate to the change:

```bash
make test-release
python3 devtools/check-docs.py
cargo test --locked --workspace --lib --bins
```

Describe completed validation and anything not exercised. Installation changes also need real systemd/PostgreSQL VM checks; payment changes need duplicate/out-of-order callback and signature checks in an isolated environment.

## Pull requests

Explain the problem, resulting behavior, testing and operator actions. Include anonymized screenshots for UI changes. Do not include .env files, customer data, private audit notes, access codes, payment credentials or local build outputs. Security findings follow [SECURITY.md](SECURITY.md).

GitHub publication and the first stable release are pending. Preparing a change locally does not imply that the public repository already contains it.
