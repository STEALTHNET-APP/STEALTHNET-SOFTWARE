/* ═══════════ Клиент API: реальные данные вместо моков ═══════════
   Наполняет ту же глобальную структуру DB, которую читают страницы,
   поэтому код экранов не меняется — меняется только источник данных. */
'use strict';

const API = {
  base: '',                                   // тот же домен, Caddy проксирует /api
  token: localStorage.getItem('sn_token') || null,

  setToken(t) {
    this.token = t;
    t ? localStorage.setItem('sn_token', t) : localStorage.removeItem('sn_token');
  },

  async call(path, { method = 'GET', body, raw = false, responseType = 'json' } = {}) {
    const controller = new AbortController();
    const timer = setTimeout(() => controller.abort(), 30000);
    let res;
    try { res = await fetch(this.base + path, {
      method,
      signal: controller.signal,
      headers: {
        ...(body ? { 'Content-Type': raw ? (body.type || 'application/octet-stream') : 'application/json' } : {}),
        ...(this.token ? { Authorization: 'Bearer ' + this.token } : {}),
      },
      body: body ? (raw ? body : JSON.stringify(body)) : undefined,
    }); } catch (error) {
      if (controller.signal.aborted) throw new Error('Сервер не ответил за 30 секунд. Проверьте соединение и повторите запрос.');
      throw error;
    } finally { clearTimeout(timer); }

    if (res.status === 401) {
      // Токен протух или отозван — уводим на вход, а не показываем пустые экраны.
      this.setToken(null);
      if (location.hash !== '#/login') location.hash = '#/login';
      throw new Error('нужен вход');
    }
    if (!res.ok) {
      let message = 'HTTP ' + res.status;
      try { message = (await res.json()).error || message; } catch (_) {}
      const error = new Error(message); error.status = res.status; throw error;
    }
    return res.status === 204 ? null : responseType === 'blob' ? res.blob() : res.json();
  },

  login: (username, password, code) =>
    API.call('/api/auth/login', { method: 'POST', body: { username, password, code: code || null } }),
  logout: () => API.call('/api/auth/logout', { method: 'POST' }).catch(() => {}),
};

/* ── преобразование ответов API в структуру, которую ждут страницы ── */

const GIB = 1024 ** 3;

/* У XTR, JPY, KRW и VND нет дробной части: 129 XTR — это ровно 129,
   а не 1.29. Делить на 100 вслепую значит показать цену в сто раз меньше. */
const ZERO_DECIMAL = new Set(['XTR', 'JPY', 'KRW', 'VND', 'CLP', 'ISK']);
const minorToUnits = (minor, cur) => (ZERO_DECIMAL.has(cur) ? minor : minor / 100);
const unitsToMinor = (units, cur) =>
  ZERO_DECIMAL.has(cur) ? Math.round(units) : Math.round(units * 100);
const toGb = (bytes) => (bytes == null ? null : bytes / GIB);
const RESET_LABEL = { no_reset: 'NO_RESET', day: 'DAY', week: 'WEEK', month: 'MONTH' };
const STATUS_LABEL = { active: 'ACTIVE', disabled: 'DISABLED', limited: 'LIMITED', expired: 'EXPIRED' };

