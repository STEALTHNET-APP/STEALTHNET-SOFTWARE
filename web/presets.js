/* Готовые конфигурации и блоки маршрутизации.
 *
 * Смысл прост: чистый лист — плохая отправная точка. Конфиг Xray легко
 * написать так, что он загрузится и не будет работать, а разница между
 * рабочим и нерабочим — в паре полей, о которых нужно знать заранее.
 * Здесь лежат заготовки, которые собираются одним нажатием, а дальше
 * правятся под себя.
 *
 * Что общего у всех заготовок:
 *  - `api` + `stats` + `policy` — без них не считается трафик клиентов,
 *    а значит не работают лимиты в тарифах;
 *  - инбаунд `api-in` слушает только 127.0.0.1 — наружу его открывать
 *    нельзя, через него агент читает счётчики;
 *  - `settings.clients` всегда пуст: клиентов вписывает агент на ноде,
 *    один профиль обслуживает много нод с разным составом клиентов;
 *  - `sniffing` включён — без него маршрутизация по доменам не видит,
 *    куда идёт соединение, и правила geosite не срабатывают.
 *
 * Плейсхолдеры заглавными (ВАШ_ПРИВАТНЫЙ_КЛЮЧ, ВАШ_ДОМЕН) нарочно
 * бросаются в глаза: конфиг с ними не пройдёт проверку, и о забытом
 * поле станет известно сразу, а не от клиентов.
 */
'use strict';

/* Служебная часть, одинаковая для всех заготовок. */
const PRESET_BASE = {
  log: { loglevel: 'warning' },
  api: { tag: 'api', services: ['StatsService'] },
  stats: {},
  policy: {
    levels: { 0: { statsUserUplink: true, statsUserDownlink: true } },
    system: { statsInboundUplink: true, statsInboundDownlink: true },
  },
};

const API_INBOUND = {
  tag: 'api-in',
  listen: '127.0.0.1',
  port: 10085,
  protocol: 'dokodemo-door',
  settings: { address: '127.0.0.1' },
};

const SNIFF = { enabled: true, destOverride: ['http', 'tls', 'quic'] };

/* Исходящие и маршрутизация по умолчанию.
   Приватные сети перечислены списком, а не `geoip:private`: файл
   geoip.dat на ноде может оказаться старым или отсутствовать, и тогда
   правило со ссылкой на него роняет весь конфиг. */
const BASE_OUT = {
  outbounds: [
    { tag: 'direct', protocol: 'freedom' },
    { tag: 'block', protocol: 'blackhole' },
  ],
  routing: {
    domainStrategy: 'IPIfNonMatch',
    rules: [
      { type: 'field', inboundTag: ['api-in'], outboundTag: 'api' },
      { type: 'field', protocol: ['bittorrent'], outboundTag: 'block' },
      {
        type: 'field',
        outboundTag: 'block',
        ip: ['10.0.0.0/8', '172.16.0.0/12', '192.168.0.0/16', '127.0.0.0/8', '169.254.0.0/16'],
      },
    ],
  },
};

const build = (inbounds) => JSON.stringify(
  { ...PRESET_BASE, inbounds: [API_INBOUND, ...inbounds], ...BASE_OUT },
  null, 2,
);

