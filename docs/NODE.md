# Подключение VPN-ноды

Нода — это отдельный сервер, через который идёт трафик клиентов. На нём работают две вещи: движок (Xray) и агент (`sn-node`), который держит связь с панелью.

Порядок именно такой: сначала ключи и профиль в панели, потом сервер. Иначе нода получит конфиг без ключей и не поднимется.

## 1. Ключи Reality

Панель → **Профили конфигураций** → откройте профиль → **Ключи** → «Сгенерировать пару».

Панель сразу подписывает, куда что вставлять:

| Что | Куда |
|---|---|
| Приватный ключ | в конфиг профиля, `realitySettings.privateKey` |
| Публичный ключ | в хост, поле `public_key` — клиент получит его как `pbk` |
| Short ID | в `shortIds` профиля **и** в хост как `sid` |

Ключи генерирует панель, а не нода: только так она сможет собрать клиенту рабочую ссылку.

## 2. Профиль

Вставьте блок в конфиг и нажмите **Проверить**. Проверка идёт в два уровня: структурные правила и, если на сервере панели есть бинарь Xray, его собственный `-test`. Сохранить невалидный конфиг нельзя — иначе агенты заберут его и уронят все ноды профиля разом.

Минимальный рабочий конфиг, проверенный на живом сервере:

```json
{
  "log": { "loglevel": "warning" },
  "api": { "tag": "api", "services": ["StatsService"] },
  "stats": {},
  "policy": {
    "levels": { "0": { "statsUserUplink": true, "statsUserDownlink": true } },
    "system": { "statsInboundUplink": true, "statsInboundDownlink": true }
  },
  "inbounds": [
    {
      "tag": "api-in",
      "listen": "127.0.0.1",
      "port": 10085,
      "protocol": "dokodemo-door",
      "settings": { "address": "127.0.0.1" }
    },
    {
      "tag": "vless-reality",
      "listen": "0.0.0.0",
      "port": 443,
      "protocol": "vless",
      "settings": { "clients": [], "decryption": "none" },
      "streamSettings": {
        "network": "tcp",
        "security": "reality",
        "realitySettings": {
          "dest": "www.cloudflare.com:443",
          "serverNames": ["www.cloudflare.com"],
          "privateKey": "ВАШ_ПРИВАТНЫЙ_КЛЮЧ",
          "shortIds": ["ВАШ_SHORT_ID"]
        }
      },
      "sniffing": { "enabled": true, "destOverride": ["http", "tls"] }
    }
  ],
  "outbounds": [
    { "protocol": "freedom", "tag": "direct" },
    { "protocol": "blackhole", "tag": "block" }
  ],
  "routing": {
    "rules": [
      { "type": "field", "inboundTag": ["api-in"], "outboundTag": "api" },
      { "type": "field", "protocol": ["bittorrent"], "outboundTag": "block" }
    ]
  }
}
```

Два блока, которые кажутся необязательными, но нужны:

- **`api` + `stats` + `policy`** — без них Xray не считает трафик по клиентам, и лимиты работать не будут. Агент читает счётчики через `api-in` на 10085.
- **`api-in` слушает только 127.0.0.1** — наружу этот порт открывать нельзя ни в коем случае.

`clients` оставляйте пустым: агент подставит туда клиентов сам, ориентируясь на сквады. Если впишете вручную — при первой же синхронизации список перезапишется, панель об этом предупредит.

## 3. Нода и хост в панели

**Ноды** → «Подключить ноду». Панель выдаст секрет — он понадобится на сервере.

Затем **Хосты** → создайте хост:

- адрес и порт — как у ноды;
- `security` = reality, `sni` = тот же домен, что в `serverNames`;
- `public_key` и `short_id` — из шага 1.

Хост попадёт в подписку только если его инбаунд входит в сквад клиента **и** поднят на живой ноде. Пока нода в статусе `provisioning`, клиенты его не увидят — это защита от выдачи мёртвых локаций.

## 4. Сервер ноды