function mapClient(c) {
  return {
    id: String(c.id),
    username: c.username,
    tg: null,
    tgId: '',
    email: null,
    status: STATUS_LABEL[c.status] || c.status.toUpperCase(),
    // «онлайн» считаем по последней активности: за 5 минут — значит в сети
    online: c.last_online_at ? Date.now() - new Date(c.last_online_at) < 5 * 60 * 1000 : false,
    tariff: c.tariff_code || '—',
    period: c.expires_at ? '' : '∞',
    paidUntil: c.expires_at,
    autoRenew: !!c.autorenew,
    ltv: minorToUnits(c.ltv_minor || 0,c.ltv_currency||DB.currency),
    ltvCurrency:c.ltv_currency||DB.currency, ltvByCurrency:c.ltv_by_currency||[],
    payments: c.payment_count || 0,
    usedGb: toGb(c.traffic_used_bytes || 0),
    limitGb: toGb(c.traffic_limit_bytes),
    // Раньше при отсутствии поля подставлялся 'month' — панель уверенно
    // показывала стратегию, которой у подписки не было.
    strategy: (c.reset_strategy || 'no_reset').toUpperCase(),
    /// Когда счётчик обнулится в следующий раз. Пусто — не обнуляется.
    resetAt: c.traffic_reset_at || null,
    hwid: c.device_count || 0,
    hwidLimit: c.device_limit || 1,
    // Список сквадов приходит из API. Раньше это поле объявлялось дважды,
    // и пустой массив ниже затирал настоящее значение — в карточке всегда
    // было «сквадов нет», сколько бы их ни назначили.
    squads: c.squads || [],
    externalSquadId: c.external_squad_id == null ? "" : String(c.external_squad_id),
    tag: c.tag,
    shortUuid: c.short_id,
    uuid: c.public_id,
    createdAt: c.created_at,
    lastOnline: c.last_online_at,
    // Заметка администратора: раньше здесь стояла пустая строка, поэтому
    // сохранённый текст в карточке не появлялся никогда.
    desc: c.note || '',
    trafficTotal: c.traffic_total_bytes || 0,
    // Откуда пришёл клиент. Пусто — пришёл сам.
    referrer: c.referrer_title || null,
    referrerSlug: c.referrer_slug || null,
  };
}

/* Опознание клиента: Telegram и почта лежат отдельной таблицей и
   приходят только в подробном ответе по одному клиенту. */
function applyIdentities(u, list) {
  for (const i of (list || [])) {
    if (i.kind === 'telegram') { u.tgId = i.value; u.tg = i.value; }
    if (i.kind === 'email') u.email = i.value;
  }
  return u;
}

function mapNode(n) {
  return {
    id: String(n.id),
    cc: n.country_code,
    name: n.name,
    addr: n.address,
    port: n.api_port,
    status: n.status,
    // Движок отдельно от агента: нода может отвечать панели, пока Xray
    // на ней лежит и не пускает ни одного клиента.
    engineOk: typeof n.engine_ok === 'boolean' ? n.engine_ok : null,
    engineError: n.engine_error || null,
    xray: n.engine_version || '—',
    agent: n.agent_version || '—',
    safeEngineUpdate: n.safe_engine_update === true,
    selfsteal: n.selfsteal || null,
    lastSeen: n.last_seen_at,
    online: n.online_count || 0,
    todayBytes: n.today_bytes || 0,
    // Нагрузка — реальная от агента, а не выдуманная из числа онлайна.
    cpu: n.cpu_percent,
    ram: n.ram_percent,
    la: n.la || [null, null, null],
    rxBps: n.downlink_bps,
    txBps: n.uplink_bps,
    cpuModel: n.cpu_model,
    cpuCores: n.cpu_cores,
    kernel: n.kernel,
    memTotal: n.mem_total_bytes,
    memUsed: n.mem_used_bytes,
    uptimeSec: n.uptime_seconds,
    iface: n.iface,
    // Ряд скоростей по часам за сутки — для искры в списке нод.
    spark: n.spark || [],
    rxTotal: n.rx_total_bytes,
    txTotal: n.tx_total_bytes,
    profile: n.profile_name || '—',
    profileId: n.profile_id,
    inbounds: n.inbounds || [],
    multiplier: n.traffic_multiplier,
    notify: n.notify,
    trackTraffic: n.count_traffic,
    infraProviderId: n.infra_provider_id,
    infraProvider: n.infra_provider_name,
    infraProviderLogo: n.infra_provider_logo,
    costMinor: n.monthly_cost_minor || 0,
    billDay: n.bill_day,
  };
}

function mapTariff(t) {
  return {
    id: String(t.id),
    name: t.code, title: t.title, trial: t.is_trial, locales:t.locales||{},
    emoji: t.is_trial ? 'gift' : t.code === 'PRO' ? 'star' : 'zap',
    devices: t.device_limit,
    addons: t.addons || [],
    trafficGb: toGb(t.traffic_limit_bytes),
    active: t.is_active,
    order: t.id,
    buyers30d: t.purchases_30d || 0,
    // Валюту обязательно тащим дальше: у тарифа цены в нескольких валютах,
    // и без неё редактор сохранял их все как одну, затирая остальные.
    prices: (t.prices || []).map((p) => ({
      d: p.period_days,
      cur: p.currency,
      p: minorToUnits(p.amount_minor, p.currency),
    })),
    desc: t.description || '',
    badge: t.badge || '',
    reset: t.reset_strategy,
    squads: t.squads || [],
    hidden: !t.is_visible,
  };
}