/* ── заготовки профилей ── */
const PROFILE_PRESETS = [
  {
    id: 'reality-tcp',
    setup: [
      { t: "Сгенерируйте ключи Reality",
        d: "Кнопка «Ключи Reality» в редакторе профиля. Приватный ключ идёт в <code>privateKey</code>, публичный понадобится клиентам — панель подставит его сама." },
      { t: "Задайте короткий идентификатор",
        d: "<code>shortIds</code> — от 2 до 16 шестнадцатеричных символов. Та же кнопка предложит готовый." },
      { t: "Выберите сайт для маскировки",
        d: "<code>dest</code> и <code>serverNames</code> — чужой сайт с TLS 1.3 и HTTP/2, который отвечает из страны вашей ноды. Он должен быть доступен с ноды: движок ходит туда за рукопожатием." },
      { t: "Сертификат не нужен",
        d: "Reality прикрывается сертификатом того самого чужого сайта. Свой домен тоже не требуется." },
    ],
    name: 'VLESS + Reality (TCP)',
    tag: 'рекомендуется',
    desc: 'Основной вариант: маскируется под чужой сайт, не требует своего домена и сертификата.',
    note: 'Ключи возьмите в генераторе на странице профиля. dest и serverNames — сайт, под который маскируемся: он должен поддерживать TLS 1.3 и быть недалеко от ноды.',
    config: build([{
      tag: 'vless-reality',
      listen: '0.0.0.0',
      port: 443,
      protocol: 'vless',
      settings: { clients: [], decryption: 'none' },
      streamSettings: {
        network: 'tcp',
        security: 'reality',
        realitySettings: {
          dest: 'www.cloudflare.com:443',
          serverNames: ['www.cloudflare.com'],
          privateKey: 'ВАШ_ПРИВАТНЫЙ_КЛЮЧ',
          shortIds: ['ВАШ_SHORT_ID'],
        },
      },
      sniffing: SNIFF,
    }]),
  },
  {
    id: 'reality-grpc',
    setup: [
      { t: "Всё то же, что у Reality на TCP",
        d: "Ключи, короткий идентификатор и сайт для маскировки задаются одинаково." },
      { t: "Проверьте имя сервиса",
        d: "<code>serviceName</code> — произвольная строка, но у клиента она должна совпадать с серверной. Панель подставляет её в подписку сама." },
    ],
    name: 'VLESS + Reality (gRPC)',
    desc: 'Тот же Reality, но поверх gRPC — иногда проходит там, где режут обычный TCP.',
    note: 'serviceName должен совпадать у сервера и клиента; панель подставит его в подписку сама.',
    config: build([{
      tag: 'vless-reality-grpc',
      listen: '0.0.0.0',
      port: 443,
      protocol: 'vless',
      settings: { clients: [], decryption: 'none' },
      streamSettings: {
        network: 'grpc',
        security: 'reality',
        grpcSettings: { serviceName: 'grpc', multiMode: false },
        realitySettings: {
          dest: 'www.cloudflare.com:443',
          serverNames: ['www.cloudflare.com'],
          privateKey: 'ВАШ_ПРИВАТНЫЙ_КЛЮЧ',
          shortIds: ['ВАШ_SHORT_ID'],
        },
      },
      sniffing: SNIFF,
    }]),
  },
  {
    id: 'vless-ws-tls',
    setup: [
      { t: "Нужен обратный прокси",
        d: "Заготовка рассчитана на nginx или Caddy перед движком: он держит TLS, движок слушает локально. Без прокси клиент придёт по обычному HTTP, и соединение не поднимется." },
      { t: "Согласуйте путь",
        d: "<code>path</code> в конфиге и <code>location</code> у прокси должны совпадать буква в букву." },
      { t: "Проброс заголовков",
        d: "Прокси обязан передавать <code>Upgrade</code> и <code>Connection</code>, иначе веб-сокет не установится." },
    ],
    name: 'VLESS + WebSocket + TLS',
    desc: 'Для работы за обратным прокси или CDN: трафик выглядит обычным веб-сокетом.',
    note: 'Сертификат выдаёт веб-сервер перед Xray (Caddy, nginx), поэтому здесь TLS без сертификатов и порт внутренний. Наружу смотрит прокси.',
    config: build([{
      tag: 'vless-ws',
      listen: '127.0.0.1',
      port: 8443,
      protocol: 'vless',
      settings: { clients: [], decryption: 'none' },
      streamSettings: {
        network: 'ws',
        security: 'none',
        wsSettings: { path: '/ВАШ_ПУТЬ' },
      },
      sniffing: SNIFF,
    }]),
  },
  {
    id: 'vless-xhttp',
    setup: [
      { t: "Тот же обратный прокси",
        d: "XHTTP живёт поверх обычного HTTP, поэтому TLS держит прокси перед движком." },
      { t: "Путь и режим",
        d: "<code>path</code> должен совпадать с прокси. Режим <code>auto</code> подходит почти всегда; менять его стоит, только если провайдер режет длинные соединения." },
    ],
    name: 'VLESS + XHTTP + TLS',
    desc: 'Транспорт поверх обычных HTTP-запросов. Живуч там, где веб-сокеты обрывают.',
    note: 'Тоже рассчитан на веб-сервер перед Xray. Путь придумайте свой и не используйте короткий.',
    config: build([{
      tag: 'vless-xhttp',
      listen: '127.0.0.1',
      port: 8444,
      protocol: 'vless',
      settings: { clients: [], decryption: 'none' },
      streamSettings: {
        network: 'xhttp',
        security: 'none',
        xhttpSettings: { path: '/ВАШ_ПУТЬ', mode: 'auto' },
      },
      sniffing: SNIFF,
    }]),
  },
  {
    id: 'vless-httpupgrade',
    setup: [
      { t: "Обратный прокси с поддержкой Upgrade",
        d: "Схема как у веб-сокета, но легче. Прокси должен пропускать заголовок <code>Upgrade</code>." },
      { t: "Согласуйте путь",
        d: "<code>path</code> в конфиге и у прокси — одна и та же строка." },
    ],
    name: 'VLESS + HTTPUpgrade + TLS',
    desc: 'Промежуточный вариант между WS и XHTTP: легче веб-сокета, проходит через большинство прокси.',
    note: 'Ставится за веб-сервером так же, как WS.',
    config: build([{
      tag: 'vless-hu',
      listen: '127.0.0.1',
      port: 8445,
      protocol: 'vless',
      settings: { clients: [], decryption: 'none' },
      streamSettings: {
        network: 'httpupgrade',
        security: 'none',
        httpupgradeSettings: { path: '/ВАШ_ПУТЬ' },
      },
      sniffing: SNIFF,
    }]),
  },
  {
    id: 'vless-mkcp',
    setup: [
      { t: "Откройте порт UDP",
        d: "mKCP работает поверх UDP, а не TCP. Убедитесь, что порт открыт в брандмауэре именно для UDP — это самая частая причина «настроил, но не подключается»." },
      { t: "Учтите расход трафика",
        d: "mKCP шлёт избыточные пакеты ради устойчивости: на плохой линии он выигрывает в скорости, но тратит заметно больше трафика." },
    ],
    name: 'VLESS + mKCP (UDP)',
    desc: 'Поверх UDP. Иногда проходит там, где TCP душат, и лучше держится на плохой линии.',
    note: 'Порт нужно открыть по UDP, а не по TCP. Обфускацию заголовком и seed из движка убрали — настраивать здесь нечего.',
    config: build([{
      tag: 'vless-mkcp',
      listen: '0.0.0.0',
      port: 2408,
      protocol: 'vless',
      settings: { clients: [], decryption: 'none' },
      streamSettings: {
        network: 'kcp',
        security: 'none',
        kcpSettings: { mtu: 1350, tti: 50 },
      },
      sniffing: SNIFF,
    }]),
  },
  {
    id: 'vmess-ws',
    setup: [
      { t: "Берите только для старых клиентов",
        d: "VMess нужен там, где приложение не умеет VLESS. В остальном VLESS проще и быстрее." },
      { t: "Обратный прокси и путь",
        d: "Как у VLESS + WebSocket: TLS держит прокси, <code>path</code> должен совпадать." },
    ],
    name: 'VMess + WebSocket',
    desc: 'Старый протокол. Нужен, если у клиентов приложения, не умеющие VLESS.',
    note: 'VMess шифрует служебные поля сам, поэтому работает и без TLS — но за прокси с сертификатом надёжнее.',
    config: build([{
      tag: 'vmess-ws',
      listen: '127.0.0.1',
      port: 8446,
      protocol: 'vmess',
      settings: { clients: [] },
      streamSettings: {
        network: 'ws',
        security: 'none',
        wsSettings: { path: '/ВАШ_ПУТЬ' },
      },
      sniffing: SNIFF,
    }]),
  },
  {
    id: 'trojan-tcp',
    setup: [
      { t: "Нужен свой домен",
        d: "Домен с A-записью на эту ноду. Trojan притворяется обычным сайтом, и без домена смысла в нём нет." },
      { t: "Выпустите сертификат на ноде",
        d: "Установщик этого не делает: <code>certbot certonly --standalone -d домен.ноды</code>, затем скопируйте <code>fullchain.pem</code> и <code>privkey.pem</code> в <code>/etc/sn-node/tls/</code>." },
      { t: "Пропишите пути в профиле",
        d: "<code>streamSettings.tlsSettings.certificates</code>. Путь — до сертификата <b>этой ноды</b>, а не домена панели." },
      { t: "После продления перезапустите движок",
        d: "Сертификат читается только при старте. Кнопка «Перезапустить Xray» в меню ноды, или <code>--deploy-hook \"systemctl restart sn-node\"</code> в задании certbot." },
    ],
    name: 'Trojan + TLS',
    desc: 'Выглядит как обычный HTTPS-сайт. Требует своего домена и сертификата.',
    note: 'Пароль каждому клиенту панель выдаёт сама — это его UUID, отзыв подписки отключает доступ и здесь.',
    config: build([{
      tag: 'trojan-tls',
      listen: '0.0.0.0',
      port: 443,
      protocol: 'trojan',
      settings: { clients: [] },
      streamSettings: { network: 'tcp', security: 'tls', tlsSettings: {
        certificates: [{ certificateFile: '/etc/sn-node/tls/fullchain.pem', keyFile: '/etc/sn-node/tls/privkey.pem' }],
      } },
      sniffing: SNIFF,
    }]),
  },
  {
    id: 'hysteria2',
    setup: [
      { t: "Нужен свой домен и сертификат",
        d: "Без сертификата движок не запустится вовсе — и утянет за собой остальные инбаунды профиля. Порядок тот же, что у Trojan: certbot на ноде, файлы в <code>/etc/sn-node/tls/</code>, пути в <code>tlsSettings.certificates</code>." },
      { t: "Путь — до сертификата этой ноды",
        d: "В заготовке стоит заглушка <code>/ПУТЬ/К/</code>. Если оставить в ней домен панели, движок ответит <code>no such file or directory</code>." },
      { t: "Разные домены — разные профили",
        d: "Путь в конфиге один на весь профиль. Ноды с разными доменами не могут делить один TLS-инбаунд." },
      { t: "Это Hysteria поверх Xray, а не отдельный сервер",
        d: "В Xray протокол называется <code>hysteria</code>, версия задаётся полем <code>\"version\": 2</code>. Подключаются клиенты на движке Xray; самостоятельный QUIC-сервер hysteria к нему не подходит." },
    ],
    name: 'Hysteria2',
    desc: 'Быстрый на плохой линии: своё управление перегрузкой, хорошо держит потери пакетов.',
    note: 'Работает через QUIC/UDP. Нужны TLS-сертификат и открытый UDP-порт. Индивидуальные пароли клиентов агент передаёт в users.auth.',
    config: build([{
      tag: 'hysteria2',
      listen: '0.0.0.0',
      port: 8449,
      protocol: 'hysteria',
      // Индивидуальные credentials заполняет агент в settings.users[].auth.
      settings: { version: 2, users: [] },
      streamSettings: {
        // Hysteria 2 — собственный транспорт, а не протокол поверх TCP.
        // Без этого свежие клиентские ядра отвечают «not hysteria transport».
        network: 'hysteria',
        security: 'tls',
        tlsSettings: {
          certificates: [{ certificateFile: '/ПУТЬ/К/fullchain.pem', keyFile: '/ПУТЬ/К/privkey.pem' }],
        },
      },
      sniffing: SNIFF,
    }]),
  },
  {
    id: 'shadowsocks-2022',
    setup: [
      { t: "Задайте серверный ключ",
        d: "Поле <code>password</code> в настройках инбаунда — это ключ сервера, он обязателен. Без него движок отвечает <code>missing psk</code>." },
      { t: "Только методы 2022",
        d: "У классических методов один пароль на всех, и раздать клиентам разные ключи невозможно. Годятся только <code>2022-blake3-*</code>." },
      { t: "Личные ключи панель считает сама",
        d: "Клиент подключается парой «серверный ключ : личный ключ». Личный выводится из UUID клиента, поэтому отзыв подписки закрывает доступ и здесь." },
      { t: "Сертификат не нужен",
        d: "Shadowsocks шифрует сам, поверх TLS его не заворачивают." },
    ],
    name: 'Shadowsocks 2022',
    desc: 'Без TLS вообще: ни сертификата, ни домена. Отдельные ключи у каждого клиента.',
    note: 'Только методы 2022-*: у классических методов один пароль на всех, и раздать разным клиентам разные ключи невозможно.',
    config: build([{
      tag: 'ss-2022',
      listen: '0.0.0.0',
      port: 8388,
      protocol: 'shadowsocks',
      settings: {
        method: '2022-blake3-aes-128-gcm',
        // Серверный ключ. В многопользовательском shadowsocks-2022 он
        // обязателен: клиент подключается парой «серверный:личный», и без
        // него движок не стартует с ошибкой «missing psk».
        // Сгенерировать — кнопкой на странице профиля.
        password: 'СЕРВЕРНЫЙ_КЛЮЧ',
        clients: [],
        network: 'tcp,udp',
      },
      streamSettings: { network: 'tcp', security: 'none' },
      sniffing: SNIFF,
    }]),
  },
  {
    id: 'reality-plus-ws',
    setup: [
      { t: "Две точки входа в одном профиле",
        d: "Reality — основная, WebSocket — запасная на случай, когда основную режут. Клиент получает обе локации и переключается сам." },
      { t: "Настройте обе части",
        d: "Для Reality — ключи, короткий идентификатор и сайт маскировки. Для WebSocket — обратный прокси и совпадающий путь." },
      { t: "Разные порты",
        d: "Инбаунды слушают разные порты; оба должны быть открыты в брандмауэре." },
    ],
    name: 'Reality + WebSocket вместе',
    desc: 'Две точки входа в одном профиле: основная на 443 и запасная за прокси.',
    note: 'Удобно, когда часть клиентов сидит за сетями, где Reality не проходит. Каждый инбаунд даёт свой хост в подписке.',
    config: build([
      {
        tag: 'vless-reality',
        listen: '0.0.0.0',
        port: 443,
        protocol: 'vless',
        settings: { clients: [], decryption: 'none' },
        streamSettings: {
          network: 'tcp',
          security: 'reality',
          realitySettings: {
            dest: 'www.cloudflare.com:443',
            serverNames: ['www.cloudflare.com'],
            privateKey: 'ВАШ_ПРИВАТНЫЙ_КЛЮЧ',
            shortIds: ['ВАШ_SHORT_ID'],
          },
        },
        sniffing: SNIFF,
      },
      {
        tag: 'vless-ws',
        listen: '127.0.0.1',
        port: 8443,
        protocol: 'vless',
        settings: { clients: [], decryption: 'none' },
        streamSettings: { network: 'ws', security: 'none', wsSettings: { path: '/ВАШ_ПУТЬ' } },
        sniffing: SNIFF,
      },
    ]),
  },
];

