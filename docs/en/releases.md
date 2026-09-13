[All guides](README.md) · [Русский](../ru/releases.md) / [English](../en/releases.md)

# Build and publish a release

The repository is STEALTHNET-APP/STEALTHNET-SOFTWARE. The public installer selects the latest published stable GitHub Release unless the operator specifies a tag.

## Build workflow

`checks.yml` validates pushes and pull requests. `release.yml` runs for a `vX.Y.Z` tag or manually for an existing tag matching the Cargo workspace version exactly. It builds native amd64 and arm64 Linux binaries on Ubuntu 22.04 runners, with Rust 1.98.1 and Cargo.lock. The glibc baseline is 2.35; changing the build OS can change runtime compatibility.

The archive contains seven executables, panel assets, all migrations, install/update tools, docs and checksums. Node, subscription and customer-gateway binaries for **both** architectures are published by each panel release. The packager rejects wrong-architecture/non-Linux binaries.

Private environments, database dumps, audit sessions, conversations, SSH keys, target directories and demo database seeds are not release contents. Keep private files out of the public web directory.

## Publish

1. Set `[workspace.package].version` in Cargo.toml, then run `python3 devtools/release_version.py --sync`. It synchronizes workspace packages in Cargo.lock, current installation/update commands in all guides, README version labels and download buttons. Review the diff and write RU/EN release notes, including migration compatibility and required operator actions.
2. Run tests and verify clean installation plus an upgrade from the previous release.
3. Create and push the matching version tag after repository publication is authorized.
4. Wait for both native builds. The workflow creates a **draft** release with archives and SHA256 files.
5. Review contents and notes, then publish it as stable. Only then can customers install it and see it in update checks.

Existing release assets are never overwritten. Fixes require another version and tag. A workflow rerun stops if the release already exists.

`python3 devtools/release_version.py --check` fails if release references differ from Cargo.toml. It runs as part of `make test-release` in both change checks and release builds. Historical compatibility reports, third-party dependency versions and descriptions of earlier fixes retain their original versions. Keep new historical reports out of current installation guides and explicitly register them in the version tool's history list.

```text
stealthnet-vX.Y.Z-linux-amd64.tar.gz
stealthnet-vX.Y.Z-linux-amd64.tar.gz.sha256
stealthnet-vX.Y.Z-linux-arm64.tar.gz
stealthnet-vX.Y.Z-linux-arm64.tar.gz.sha256
sn-{node,sub,cabinet}-linux-{amd64,arm64}
sn-{node,sub,cabinet}-linux-{amd64,arm64}.sha256
SHA256SUMS
```

The `.sha256` sidecar contains the hash alone. `SHA256SUMS` is the multi-file format for `sha256sum -c`. HTTPS GitHub and repository publishing permissions are the trust boundary; checksums do not independently authenticate a malicious repository owner.

## Maintainer checks

```bash
make test-release
cargo test --locked --workspace --lib --bins
python3 deploy/package-release.py --version v0.1.9 --builds /path/to/builds --output /path/to/dist
```

Each architecture directory must contain all seven real Linux binaries. Pin action commits, protect the main branch/tags, require checks and use 2FA for maintainers. Keep write permissions restricted to the release job. See [installation test coverage](../compatibility.md) for what has actually been exercised.