const PAY_STATUS_MAP = { success: 'success', failed: 'failed', refunded: 'refunded', pending: 'pending', canceled: 'canceled' };

function mapPayment(p) {
  return {
    id: String(p.id),
    user: p.username || '—',
    clientId: p.client_id == null ? null : String(p.client_id),
    amount: minorToUnits(p.amount_minor, p.currency),
    currency: p.currency,
    refunded: minorToUnits(p.refunded_minor || 0, p.currency),
    method: p.provider,
    kind: p.kind,
    methodType: p.provider.includes('crypto') ? 'crypto' : 'card',
    item: `${p.tariff_code || '—'}${p.period_days ? ' · ' + p.period_days + ' дн' : ''}`,
    status: PAY_STATUS_MAP[p.status] || p.status,
    at: p.paid_at || p.created_at,
    txid: p.provider_txid || '—',
    err: p.error_message || undefined,
  };
}

function mapHost(h) {
  return {
    id: String(h.id),
    order: h.sort_order,
    remark: h.remark,
    addr: h.address,
    port: h.port,
    // Пусто — привязка слетела при замене конфига профиля.
    profile: h.profile_name || '',
    profileId: h.profile_id == null ? "" : String(h.profile_id),
    options: h.options || {},
    hostHeader: h.host_header || "",
    inbound: h.inbound_tag || '',
    sni: h.sni || '',
    fp: h.fingerprint || '',
    alpn: h.alpn || '',
    path: h.path || '',
    pbk: h.public_key || '',
    sid: h.short_id || '',
    security: h.security.toUpperCase(),
    enabled: h.is_enabled,
  };
}

/* ── загрузка всех справочников разом ── */

/// Справочники, которые не критичны для старта: если endpoint ещё не готов
/// или упал, панель должна открыться, а не показать белый экран.
async function loadOptional(path, map) {
  try {
    const data = await API.call(path);
    delete DB.loadErrors[path];
    return Array.isArray(data) ? data.map(map) : [];
  } catch (e) {
    if (e.status !== 403) DB.loadErrors[path] = e.message;
    console.warn('не загрузил ' + path + ': ' + e.message);
    return [];
  }
}

