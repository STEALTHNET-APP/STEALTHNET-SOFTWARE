# Compatibility / Совместимость

[English docs](en/README.md) · [Русская документация](ru/README.md)

## Verified clean-system runs / Проверки на чистых системах

2026-09-13 · development artifacts before publication · isolated KVM guests · real systemd, APT, PostgreSQL and Caddy. / Локальные релизные пакеты, изолированные VM, настоящие службы и зависимости.

| OS / ОС | Architecture | Install / Установка | Update / Обновление | PostgreSQL | Migrations |
|---|---|---|---|---|---|
| Debian 13 | amd64 | PASS | PASS | 17.11 | 44 |
| Ubuntu 24.04 LTS | amd64 | PASS | PASS | 16.15 | 44 |
| Ubuntu 26.04 LTS | amd64 | PASS | PASS | 18.6 | 44 |

Each run checked automatic dependency installation, the application/database connection, subscription readiness, Caddy HTTPS with a trusted local CA, retention of an unrelated Caddy site, private environment permissions, backup creation and database/secret preservation after an update. All 44 migrations applied.

В каждом прогоне проверены установка зависимостей, API/база, готовность подписок, HTTPS Caddy с доверенным локальным CA, сохранение постороннего сайта Caddy, права окружения, резервная копия, сохранение базы и секретов при обновлении. Применены все 44 миграции.

## Scope and remaining checks / Границы проверки

- Installer targets also include Debian 12 and Ubuntu 22.04 LTS. The current clean-system matrix above specifically records the newly requested systems. / Установщик также поддерживает Debian 12 и Ubuntu 22.04; таблица фиксирует отдельный прогон новых ОС.
- amd64 and arm64 binaries were built and checked as Linux ELF. ARM VM installation has not yet been run. / Бинарники обеих архитектур собраны; установка в ARM-VM ещё не проверена.
- Upgrade tests used a synthetic v0.1.2 package label for the same development source. That test label predates the public v0.1.2 release and does not verify later schema changes. / Тестовый тег v0.1.2 использован только для проверки механизма обновления; он предшествовал публичному v0.1.2 и не проверял более поздние миграции.
- VM HTTPS used Caddy's trusted internal CA without disabling certificate verification. Public ACME issuance was not exercised by these VM tests. / Публичный выпуск ACME-сертификата не является частью этих VM-проверок.
- Published v0.1.1 was installed through `git clone` and the public GitHub release on a clean Ubuntu 26.04 amd64 VM: all 45 migrations, dependencies, API, subscriptions and HTTPS passed. `make update VERSION=v0.1.1` passed. A synthetic next version with an additional migration preserved the database, credentials, branding, custom logo and an unrelated Caddy site; its backup was checked. / Публичный v0.1.1 проверен через `git clone` на чистой Ubuntu 26.04 amd64: установка, 45 миграций, зависимости, API, подписки и HTTPS работают. Проверены `make update VERSION=v0.1.1` и переход на тестовую следующую версию с миграцией, сохранением базы, ключей, брендинга, логотипа и постороннего сайта Caddy, включая резервную копию.

## Image sources / Источники образов

Official images downloaded over HTTPS and verified against the publisher's SHA256/SHA512 files: [Debian 13](https://cloud.debian.org/images/cloud/trixie/latest/), [Ubuntu 24.04](https://cloud-images.ubuntu.com/releases/24.04/release/), [Ubuntu 26.04](https://cloud-images.ubuntu.com/releases/26.04/release/).

Официальные образы проверены по опубликованным контрольным суммам. Это проверка конкретной сборки и сценариев, а не гарантия отсутствия любых ошибок или уязвимостей.