/* ── блоки маршрутизации ──
   Вставляются в уже существующий конфиг: правило в routing.rules,
   outbound — в outbounds. */
const ROUTE_SNIPPETS = [
  ['Блокировать торренты', 'самое частое требование хостеров',
   `{ "type": "field", "protocol": ["bittorrent"], "outboundTag": "block" }`],

  ['Локальные сети — мимо туннеля', 'списком, а не geoip:private: файла может не быть на ноде',
   `{ "type": "field", "outboundTag": "direct",
  "ip": ["10.0.0.0/8", "172.16.0.0/12", "192.168.0.0/16", "127.0.0.0/8"] }`],

  ['Российские сайты напрямую', 'требует свежего geoip.dat на ноде',
   `{ "type": "field", "outboundTag": "direct",
  "ip": ["geoip:ru"], "domain": ["geosite:category-ru"] }`],

  ['Блокировать рекламу', 'geosite:category-ads-all',
   `{ "type": "field", "outboundTag": "block", "domain": ["geosite:category-ads-all"] }`],

  ['Заблокированные в РФ — в туннель', 'нужен zapret.dat, его ставит наш установщик ноды',
   `{ "type": "field", "outboundTag": "proxy", "domain": ["zapret:zapret"] }`],

  ['Цепочка на другую ноду', 'трафик уходит через второй сервер',
   `{ "tag": "chain", "protocol": "vless",
  "settings": { "vnext": [{ "address": "ВТОРАЯ_НОДА", "port": 443,
    "users": [{ "id": "UUID_СЕРВИСНОГО_КЛИЕНТА", "encryption": "none", "flow": "xtls-rprx-vision" }] }] },
  "streamSettings": { "network": "tcp", "security": "reality",
    "realitySettings": { "serverName": "www.cloudflare.com", "fingerprint": "chrome",
      "publicKey": "ПУБЛИЧНЫЙ_КЛЮЧ_ВТОРОЙ_НОДЫ", "shortId": "SHORT_ID" } } }`],

  ['Свой DNS для туннеля', 'клиенты перестают светить запросы провайдеру',
   `"dns": { "servers": ["1.1.1.1", "8.8.8.8"], "queryStrategy": "UseIPv4" }`],

  ['Статистика по клиентам', 'без неё лимиты трафика в тарифах не работают',
   `"policy": {
  "levels": { "0": { "statsUserUplink": true, "statsUserDownlink": true } },
  "system": { "statsInboundUplink": true, "statsInboundDownlink": true }
}`],

  ['API-инбаунд для агента', 'слушает только 127.0.0.1 — наружу открывать нельзя',
   `{ "tag": "api-in", "listen": "127.0.0.1", "port": 10085,
  "protocol": "dokodemo-door", "settings": { "address": "127.0.0.1" } }`],
];