async function loadDB() {
  DB.loadErrors = {};
  const identity = await API.call('/api/auth/me');
  DB.admin = identity.admin;
  const support = DB.admin.role === 'support';
  // health не требует токена и заодно проверяет связь с БД
  PAGES._health = await API.call('/api/health').catch(() => null);
  const [dashboard, clients, tariffs, payments, nodes, hosts, squads, profiles] = await Promise.all([
    API.call('/api/dashboard'),
    API.call('/api/clients?limit=200'),
    API.call('/api/tariffs'),
    API.call('/api/payments?limit=100'),
    API.call('/api/nodes'),
    API.call('/api/hosts'),
    API.call('/api/squads'),
    support ? Promise.resolve([]) : API.call('/api/profiles'),
  ]);

  // Настройки нужны до отрисовки: из них берётся валюта системы.
  const settingsMap = support ? identity.settings : await API.call('/api/settings');
  DB.dashboard = dashboard;
  DB.users = clients.items.map(mapClient);
  DB.usersTotal = clients.total;
  DB.tariffs = tariffs.map(mapTariff);
  DB.payments = payments.map(mapPayment);
  DB.nodes = nodes.map(mapNode);
  DB.nodesFetchedAt = Date.now();
  DB.hosts = hosts.map(mapHost);
  DB.squadsInt = squads.map((s) => ({
    id: String(s.id), name: s.name, members: s.members,
    inbounds: s.inbounds, inboundRefs: s.inbound_refs || [], desc: s.description || '',
  }));
  // Второстепенные разделы грузим параллельно и не роняем панель при ошибке.
  // Валюта системы одна на всю панель: цены, отчёты и выплаты считаются в ней.
  DB.currency = (settingsMap['billing.currency'] || 'USD').toUpperCase();
  DB.brandName = settingsMap['brand.name'] || '';
  DB.brandAccent = settingsMap['brand.accent'] || '';
  // Адрес сервиса подписок. Раньше в карточке клиента стоял прошитый
  // «sub.stealthnet.app», и панель показывала ссылку, которая никуда не
  // ведёт: у каждой установки свой домен, и задаётся он в настройках.
  DB.subPublicUrl = (settingsMap['subscription.public_url'] || identity.settings?.['subscription.public_url'] || '').replace(/\/+$/, '');
  // Желаемая версия движка. Ноды сверяются с ней сами; панель
  // показывает отставших, чтобы «локация не работает» не пришлось
  // искать перебором по серверам.
  DB.engineVersion = settingsMap['nodes.engine_version'] || '';
  // Публичный адрес панели: из него собираются команды для нод. Адрес
  // страницы годится не всегда — панель бывает за обратным прокси.
  DB.panelPublicUrl = (settingsMap['panel.public_url'] || '').replace(/\/+$/, '');

  const [promos, partners, tickets, broadcasts, srh, devices, providers] = await Promise.all([
    loadOptional('/api/promos', (p) => ({
      id: String(p.id), code: p.code,
      type: p.kind === 'percent' ? 'percent' : p.kind === 'days' ? 'days' : 'fixed',
      value: p.value, currency:p.currency, uses: p.used_count, limit: p.max_uses,
      until: p.valid_until, active: p.is_active,
      firstOnly: p.first_purchase_only,
      appliesTo: p.tariff_code || 'все тарифы',
    })),
    loadOptional('/api/partners', (p) => ({
      id: String(p.id), user: p.title, share: p.share_percent, referred: p.referred,
      earned: minorToUnits(p.earned_minor, p.currency), paid: minorToUnits(p.paid_minor, p.currency),
      balance: minorToUnits(p.balance_minor, p.currency), currency:p.currency, active: p.is_active,
      wallets:p.wallets || [{currency:p.currency,earned_minor:p.earned_minor,paid_minor:p.paid_minor,balance_minor:p.balance_minor}],
      link:p.referral_url || '', slug:p.slug,
    })),
    loadOptional('/api/tickets', (t) => ({
      id: String(t.id), user: t.username, subject: t.subject,
      status: t.status === 'closed' ? 'closed' : t.status,
      prio: t.priority === 'high' ? 'high' : t.priority === 'low' ? 'low' : 'normal',
      updated: t.updated_at,
      msgs: [{ who: 'user', text: t.last_message || '', at: '' }],
    })),
    loadOptional('/api/broadcasts', (b) => ({
      id: String(b.id), name: b.title, status: b.status,lastError:b.last_error,retryAt:b.retry_at,
      segment: b.segment || 'all',
      audience: { all:'все клиенты', active:'активные', expired:'истёкшие',
                  limited:'на лимите', trial:'на пробном' }[b.segment] || b.segment || 'все',
      body: b.body || '',
      total: b.total_count || 0, buttonText:b.button_text||'',buttonUrl:b.button_url||'',photoId:b.photo_id||null,failed:b.failed_count||0,
      sent: b.sent_count, opened: 0, at: b.scheduled_at || b.created_at,
    })),
    loadOptional('/api/srh', (r) => ({
      short: r.short_id, ua: r.user_agent || '—', ip: r.ip || '—',
      resp: r.response_code || '—', at: r.requested_at,
    })),
    loadOptional('/api/devices', (d) => ({
      hwid: d.hwid, platform: d.platform || '—', model: d.model || '—',
      app: d.app_version || '—', first: d.first_seen_at, last: d.last_seen_at,
      username: d.username, clientId: String(d.client_id), sharedBy: d.shared_by,
    })),
    loadOptional('/api/pay/providers', (p) => ({
      id: p.id, title: p.title, currencies: p.currencies,
      configured: p.is_configured, enabled: p.is_enabled,
    })),
  ]);

  DB.promos = promos;
  DB.partners = partners;
  DB.tickets = tickets;
  DB.broadcasts = broadcasts;
  DB.srh = srh;
  DB.devicesAll = devices;
  DB.payProviders = providers;

  // Устройства по клиентам — для карточки клиента.
  DB.hwidDevices = {};
  for (const d of devices) {
    (DB.hwidDevices[d.clientId] = DB.hwidDevices[d.clientId] || []).push(d);
  }

  DB.profiles = profiles.map((p) => ({
    id: String(p.id), name: p.name, inbounds: p.inbounds,
    nodes: DB.nodes.filter((n) => n.profile === p.name).map((n) => n.name),
    updated: p.updated_at, json: JSON.stringify(p.config, null, 2),
    version: p.version, engine: p.engine,
    // Разбор инбаундов для списка: протокол, порт и транспорт видно
    // сразу, без открытия редактора. Служебный вход пропускаем — он
    // есть у всех и ничего не различает.
    parts: (p.config?.inbounds || [])
      .filter((i) => i.protocol !== 'dokodemo-door')
      .map((i) => ({
        tag: i.tag || '—',
        protocol: i.protocol || '—',
        port: i.port,
        net: i.streamSettings?.network || 'tcp',
        sec: i.streamSettings?.security || 'none',
      })),
  }));
  return DB;
}