Нужен любой Linux с белым IP. Ставим движок:

```bash
ARCH=$(uname -m); case $ARCH in x86_64) A=64;; aarch64) A=arm64-v8a;; esac
VER=$(curl -s https://api.github.com/repos/XTLS/Xray-core/releases/latest \
      | grep -oP '"tag_name": "\K[^"]+')
curl -sLo /tmp/xray.zip \
  "https://github.com/XTLS/Xray-core/releases/download/$VER/Xray-linux-$A.zip"
unzip -oq /tmp/xray.zip -d /tmp/xray-dist
install -m755 /tmp/xray-dist/xray /usr/local/bin/xray
mkdir -p /usr/local/share/xray
install -m644 /tmp/xray-dist/geoip.dat /tmp/xray-dist/geosite.dat /usr/local/share/xray/
```

Агента возьмите готовым бинарём из релиза или соберите (`cargo build --release -p sn-node`) и положите в `/usr/local/bin/sn-node`.

Сервис:

```ini
[Unit]
Description=VPN node agent
After=network.target

[Service]
Environment=PANEL_URL=https://panel.example.com
Environment=NODE_SECRET=секрет_из_панели
Environment=ENGINE_BIN=/usr/local/bin/xray
Environment=ENGINE_CONFIG=/etc/sn-node/config.json
Environment=ENGINE_API=127.0.0.1:10085
Environment=XRAY_LOCATION_ASSET=/usr/local/share/xray
ExecStart=/usr/local/bin/sn-node
Restart=always
RestartSec=5

[Install]
WantedBy=multi-user.target
```

```bash
mkdir -p /etc/sn-node
systemctl daemon-reload && systemctl enable --now sn-node
journalctl -u sn-node -f
```

В логе должно появиться «получен новый конфиг» и «движок перезапущен». Конфиг агент пишет сам — руками его трогать не нужно, при следующей синхронизации правки затрутся.

## 5. Проверка

На сервере ноды:

```bash
ss -tlnp | grep -E ':443|:10085'          # оба порта слушаются
xray api statsquery --server=127.0.0.1:10085 -pattern "user>>>"
```

В панели нода должна стать `online`, показать версию движка и версию агента.

С любой машины — подключитесь по ссылке подписки и сверьте внешний адрес:

```bash
curl -s --socks5-hostname 127.0.0.1:10808 https://api.ipify.org
```

Должен вернуться IP ноды, а не ваш.

## Как это устроено внутри

Агент **сам стучится в панель**, панель к ноде не подключается. У edge-серверов часто нет белого IP или закрыт вход, а исходящее соединение есть всегда.

Конфиг передаётся, **только если сменилась версия** — иначе каждые 15 секунд гонялись бы килобайты JSON впустую. Версия растёт при каждом сохранении профиля.

Статистику агент читает командой `xray api statsquery --reset`: она сразу возвращает прирост с прошлого опроса. Это важно, потому что после перезапуска движка счётчики обнуляются, и абсолютные значения дали бы отрицательную разницу.

Панель принимает трафик по UUID, а движок отдаёт по `email` — агент сопоставляет их по таблице, полученной вместе с конфигом.

## Если что-то не работает

**Нода не выходит на связь.** Смотрите `journalctl -u sn-node`. «панель не признала секрет» — неверный `NODE_SECRET`. Панель недоступна — агент не падает и продолжает обслуживать клиентов на последнем конфиге; это задумано.

**Клиент не подключается.** Сверьте `public_key` в хосте с `privateKey` профиля: это должна быть одна пара. Проверьте, что `sni` хоста совпадает с `serverNames`, а `sid` — с `shortIds`.

**Трафик не считается.** Убедитесь, что в конфиге есть `api`, `stats` и `policy`, а `api-in` слушает 10085. Проверьте вручную: `xray api statsquery --server=127.0.0.1:10085 -pattern "user>>>"`.

**Клиента нет в конфиге ноды.** Он попадёт туда, только если подписка активна по дате и трафику, а его сквад включает инбаунд этой ноды.
