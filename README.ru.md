<p align="center"><a href="README.ru.md"><b>Русский</b></a> · <a href="README.md">English</a></p>

![STEALTHNET — ваш VPN-сервис](docs/media/cover-ru.svg)

<p align="center">
  <a href="docs/ru/installation.md"><img src="docs/media/navigation/button-install-ru.svg" width="180" alt="Установить" /></a>
  <a href="docs/ru/README.md"><img src="docs/media/navigation/button-docs-ru.svg" width="180" alt="Документация" /></a>
  <a href="https://t.me/stealthnet_admin_panel"><img src="docs/media/navigation/button-community-ru.svg" width="180" alt="Сообщество" /></a>
  <a href="#support-project"><img src="docs/media/navigation/button-donate-ru.svg" width="180" alt="Поддержать" /></a>
</p>

**STEALTHNET** — платформа для своего VPN-сервиса: инфраструктура, подписки, продажи и клиентские аккаунты в одной системе. Rust и PostgreSQL, независимые службы, интерфейсы на русском и английском.

**v0.1.0 · первый публичный выпуск.** [Скачать сборки Linux](https://github.com/STEALTHNET-APP/STEALTHNET-SOFTWARE/releases/tag/v0.1.0) · [Совместимость и область проверок](docs/compatibility.md).

## Один сервис — три интерфейса

| Панель владельца | Кабинет и Mini App | Инфраструктура |
|---|---|---|
| Клиенты, тарифы, оплаты, поддержка и уведомления | Витрина, вход по коду, покупка, устройства и трафик | Профили, ноды, хосты, сквады и выдача подписок |
| Версии, диагностика, установка и брендинг | RU/EN, светлая/тёмная тема, единый аккаунт с Telegram | Агент Xray, контроль доступа и сведения bgp.tools |

## Галерея интерфейсов

Нажмите на скриншот, чтобы открыть его в полном размере.

<table>
<tr>
<td width="50%" valign="top"><h3>Панель владельца</h3><a href="docs/media/panel-nodes-ru.png"><img src="docs/media/panel-nodes-ru.png" width="640" alt="Панель владельца" /></a><p>Ноды, диагностика и инфраструктура</p></td>
<td width="50%" valign="top"><h3>Сайт и витрина</h3><a href="docs/media/storefront-ru.png"><img src="docs/media/storefront-ru.png" width="640" alt="Сайт и витрина" /></a><p>Описание сервиса и тарифы до входа</p></td>
</tr>
<tr>
<td width="50%" valign="top"><h3>Кабинет клиента · ПК</h3><a href="docs/media/cabinet-ru.png"><img src="docs/media/cabinet-ru.png" width="640" alt="Кабинет клиента на компьютере" /></a><p>Подписка, покупки, устройства и трафик</p></td>
<td width="50%" valign="top" align="center"><h3>Mini App · Telegram</h3><a href="docs/media/miniapp-ru.png"><img src="docs/media/miniapp-ru.png" width="160" alt="STEALTHNET Mini App" /></a><p>Тот же аккаунт и подписка внутри Telegram</p></td>
</tr>
</table>

[О скриншотах](docs/media/README.md): настоящий интерфейс в тестовом окружении, с демонстрационными данными.

## Что включено

- **Продажи:** тарифы и периоды, промокоды, партнёрская программа; Platega, RollyPay, ParityPay v2, Stars, CryptoBot и ручная оплата.
- **Докупки:** отдельные пакеты и цены у каждого тарифа. Устройства — до конца оплаченного срока; трафик — до сброса или окончания подписки.
- **Доступ:** внутренние сквады управляют инбаундами; внешний сквад переопределяет шаблоны и настройки выдачи.
- **Подключение:** страница подписки, приложения, инструкции и QR; выдача Xray JSON, share-links, Clash/Mihomo/Stash и sing-box.
- **Клиентский сайт:** витрина до входа, постоянный код доступа, привязка Telegram, покупки и поддержка. Кабинет устанавливается отдельно; Mini App работает через него.
- **Брендинг:** лого для обеих тем, favicon, цвета, SEO, тексты RU/EN, FAQ и инструкции из админки.
- **Эксплуатация:** установка из релизов, `make update`, резервная копия перед обновлением, проверка версий, журналы и уведомления команды.

## Установка

Поддерживаемые цели: **Debian 12/13**, **Ubuntu 22.04/24.04/26.04 LTS**, **amd64/arm64**. Для новых Debian 13, Ubuntu 24.04 и 26.04 выполнены чистая установка и обновление на amd64; ARM-бинарники собраны, отдельная ARM-VM проверка ещё не выполнена.

[Открыть пошаговую установку](docs/ru/installation.md) — требования, DNS, мастер, HTTPS и диагностика. [Ноды](docs/ru/node-installation.md), [подписка](docs/ru/subscription-installation.md) и [кабинет](docs/ru/cabinet-installation.md) имеют отдельные инструкции, включая один или разные серверы.

На установленной панели:

```bash
cd /opt/stealthnet-software
make update
```

Обновление скачивает опубликованный релиз, проверяет SHA256 и создаёт резервную копию. Возврат бинарников не отменяет миграции базы. [Обновление и восстановление](docs/ru/backup-restore.md).

## Документация по задачам

<table>
<tr>
<td width="50%"><a href="docs/ru/installation.md"><img src="docs/media/navigation/install-ru.svg" width="640" alt="Установка" /></a></td>
<td width="50%"><a href="docs/ru/README.md"><img src="docs/media/navigation/guide-ru.svg" width="640" alt="Работа с панелью" /></a></td>
</tr>
<tr>
<td width="50%"><a href="docs/ru/profiles.md"><img src="docs/media/navigation/profiles-ru.svg" width="640" alt="Профили и конфиги" /></a></td>
<td width="50%"><a href="docs/ru/subscription-installation.md"><img src="docs/media/navigation/subscription-ru.svg" width="640" alt="Страница подписки" /></a></td>
</tr>
<tr>
<td width="50%"><a href="docs/ru/cabinet-installation.md"><img src="docs/media/navigation/cabinet-ru.svg" width="640" alt="Кабинет и Mini App" /></a></td>
<td width="50%"><a href="docs/ru/backup-restore.md"><img src="docs/media/navigation/operations-ru.svg" width="640" alt="Обновление и копии" /></a></td>
</tr>
</table>

**[Все 34 раздела панели](docs/ru/README.md)** · [Платежи](docs/ru/payment-gateways.md) · [Докупки](docs/ru/addons.md) · [Брендинг и языки](docs/ru/branding.md) · [Уведомления](docs/ru/team-notifications.md) · [Диагностика](docs/ru/troubleshooting.md)

## Архитектура

```mermaid
flowchart LR
  Admin[Администратор] --> API[API панели]
  API --> DB[(PostgreSQL)]
  Bot[Telegram-бот] --> DB
  Worker[Фоновые задачи] --> DB
  Site[Сайт и Mini App] --> Gateway[Шлюз кабинета]
  Gateway --> API
  Node[Агент ноды / Xray] --> API
  Sub[Сервис подписок] --> API
  Client[VPN-клиент] --> Sub
  Client --> Node
```

При совместной установке сервис подписок может работать с локальной базой. У отдельного кабинета и сервиса подписок в режиме API нет доступа к PostgreSQL.

## Разработка и лицензия

[Архитектура и разработка](docs/ru/development.md) · [Участие](CONTRIBUTING.ru.md) · [Сообщить об уязвимости](SECURITY.md) · [Сторонние компоненты](THIRD_PARTY_NOTICES.md)

Проект: **AGPL-3.0-only**. Полный текст в [LICENSE](LICENSE). Лицензии зависимостей и шрифтов сохраняются отдельно.

<a id="support-project"></a>

## Сообщество и поддержка проекта

[Новости, обсуждения и помощь · @stealthnet_admin_panel](https://t.me/stealthnet_admin_panel)

Добровольный донат на развитие STEALTHNET. **Сеть: TRON · TRC20.**

```text
THQA9Qnx87NcHAwYrcCTBGSi6BhY72LXEZ
```