/* Структура заполняется из API; пустые массивы — чтобы страницы,
   для которых endpoint ещё не написан, не падали. */
const DB = {
  dashboard: null, loadErrors: {},
  currency: 'USD',
  users: [], usersTotal: 0, tariffs: [], payments: [], nodes: [], hosts: [],
  squadsInt: [], profiles: [],
  promos: [], partners: [], broadcasts: [], tickets: [], squadsExt: [], plugins: [],
  sessions: [], torrentReports: [], srh: [], hwidDevices: {}, providers: [],
  billingRecords: [], apiTokens: [], passkeys: [], devicesAll: [], payProviders: [],
  templates: {
    XRAY_JSON: { label: 'Xray JSON', body: '{\n  "remarks": "{{TITLE}}"\n}' },
    MIHOMO: { label: 'Mihomo', body: 'proxies: []' },
    SINGBOX: { label: 'Sing-box', body: '{\n  "outbounds": []\n}' },
    CLASH: { label: 'Clash', body: 'proxies: []' },
    STASH: { label: 'Stash', body: 'proxies: []' },
  },
  responseRules: [], subApps: { ios: [], android: [], pc: [] },
};

const findUser = (id) => DB.users.find((u) => u.id === id);
const userByName = (n) => DB.users.find((u) => u.username === n);
const allInbounds = () => [...new Set(DB.profiles.flatMap((p) => p.inbounds))];

/* ── старт приложения ── */

async function bootApp() {
  // Следим за поздней разметкой: модалки и перерисовки таблиц иначе
  // остались бы русскими на английском интерфейсе.
  watchI18n();
  window.addEventListener('hashchange', route);

  if (!API.token) {
    authed = false;
    location.hash = '#/login';
    route();
    return;
  }

  try {
    document.getElementById('app').innerHTML = '<div class="startup-state" role="status"><div class="startup-mark">S</div><h1>Подключаемся к панели</h1><p>Загружаем состояние сервиса и настройки.</p></div>';
    await loadDB();
    authed = true;
    if (!location.hash || location.hash === '#/login') location.hash = '#/home';
    route();
  } catch (e) {
    authed = false;
    if (e.message === 'нужен вход') {
      location.hash = '#/login';
      route();
    } else {
      document.getElementById('app').innerHTML = `<div class="startup-state" role="alert"><div class="startup-mark">!</div><h1>Не удалось загрузить панель</h1><p>${esc(e.message)}</p><button class="btn primary" id="retryStartup">Повторить загрузку</button></div>`;
      document.getElementById('retryStartup').onclick = () => location.reload();
    }
  }
}

/// Перезагрузить данные и перерисовать текущую страницу.
async function refreshDB() {
  try {
    await loadDB();
    route();
  } catch (e) {
    toast('Ошибка обновления: ' + e.message, 'err');
  }
}
