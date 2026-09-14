"use strict";
const cabinetMode = document.documentElement.dataset.surface === "cabinet";
const tg = window.Telegram?.WebApp,
  root = document.querySelector("#root"),
  tabs = document.querySelector("#tabs"),
  dialog = document.querySelector("#sheet");
const systemTheme = window.matchMedia("(prefers-color-scheme: dark)");
let themeMode = "auto";
try { themeMode = localStorage.getItem("sn.app.theme") || "auto"; } catch {}
if (!["auto", "light", "dark"].includes(themeMode)) themeMode = "auto";
function syncTheme() {
  const theme = themeMode === "auto" ? ((tg?.initData && tg?.colorScheme) || (systemTheme.matches ? "dark" : "light")) : themeMode;
  document.documentElement.dataset.theme = theme;
  document.documentElement.style.colorScheme = theme;
  const bg = theme === "dark" ? "#101321" : "#f7fafe";
  try { tg?.setHeaderColor?.(bg); tg?.setBackgroundColor?.(bg); } catch {}
  document.querySelectorAll(".theme-trigger").forEach(b => {
    b.innerHTML = themeIcon(theme);
    b.setAttribute("aria-label", CT.html`Оформление: ${theme === "dark" ? CT.html("тёмное") : CT.html("светлое")}. Изменить тему`);
  });
}
function themeIcon(theme) {
  return `<svg viewBox="0 0 24 24" aria-hidden="true">${theme === "dark" ? '<path d="M20.7 13.2A8.8 8.8 0 0 1 10.8 3.3a9 9 0 1 0 9.9 9.9Z"/>' : '<circle cx="12" cy="12" r="4"/><path d="M12 2v2m0 16v2M2 12h2m16 0h2M5 5l1.5 1.5m11 11L19 19M5 19l1.5-1.5m11-11L19 5"/>'}</svg>`;
}
function showThemeSettings() {
  showDialog(CT.html("Оформление"), CT.html`<p>Выберите удобную тему. В автоматическом режиме оформление меняется вместе с Telegram или устройством.</p><fieldset class="theme-options"><legend class="visually-hidden">Тема приложения</legend>${[["auto",CT.html("Автоматически"),CT.html("Как в Telegram или на устройстве")],["light",CT.html("Светлая"),CT.html("Светлые карточки и мягкие тени")],["dark",CT.html("Тёмная"),CT.html("Мягкий контраст на тёмном фоне")]].map(([id,title,hint])=>`<label class="theme-option"><input type="radio" name="app-theme" value="${id}" ${themeMode===id?'checked':''}><span><strong>${title}</strong><small>${hint}</small></span><svg viewBox="0 0 24 24" aria-hidden="true"><path d="m5 12 4 4L19 6"/></svg></label>`).join("")}</fieldset>`);
  dialog.querySelectorAll('[name="app-theme"]').forEach(input => input.onchange = () => {
    themeMode = input.value;
    try { localStorage.setItem("sn.app.theme", themeMode); } catch {}
    syncTheme();
    tg?.HapticFeedback?.selectionChanged?.();
  });
}
function mountThemeControl() {
  if (root.querySelector(".theme-trigger")) return;
  const b = document.createElement("button");
  b.className = "theme-trigger";
  b.type = "button";
  b.onclick = showThemeSettings;
  const top = root.querySelector(".hero .top");
  if (top) {
    const actions = document.createElement("div");
    actions.className = "hero-actions";
    const pill = top.querySelector(".pill");
    if (pill) actions.append(pill);
    actions.append(b);
    top.append(actions);
  } else {
    const title = root.querySelector("h1");
    if (!title) return;
    let heading = title.closest(".page-heading");
    if (!heading) {
      heading = document.createElement("div");
      heading.className = "page-heading";
      title.before(heading);
      heading.append(title);
    }
    heading.append(b);
  }
  syncTheme();
}
syncTheme();
tg?.onEvent?.("themeChanged", syncTheme);
systemTheme.addEventListener("change", syncTheme);
const esc = (s) =>
  String(s ?? "").replace(
    /[&<>"']/g,
    (c) =>
      ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[
        c
      ],
  );
const safeURL = (s) => {
  try {
    const u = new URL(s);
    return ["https:", "http:"].includes(u.protocol) ? u.href : null;
  } catch {
    return null;
  }
};
// Render only Telegram text formatting; never insert administrator HTML directly.
function richText(value) {
  const template = document.createElement("template");
  template.innerHTML = String(value || "");
  const render = (node) => {
    if (node.nodeType === 3) return esc(node.textContent);
    if (node.nodeType !== 1) return "";
    const tag = node.tagName.toLowerCase();
    if (["script", "style", "iframe", "object", "svg", "math", "template"].includes(tag)) return "";
    const content = [...node.childNodes].map(render).join("");
    if (["b", "strong", "i", "em", "u", "s", "del", "code", "pre", "blockquote"].includes(tag))
      return `<${tag}>${content}</${tag}>`;
    if (tag === "br") return "<br>";
    if (tag === "a" && safeURL(node.getAttribute("href")))
      return `<a href="${esc(safeURL(node.getAttribute("href")))}" target="_blank" rel="noopener noreferrer">${content}</a>`;
    return content + (["p", "div", "li"].includes(tag) ? "\n" : "");
  };
  return [...template.content.childNodes].map(render).join("");
}
const cleanBadge = (text) => String(text || "").replace(/[\p{Extended_Pictographic}\p{Regional_Indicator}\uFE0F\u200D\u20E3]/gu, "").trim();
const deviceLabel = (count) => !count ? CT.html("Без лимита устройств") : `${count} ${new Intl.PluralRules(CT.locale).select(count) === "one" ? CT.html("устройство") : new Intl.PluralRules(CT.locale).select(count) === "few" ? CT.html("устройства") : CT.html("устройств")}`;
const duration = (days) => `${days} ${new Intl.PluralRules(CT.locale).select(days) === "one" ? CT.html("день") : new Intl.PluralRules(CT.locale).select(days) === "few" ? CT.html("дня") : CT.html("дней")}`;
function canChoosePlan() {
  return config.shop_enabled === true || config.free_access_available === true;
}
function paymentOptions(catalog, tariff, days, serviceCurrency) {
  const price = tariff.prices.find(p => p.days === days && p.currency === serviceCurrency);
  if (!price) return [];
  const options = (catalog.methods_by_currency?.[serviceCurrency] || catalog.methods || [])
    .map(m => ({ ...m, currency: serviceCurrency, amount_minor: price.amount_minor }));
  const stars = tariff.prices.find(p => p.days === days && p.currency === "XTR");
  if (serviceCurrency !== "XTR" && stars && price.amount_minor > 0)
    options.push(...(catalog.methods_by_currency?.XTR || []).filter(m => m.id === "stars")
      .map(m => ({ ...m, currency: "XTR", amount_minor: stars.amount_minor })));
  return options;
}
const money = (n, c = "RUB") =>
  c === "XTR"
    ? `${new Intl.NumberFormat(CT.locale).format(n)} Stars`
    : new Intl.NumberFormat(CT.locale, {
        style: "currency",
        currency: c,
        maximumFractionDigits: 2,
      }).format(
        n / (["JPY", "KRW", "VND", "CLP", "ISK"].includes(c) ? 1 : 100),
      );
const bytes = (n) => {
  n = Number(n) || 0;
  const units = [CT.html("Б"), CT.html("КБ"), CT.html("МБ"), CT.html("ГБ"), CT.html("ТБ")];
  let i = 0;
  while (n >= 1024 && i < 4) {
    n /= 1024;
    i++;
  }
  return (
    new Intl.NumberFormat(CT.locale, { maximumFractionDigits: i ? 1 : 0 }).format(
      n,
    ) +
    " " +
    units[i]
  );
};
const date = (s) =>
  s
    ? new Intl.DateTimeFormat(CT.locale, {
        day: "numeric",
        month: "short",
        year: "numeric",
      }).format(new Date(s))
    : CT.html("Бессрочно");
const statusLabel = {
  active: CT.html("Активна"),
  expired: CT.html("Истекла"),
  limited: CT.html("Лимит трафика"),
  disabled: CT.html("Отключена"),
  success: CT.html("Оплачен"),
  pending: CT.html("Ожидает оплаты"),
  failed: CT.html("Не оплачен"),
  refunded: CT.html("Возвращён"),
  cancelled: CT.html("Отменён"),
  canceled: CT.html("Отменён"),
};
const ticketStatus = {
  open: CT.html("Ждёт ответа"),
  pending: CT.html("В работе"),
  closed: CT.html("Закрыто"),
};
let config = {},
  me = null,
  shop = null,
  section = "home",
  generation = 0,
  currency = "",
  currentTicket = null,
  drafts = new Map(),
  activePlan = null,
  pendingPayment = null,
  noticeTimer;
async function api(path, opts = {}) {
  const controller = new AbortController(),
    timer = setTimeout(() => controller.abort(), 25000);
  try {
    const r = await fetch((cabinetMode ? "/api/cabinet" : "/api/miniapp") + path, {
      ...opts,
      signal: controller.signal,
      headers: {
        "Content-Type": "application/json",
        ...(cabinetMode ? {"x-csrf-token": cabinetCsrf} : {"x-telegram-init-data": tg?.initData || ""}),
      },
    });
    const b = await r.json();
    if (!r.ok) {
      const e = new Error(
        r.status === 401
          ? (cabinetMode ? CT.html("Войдите по коду доступа") : CT.html("Сеанс завершён. Закройте Mini App и откройте его из бота заново."))
          : CT.error(b.error) || CT.html("Запрос не выполнен"),
      );
      e.status = r.status;
      throw e;
    }
    return CT.response(path,b);
  } catch (e) {
    if (e.name === "AbortError")
      throw new Error(
        CT.html("Сервер долго отвечает. Проверьте соединение и повторите."),
      );
    throw e;
  } finally {
    clearTimeout(timer);
  }
}
const post = (path, body = {}) =>
  api(path, { method: "POST", body: JSON.stringify(body) });
function notice(text) {
  clearTimeout(noticeTimer);
  let n = document.querySelector("#notice");
  n.hidden = true;
  if (dialog.open) {
    n = dialog.querySelector('.dialog-notice');
    if (!n) { n = document.createElement('div'); n.className='dialog-notice'; n.setAttribute('role','status'); n.setAttribute('aria-live','polite'); dialog.append(n); }
  }
  n.textContent = text;
  n.hidden = false;
  noticeTimer = setTimeout(() => (n.hidden = true), 4500);
}
function errorBox(e, retry) {
  root.innerHTML = CT.html`<div class="center"><h2>Не удалось загрузить</h2><p>${esc(e.message)}</p><button class="btn" id="retry">Повторить</button>${config.bot ? CT.html`<button class="btn line" id="openBot">Открыть бота</button>` : ""}${safeURL(config.support_url) ? CT.html('<button class="btn line" id="errorSupport">Написать в поддержку</button>') : ""}</div>`;
  root.querySelector("#retry").onclick = retry;
  root
    .querySelector("#errorSupport")
    ?.addEventListener("click", () => openURL(config.support_url));
  root
    .querySelector("#openBot")
    ?.addEventListener("click", () => openURL("https://t.me/" + config.bot));
}
function openURL(url) {
  const safe = safeURL(url);
  if (!safe) {
    notice(CT.html("Ссылка недоступна. Обратитесь в поддержку."));
    return;
  }
  const u = new URL(safe);
  if (u.hostname === "t.me" && tg?.openTelegramLink) tg.openTelegramLink(safe);
  else if (tg?.openLink) tg.openLink(safe);
  else window.open(safe, "_blank", "noopener,noreferrer");
}
async function copy(value) {
  try {
    if (!navigator.clipboard?.writeText) throw new Error();
    await navigator.clipboard.writeText(value);
    notice(CT.html("Ссылка скопирована"));
  } catch {
    showDialog(
      CT.html("Скопировать ссылку"),
      CT.html`<p>Нажмите на ссылку, выделите её и скопируйте.</p><textarea readonly class="copy-text" aria-label="Ссылка для копирования">${esc(value)}</textarea>`,
    );
    const f = dialog.querySelector("textarea");
    f.focus();
    f.select();
  }
}
function showDialog(title, body) {
  dialog.setAttribute("aria-labelledby", "sheetTitle");
  dialog.innerHTML = CT.html`<div class="dialog-head"><h2 id="sheetTitle">${esc(title)}</h2><button class="icon-btn" id="closeDialog" aria-label="Закрыть окно"><svg viewBox="0 0 24 24" aria-hidden="true"><path d="m6 6 12 12M18 6 6 18"/></svg></button></div><div class="dialog-body">${body}</div>`;
  dialog.querySelector("#closeDialog").onclick = () => dialog.close();
  if (!dialog.open) dialog.showModal();
  tg?.BackButton?.show?.();
}
dialog.addEventListener("close", () => {
  activePlan = null;
  syncBack();
});
dialog.addEventListener("click", (e) => {
  if (e.target === dialog) {
    const r = dialog.getBoundingClientRect();
    if (
      e.clientX < r.left ||
      e.clientX > r.right ||
      e.clientY < r.top ||
      e.clientY > r.bottom
    )
      dialog.close();
  }
});
function syncBack() {
  if (dialog.open || currentTicket || (section === "payments" && canChoosePlan())) tg?.BackButton?.show?.();
  else tg?.BackButton?.hide?.();
}
tg?.BackButton?.onClick?.(() => {
  if (dialog.open) dialog.close();
  else {
    currentTicket = null;
    navigate(section === "payments" ? "shop" : "help");
  }
});
const ICON = {
  home: '<circle cx="12" cy="12" r="9"/><path d="M3.6 9h16.8M3.6 15h16.8"/><path d="M12 3a15 15 0 0 1 0 18a15 15 0 0 1 0-18"/>',
  shop: '<path d="M3 8.5A2.5 2.5 0 0 1 5.5 6H18a3 3 0 0 1 3 3v7a3 3 0 0 1-3 3H6a3 3 0 0 1-3-3Z"/><path d="M3 9.5h16"/><circle cx="17.5" cy="14" r=".7"/>',
  ref:  '<circle cx="9" cy="8" r="3.6"/><path d="M2.5 20a6.5 6.5 0 0 1 13 0"/><path d="M18 8v6M15 11h6"/>',
  help: '<path d="M20.5 12.5a7.5 7.5 0 0 1-10.9 6.7L4 21l1.8-5.4A7.5 7.5 0 1 1 20.5 12.5Z"/>',
};

const navItems = () => [
  ["home", CT.html("Подписка")],
  [canChoosePlan() ? "shop" : "payments", canChoosePlan() ? CT.html("Тарифы") : CT.html("Платежи")],
  ...(config.referral_enabled ? [["ref", CT.html("Друзья")]] : []),
  ["help", CT.html("Помощь")],
];
function legacyDrawNav() {
  tabs.hidden = false;
  tabs.innerHTML = navItems()
    .map(
      ([id, label]) =>
        `<button class="tab ${(section === id || (id === "shop" && section === "payments")) ? "on" : ""}" data-nav="${id}" aria-label="${label}" title="${label}" aria-current="${(section === id || (id === "shop" && section === "payments")) ? "page" : "false"}"><svg viewBox="0 0 24 24" aria-hidden="true">${ICON[id === "payments" ? "shop" : id]}</svg></button>`,
    )
    .join("");
  tabs.querySelectorAll("[data-nav]").forEach(
    (b) =>
      (b.onclick = () => {
        tg?.HapticFeedback?.selectionChanged?.();
        currentTicket = null;
        navigate(b.dataset.nav);
      }),
  );
}
function navigate(next) {
  if (next === "shop") shop = null;
  section = next;
  generation++;
  drawNav();
  syncBack();
  window.scrollTo({ top: 0, behavior: "instant" });
  draw();
}
async function boot() {
  if (cabinetMode) return cabinetBoot();
  document.querySelector("#refresh").disabled = true;
  try {
    config = await api("/config");
    applyCustomerBrand();
    me = await api("/me");
    shop = null;
    currency = "";
    navigate("home");
  } catch (e) {
    errorBox(e, boot);
  } finally {
    document.querySelector("#refresh").disabled = false;
  }
}
document.querySelector("#refresh").onclick = async () => {
  const b = document.querySelector("#refresh");
  b.disabled = true;
  try {
    me = await api("/me");
    config = await api("/config");
    shop = null;
    if (section === "shop" && !canChoosePlan()) section = "home";
    if (section === "ref" && !config.referral_enabled) section = "home";
    if (currentTicket?.id) currentTicket = { id: currentTicket.id };
    generation++;
    drawNav();
    await draw();
  } catch (e) {
    notice(e.message);
  } finally {
    b.disabled = false;
  }
};
async function draw() {
  const n = generation;
  document.querySelector(".app-header").hidden = false;
  try {
    if (section === "home") drawHome();
    else if (section === "shop") await drawShop(n);
    else if (section === "payments") await drawPayments(n);
    else if (section === "ref") await drawRef(n);
    else await drawHelp(n);
    if (n === generation) mountCustomerHeader();
  } catch (e) {
    if (n === generation)
      errorBox(e, () => {
        generation++;
        draw();
      });
  }
}
function legacyDrawHome() {
  const active = me.status === "active",
    days = me.expires_at
      ? Math.max(
          0,
          Math.ceil((Date.parse(me.expires_at) - Date.now()) / 86400000),
        )
      : null;
  const used = Number(me.used) || 0,
    left = me.limit == null ? null : Math.max(0, me.limit - used),
    pct = me.limit ? Math.min(100, used / me.limit * 100) : 0,
    shortDate = value => value ? new Date(value).toLocaleDateString(CT.locale, {day:"numeric", month:"short"}) : "—",
    dayWord = days % 100 >= 11 && days % 100 <= 14 ? CT.html("дней") : days % 10 === 1 ? CT.html("день") : days % 10 >= 2 && days % 10 <= 4 ? CT.html("дня") : CT.html("дней"),
    main = !active ? CT.html("Нет доступа") : days == null ? CT.html("Бессрочно") : days === 0 ? CT.html("Истекает сегодня") : `${days} ${dayWord}`;
  document.querySelector(".app-header").hidden = true;
  root.innerHTML = CT.html`<section class="hero rise" aria-label="Подписка">
    <div class="top"><button class="who" id="account" aria-label="Управление подпиской">${safeURL(config.logo) ? `<img class="service-logo" src="${esc(safeURL(config.logo))}" alt="">` : ""}${esc(me.username)}<svg viewBox="0 0 24 24" aria-hidden="true"><path d="m9 5 7 7-7 7"/></svg></button><span class="pill ${active ? "live" : ""}"><span class="dot"></span>${esc(statusLabel[me.status] || CT.html("Нет подписки"))}</span></div>
    <h1 class="big">${main}</h1><div class="under">${active ? esc(me.tariff || "") + (me.expires_at ? CT.html(" · до ") + new Date(me.expires_at).toLocaleDateString(CT.locale,{day:"numeric",month:"long"}) : "") : CT.html("Выберите тариф или обратитесь в поддержку")}</div>
    ${active || me.limit != null ? `<div class="gauge"><div class="nums"><span>${left === null ? CT.html("Трафик безлимитный") : bytes(left)+CT.html(" осталось")}</span>${left === null ? "" : CT.html`<span>из ${bytes(me.limit)}</span>`}</div>${left === null ? "" : CT.html`<div class="track ${pct > 90 ? "hot" : ""}" role="progressbar" aria-label="Использовано трафика" aria-valuenow="${Math.round(pct)}" aria-valuemin="0" aria-valuemax="100"><i style="width:${pct}%"></i></div>`}</div>` : ""}
  </section>
  <div class="tiles rise"><${config.devices_enabled ? CT.html('button type="button" id="devices" aria-label="Мои устройства"') : 'div'} class="tile"><div class="n">${me.devices || 0}<span class="of">/${me.device_limit || "∞"}</span></div><div class="l">Устройства</div></${config.devices_enabled ? 'button' : 'div'}><div class="tile"><div class="n">${bytes(used)}</div><div class="l">Израсходовано</div></div><div class="tile"><div class="n">${me.traffic_reset_at && me.reset_strategy !== "no_reset" ? shortDate(me.traffic_reset_at) : "—"}</div><div class="l">Обновится</div></div></div>
  <div id="addonsEntry"></div>
  ${active && me.sub_url ? CT.html('<div class="btns rise"><button class="btn" id="connect">Подключиться</button><button class="btn line" id="copySub">Скопировать ссылку</button></div><p class="note rise">Ссылку вставляют в приложение VPN. Кнопка «Подключиться» откроет страницу, где приложение подберётся под ваше устройство.</p>') : canChoosePlan() ? CT.html('<button class="btn" id="buy">Выбрать тариф</button>') : CT.html('<button class="btn" id="getHelp">Обратиться в поддержку</button>')}
  ${config.welcome && config.welcome !== CT.html("Подписка, подключение и помощь — в одном месте.") ? `<p class="note">${esc(config.welcome)}</p>` : ""}
  ${active && days !== null && days <= 3 ? CT.html('<div class="warn-box">Подписка скоро закончится. Продлите её заранее, чтобы сохранить доступ.</div>') : ""}`;
  root.querySelector("#connect")?.addEventListener("click", () => openURL(me.sub_url));
  root.querySelector("#copySub")?.addEventListener("click", () => copy(me.sub_url));
  root.querySelector("#buy")?.addEventListener("click", () => navigate("shop"));
  root.querySelector("#getHelp")?.addEventListener("click", () => navigate("help"));
  root.querySelector("#devices")?.addEventListener("click", drawDevices);
  root.querySelector("#account").onclick = drawAccount;
  if(config.shop_enabled) mountAddonEntry();
  const logo = root.querySelector(".service-logo");
  if (logo) logo.onerror = () => logo.remove();
}
function legacyDrawAccount() {
  showDialog(CT.html("Управление подпиской"), CT.html`<p>${esc(me.username)} · ${esc(statusLabel[me.status] || CT.html("Нет подписки"))}</p><div class="btns">${canChoosePlan() ? CT.html('<button class="btn" id="renewAccount">Продлить или сменить тариф</button>') : ""}<button class="btn soft" id="historyAccount">История платежей</button><button class="btn line" id="refreshAccount">Обновить данные</button></div><section class="auto-section"><label class="auto-row"><span>Автопродление</span><input type="checkbox" id="autorenew" ${me.autorenew ? "checked" : ""}></label><p class="note">Продление сохранённым способом оплаты. Если автосписание недоступно, бот напомнит об оплате.</p></section>`);
  dialog.querySelector("#renewAccount")?.addEventListener("click", () => { dialog.close(); navigate("shop"); });
  dialog.querySelector("#historyAccount").onclick = () => { dialog.close(); navigate("payments"); };
  dialog.querySelector("#refreshAccount").onclick = async e => { e.target.disabled = true; await document.querySelector("#refresh").onclick(); if (dialog.open) drawAccount(); };
  dialog.querySelector("#autorenew").onchange = async e => {
    const b = e.target; b.disabled = true;
    try { await post("/autorenew", {enabled:b.checked}); me.autorenew = b.checked; notice(b.checked ? CT.html("Автопродление включено") : CT.html("Автопродление выключено")); }
    catch(err) {b.checked = !!me.autorenew; notice(err.message);}
    finally {b.disabled = false;}
  };
}
async function drawDevices() {
  try {
    const r = await api("/devices");
    showDialog(
      CT.html("Мои устройства"),
      CT.html`<p>Удаление освобождает место. Если устройство продолжит обновлять подписку, оно зарегистрируется снова.</p>${r.items.length ? r.items.map((d) => CT.html`<div class="record"><h3>${esc(d.model || d.platform || CT.html("Устройство"))}</h3><p>${esc(d.platform || CT.html("Платформа неизвестна"))} · ${esc(d.app_version || CT.html("Версия неизвестна"))}</p><p>Последний запрос: ${date(d.last_seen_at)}</p><button class="btn line" data-remove="${esc(d.hwid)}">Отвязать устройство</button></div>`).join("") : CT.html('<p class="center">Устройств пока нет. Они появятся после первого подключения.</p>')}`,
    );
    dialog.querySelectorAll("[data-remove]").forEach(
      (b) =>
        (b.onclick = async () => {
          if (!confirm(CT.html("Отвязать это устройство от подписки?"))) return;
          b.disabled = true;
          try {
            await api("/devices/" + encodeURIComponent(b.dataset.remove), {
              method: "DELETE",
            });
            me = await api("/me");
            if (section === "home") drawHome();
            await drawDevices();
            notice(CT.html("Устройство отвязано"));
          } catch (e) {
            notice(e.message);
            b.disabled = false;
          }
        }),
    );
  } catch (e) {
    notice(e.message);
  }
}
async function drawShop(n) {
  if (!shop) {
    root.innerHTML = CT.html('<div class="center">Загружаем тарифы…</div>');
    shop = await api("/tariffs");
  }
  if (n !== generation) return;
  currency = shop.currency;
  root.innerHTML = CT.html`<div class="page-heading"><h1>Тарифы</h1><button class="small-btn" id="paymentHistory">Платежи</button></div><p class="intro">После оплаты доступ включится автоматически.</p><div id="plans">${
    shop.tariffs
      .filter((t) => t.prices.some((p) => p.currency === currency))
      .map((t) => {
        const price = t.prices.filter((p) => p.currency === currency)
          .slice().sort((a,b) => a.amount_minor - b.amount_minor || a.days - b.days)[0];
        const badge = cleanBadge(t.badge) || (t.is_trial ? CT.html("Пробный") : "");
        return CT.html`<button class="plan" data-plan="${t.id}" data-period="${price.days}"><span class="plan-heading"><strong>${esc(t.title)}</strong>${badge ? `<span class="plan-badge">${esc(badge)}</span>` : ""}</span>${t.description ? `<span class="plan-desc">${esc(t.description)}</span>` : ""}<span class="plan-spec"><span>${deviceLabel(t.device_limit)}</span><span>${t.traffic_limit_bytes == null ? CT.html("Безлимитный трафик") : bytes(t.traffic_limit_bytes)}</span></span><span class="plan-footer"><span class="plan-price"><strong>${price.amount_minor === 0 ? CT.html("Бесплатно") : money(price.amount_minor, currency)}</strong><span>за ${duration(price.days)}</span></span><span class="plan-open" aria-hidden="true"><svg viewBox="0 0 24 24"><path d="m9 5 7 7-7 7"/></svg></span></span></button>`;
      })
      .join("") ||
    CT.html('<div class="center">Тарифы пока не опубликованы. Напишите в поддержку.</div>')
  }</div>`;
  root.querySelector("#paymentHistory").onclick = () => navigate("payments");
  root
    .querySelectorAll("[data-plan]")
    .forEach((b) => (b.onclick = () => buySheet(Number(b.dataset.plan), Number(b.dataset.period))));
}
function buySheet(id, selectedPeriod) {
  const t = shop.tariffs.find((t) => t.id === id),
    prices = t.prices.filter((p) => p.currency === currency);
  let period = prices.find(p => p.days === selectedPeriod)?.days ?? prices[0]?.days,
    promo = "",
    quote = null,
    methodQuotes = {},
    invoice = null,
    busy = false,
    quoteRevision = 0;
  showDialog(
    t.title,
    CT.html`<p>${esc(t.description || CT.html("Выберите срок подписки"))}</p><fieldset class="period-field"><legend>Срок подключения</legend><div class="period-options">${prices.map((p, index) => `<label class="period-option"><input type="radio" name="period" value="${p.days}" ${p.days === period ? "checked" : ""}><span class="period-choice"><strong>${duration(p.days)}</strong><span>${p.amount_minor === 0 ? CT.html("Бесплатно") : money(p.amount_minor, currency)}</span><svg viewBox="0 0 24 24" aria-hidden="true"><path d="m5 12 4 4L19 6"/></svg></span></label>`).join("")}</div></fieldset><label class="field-label" for="promo">Промокод <span>необязательно</span></label><div class="promo-row"><input id="promo" autocomplete="off" maxlength="64" placeholder="Введите код"><button class="small-btn" id="applyPromo">Применить</button></div><div id="quote" role="status"></div><div id="payActions"></div><div id="payError" role="alert"></div>`,
  );
  activePlan = id;
  const checkoutPane = dialog.querySelector(".dialog-body");
  const renderPay = () => {
    const price = prices.find((p) => p.days === period);
    const methods = paymentOptions(shop, t, period, currency);
    const box = dialog.querySelector("#payActions");
    if (!box) return;
    box.innerHTML = CT.html`<p class="checkout-total">К оплате <strong>${money(quote?.amount_minor ?? price.amount_minor, currency)}</strong></p>${quote && quote.days !== period ? CT.html`<p>Срок с бонусом: ${quote.days} дн.</p>` : ""}${price.amount_minor === 0 ? CT.html('<button class="btn" data-pay="free">Получить бесплатно</button>') : methods.length ? methods.map((m) => `<button class="btn ${m.currency !== currency ? "line" : ""}" data-pay="${esc(m.id)}" ${methodQuotes[m.currency]?.error ? "disabled" : ""}>${m.currency === "XTR" ? CT.html("Оплатить ") + money(methodQuotes.XTR?.amount_minor ?? m.amount_minor, "XTR") : CT.html("Оплатить · ") + esc(m.title)}</button>${methodQuotes[m.currency]?.error ? `<p class="note">${esc(methodQuotes[m.currency].error)}</p>` : ""}`).join("") : CT.html('<div class="warn-box">Оплата временно недоступна. Напишите в поддержку — вам помогут с подключением.</div>')}`;
    box
      .querySelectorAll("[data-pay]")
      .forEach((b) => (b.onclick = () => pay(b.dataset.pay)));
  };
  const clearQuote = () => {
    quoteRevision++;
    dialog.querySelector("#payError").classList.remove("is-error");
    dialog.querySelector("#payError").textContent = "";
    quote = null;
    methodQuotes = {};
    invoice = null;
    dialog.querySelector("#quote").textContent = "";
    renderPay();
  };
  dialog.querySelectorAll('input[name="period"]').forEach(input => input.onchange = (e) => {
    period = Number(e.target.value);
    clearQuote();
    tg?.HapticFeedback?.selectionChanged?.();
  });
  dialog.querySelector("#promo").oninput = (e) => {
    promo = e.target.value.trim();
    clearQuote();
  };
  dialog.querySelector("#applyPromo").onclick = async () => {
    const b = dialog.querySelector("#applyPromo"),
      revision = quoteRevision;
    b.disabled = true;
    try {
      const result = await post("/quote", {
        tariff_id: id,
        days: period,
        currency,
        provider: "",
        promo,
      });
      if (!b.isConnected || !dialog.open || revision !== quoteRevision) return;
      const options = paymentOptions(shop, t, period, currency);
      const extra = options.find(m => m.currency !== currency);
      let extraQuote;
      if (extra) {
        try {
          extraQuote = await post("/quote", {tariff_id:id, days:period, currency:extra.currency, provider:extra.id, promo});
        } catch (error) { extraQuote = {error: "Stars: " + error.message}; }
      }
      if (!b.isConnected || !dialog.open || revision !== quoteRevision) return;
      quote = result;
      methodQuotes = { [currency]: result, ...(extra ? {[extra.currency]:extraQuote} : {}) };
      dialog.querySelector("#quote").textContent = promo
        ? CT.html("Промокод применён")
        : CT.html("Цена без промокода");
      renderPay();
    } catch (e) {
      if (b.isConnected && dialog.open) {
        dialog.querySelector("#payError").classList.add("is-error");
        dialog.querySelector("#payError").textContent = e.message;
      }
    } finally {
      b.disabled = false;
    }
  };
  async function pay(provider) {
    if (busy) return;
    if (promo && !quote) {
      dialog.querySelector("#payError").classList.add("is-error");
      dialog.querySelector("#payError").textContent =
        CT.html("Сначала примените промокод, чтобы увидеть итоговую сумму.");
      return;
    }
    const method = paymentOptions(shop, t, period, currency).find(m => m.id === provider);
    const paymentCurrency = method?.currency || currency;
    busy = true;
    const controls = [...dialog.querySelectorAll("input,select,button")];
    controls.forEach(b => b.disabled = true);
    dialog.querySelector("#payError").classList.remove("is-error");
    dialog.querySelector("#payError").textContent = CT.html("Готовим счёт…");
    try {
      invoice = await post("/pay", {
        tariff_id: id,
        days: period,
        currency: paymentCurrency,
        provider,
        promo,
      });
      if (!checkoutPane.isConnected || !dialog.open) {
        pendingPayment=invoice.payment_id || null;
        notice(invoice.free ? CT.html("Доступ включён. Обновите подписку.") : CT.html("Счёт создан. Он доступен в разделе «Платежи»."));
        return;
      }
      if (invoice.free) {
        me = await api("/me");
        shop = null;
        dialog.close();
        navigate("home");
        notice(CT.html("Доступ включён"));
        return;
      }
      pendingPayment = invoice.payment_id;
      dialog.querySelector("#payActions").innerHTML =
        CT.html`<p>Счёт №${invoice.payment_id} · ${money(invoice.amount_minor, invoice.currency || paymentCurrency)}</p>${invoice.instructions ? `<p class="instruction-text">${esc(invoice.instructions)}</p>` : ""}${safeURL(invoice.url) ? CT.html('<button class="btn" id="openInvoice">Открыть оплату</button>') : ""}<button class="btn line" id="checkInvoice">Проверить оплату</button>`;
      dialog.querySelector("#payError").textContent =
        CT.html("После оплаты вернитесь сюда и нажмите «Проверить оплату».");
      dialog
        .querySelector("#openInvoice")
        ?.addEventListener("click", () => openInvoice(invoice));
      dialog.querySelector("#checkInvoice").onclick = () =>
        checkPayment(invoice.payment_id, dialog.querySelector("#checkInvoice"));
      if (safeURL(invoice.url)) openInvoice(invoice);
    } catch (e) {
      if (checkoutPane.isConnected && dialog.open) {
        dialog.querySelector("#payError").classList.add("is-error");
        dialog.querySelector("#payError").textContent = e.message;
      }
      else notice(e.message);
    } finally {
      busy = false;
      controls.forEach(b => b.disabled = false);
      if (invoice && checkoutPane.isConnected) {
        dialog
          .querySelectorAll('input[name="period"],#promo,#applyPromo')
          .forEach((b) => (b.disabled = true));
      }
    }
  }
  renderPay();
}
function openInvoice(invoice) {
  const url = safeURL(invoice.url);
  if (!url) {
    notice(CT.html("Используйте реквизиты из счёта или напишите в поддержку"));
    return;
  }
  const u = new URL(url);
  if (u.hostname === "t.me" && u.pathname.startsWith("/$") && tg?.openInvoice)
    tg.openInvoice(url, (status) => {
      if (status === "paid") checkPayment(invoice.payment_id);
      else
        notice(
          status === "cancelled"
            ? CT.html("Оплата отменена. Счёт можно открыть снова.")
            : CT.html("Вернитесь к счёту, чтобы проверить оплату."),
        );
    });
  else openURL(url);
}
async function checkPayment(id, button) {
  if (button) button.disabled = true;
  try {
    const r = await post("/payments/" + id + "/check");
    if (r.status === "success") {
      me = await api("/me");
      shop = null;
      pendingPayment = null;
      if (dialog.open) dialog.close();
      navigate("home");
      notice(r.addon_pending ? CT.html("Оплачено. Пакет сохранён и включится после активации подходящей подписки.") : CT.html("Оплата подтверждена. Подписка обновлена."));
    } else notice(statusLabel[r.status] || r.status);
  } catch (e) {
    notice(e.message);
  } finally {
    if (button) button.disabled = false;
  }
}
async function drawPayments(n, offset = 0) {
  if (!offset)
    root.innerHTML =
      CT.html('<h1>Платежи</h1><div class="center">Загружаем историю…</div>');
  const r = await api("/payments?offset=" + offset);
  if (n !== generation) return;
  if (!offset)
    root.innerHTML =
      CT.html('<h1>Платежи</h1><p class="intro">Счета и история оплаты вашей подписки.</p><div id="history"></div>');
  root.querySelector("#morePayments")?.remove();
  const box = root.querySelector("#history");
  box.insertAdjacentHTML(
    "beforeend",
    r.items
      .map(
        (p) =>
          `<div class="record"><div class="record-top"><strong>${esc(p.title || CT.html("Оплата подписки"))}</strong><span class="state ${p.status === "success" ? "good" : ""}">${esc(statusLabel[p.status] || p.status)}</span></div><p>${money(p.amount_minor, p.currency)} · ${date(p.created_at)} · №${p.id}</p>${p.instructions && p.status === "pending" ? `<p class="instruction-text">${esc(p.instructions)}</p>` : ""}${safeURL(p.url) ? CT.html`<button class="btn" data-invoice="${p.id}">Продолжить оплату</button>` : ""}${["pending", "failed"].includes(p.status) ? CT.html`<button class="btn line" data-check="${p.id}">Проверить оплату</button>` : ""}</div>`,
      )
      .join("") ||
      (!offset ? CT.html('<div class="center">Платежей пока нет.</div>') : ""),
  );
  if (r.has_more)
    box.insertAdjacentHTML(
      "beforeend",
      CT.html('<button class="btn line" id="morePayments">Показать ещё</button>'),
    );
  box.querySelectorAll("[data-invoice]").forEach(
    (b) =>
      (b.onclick ||= () => {
        const p = r.items.find((p) => p.id === Number(b.dataset.invoice));
        if (p) openInvoice({ ...p, payment_id: p.id });
      }),
  );
  box
    .querySelectorAll("[data-check]")
    .forEach(
      (b) => (b.onclick = () => checkPayment(Number(b.dataset.check), b)),
    );
  root.querySelector("#morePayments")?.addEventListener("click", (e) => {
    e.target.disabled = true;
    drawPayments(n, offset + 20).catch((err) => {
      notice(err.message);
      e.target.disabled = false;
    });
  });
}
async function drawRef(n) {
  root.innerHTML = CT.html('<div class="center">Загружаем приглашения…</div>');
  const r = await api("/referral");
  if (n !== generation) return;
  const link = r.bot ? `https://t.me/${r.bot}?start=r_${r.slug}` : null;
  root.innerHTML = CT.html`<h1 class="rise">Пригласить друга</h1>${r.enabled ? CT.html`<div class="hero referral-hero rise"><div class="big">${r.percent}%</div><div class="under">с оплат приглашённых клиентов</div></div><div class="tiles rise"><div class="tile"><div class="n">${r.invited}</div><div class="l">Пришло</div></div><div class="tile"><div class="n">${(r.wallets || []).map(w => money(w.earned_minor,w.currency)).join("<br>") || money(r.earned || 0,r.currency)}</div><div class="l">Начислено</div></div><div class="tile"><div class="n">${(r.wallets || []).map(w => money(w.balance_minor,w.currency)).join("<br>") || money(r.balance || 0,r.currency)}</div><div class="l">К выплате</div></div></div>${link ? CT.html`<div class="link-box rise"><b>Ваша ссылка</b><div class="u">${esc(link)}</div></div><div class="btns rise"><button class="btn" id="share">Пригласить друга</button><button class="btn line" id="copyRef">Скопировать ссылку</button></div>` : CT.html("<p>Ссылка появится после запуска бота.</p>")}<p class="note">По вопросам выплаты напишите в поддержку.</p>` : CT.html("<p>Приглашения пока отключены.</p>")}`;
  root.querySelector("#copyRef")?.addEventListener("click", () => copy(link));
  root
    .querySelector("#share")
    ?.addEventListener("click", () =>
      openURL("https://t.me/share/url?url=" + encodeURIComponent(link)),
    );
}
async function drawHelp(n, offset = 0) {
  if (currentTicket) return drawTicket(n);
  if (!offset)
    root.innerHTML = CT.html('<div class="center">Загружаем поддержку…</div>');
  const r = await api("/tickets?offset=" + offset);
  if (n !== generation) return;
  if (!offset)
    root.innerHTML = CT.html`<h1>Помощь</h1><div class="intro rich-text">${richText(config.support_text)}</div>${safeURL(config.support_url) ? `<button class="btn line" id="supportLink">${esc(config.support_label)}</button>` : ""}<details class="documents"><summary><svg viewBox="0 0 24 24" aria-hidden="true"><path d="M14 3H6a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V9Z M14 3v6h6 M8 13h8 M8 17h5"/></svg><span>Документы и условия сервиса</span><svg class="chevron" viewBox="0 0 24 24" aria-hidden="true"><path d="m9 5 7 7-7 7"/></svg></summary><div class="documents-body"><div class="rich-text">${richText(config.docs_text)}</div><div class="document-links">${(
      config.docs_links || []
    )
      .filter((d) => safeURL(d.url))
      .map(
        (d) =>
          `<a class="document-link" href="${esc(safeURL(d.url))}" target="_blank" rel="noopener noreferrer"><span>${esc(d.label)}</span><svg viewBox="0 0 24 24" aria-hidden="true"><path d="M7 17 17 7M7 7h10v10"/></svg></a>`,
      )
      .join(
        "",
      )}</div></div></details><h2>Мои обращения</h2>${config.tickets_enabled ? CT.html('<button class="btn" id="newTicket">Написать в поддержку</button>') : CT.html('<p class="note">Новые обращения отключены. История переписки доступна ниже.</p>')}<div id="ticketList"></div>`;
  root.querySelector("#moreTickets")?.remove();
  root
    .querySelector("#ticketList")
    .insertAdjacentHTML(
      "beforeend",
      r.items
        .map(
          (t) =>
            `<button class="ticket-row" data-ticket="${t.id}"><span><strong>${esc(t.subject)}</strong><small>${date(t.updated_at)} · №${t.id}</small></span><span class="state">${ticketStatus[t.status] || esc(t.status)}</span></button>`,
        )
        .join("") || (!offset ? CT.html('<p class="note">Обращений пока нет.</p>') : ""),
    );
  if (r.has_more)
    root
      .querySelector("#ticketList")
      .insertAdjacentHTML(
        "beforeend",
        CT.html('<button class="btn line" id="moreTickets">Показать ещё</button>'),
      );
  root
    .querySelector("#supportLink")
    ?.addEventListener("click", () => openURL(config.support_url));
  root.querySelectorAll("[data-ticket]").forEach(
    (b) =>
      (b.onclick = () => {
        currentTicket = { id: Number(b.dataset.ticket) };
        generation++;
        drawTicket(generation);
      }),
  );
  root.querySelector("#newTicket")?.addEventListener("click", () => {
    currentTicket = { new: true };
    drawTicket(generation);
  });
  root.querySelector("#moreTickets")?.addEventListener("click", (e) => {
    e.target.disabled = true;
    drawHelp(n, offset + 20).catch((err) => {
      notice(err.message);
      e.target.disabled = false;
    });
  });
}
async function drawTicket(n) {
  syncBack();
  let t = currentTicket;
  if (!t) return;
  try {
    if (!t.new && !t.messages) {
      t = await api("/tickets/" + t.id);
      if (n !== generation) return;
      currentTicket = t;
    }
    const key = t.id || "new";
    root.innerHTML = CT.html`<button class="small-btn back-btn" id="ticketBack">← К обращениям</button><h1>${t.new ? CT.html("Новое обращение") : esc(t.subject)}</h1>${t.has_more ? CT.html('<button class="btn line" id="older">Предыдущие сообщения</button>') : ""}<div class="messages">${(t.messages || []).map((m) => `<div class="msg ${m.who === "admin" ? "them" : "mine"}"><strong>${m.who === "admin" ? CT.html("Поддержка") : CT.html("Вы")}</strong><p>${esc(m.body)}</p><small>${date(m.at)}</small></div>`).join("")}</div>${t.status === "closed" ? CT.html('<p class="note">Обращение закрыто. Если нужна помощь, создайте новое.</p>') : config.tickets_enabled ? CT.html`<label class="field-label" for="message">${t.new ? CT.html("Что случилось?") : CT.html("Ваш ответ")}</label><textarea id="message" maxlength="4000" placeholder="Напишите сообщение">${esc(drafts.get(key) || "")}</textarea><div class="note" id="count"></div><button class="btn" id="sendMessage">Отправить сообщение</button>` : CT.html('<p class="note">Новые сообщения отключены администратором.</p>')}<div id="ticketError" role="alert"></div>`;
    root.querySelector("#ticketBack").onclick = () => {
      currentTicket = null;
      navigate("help");
    };
    const input = root.querySelector("#message");
    const counter = () => {
      drafts.set(key, input.value);
      root.querySelector("#count").textContent = input.value.length + " / 4000";
    };
    if (input) {
      counter();
      input.oninput = counter;
    }
    root.querySelector("#older")?.addEventListener("click", async (e) => {
      e.target.disabled = true;
      try {
        const older = await api(
          "/tickets/" + t.id + "?before=" + t.messages[0].id,
        );
        if (n !== generation) return;
        t.messages = [...older.messages, ...t.messages];
        t.has_more = older.has_more;
        drawTicket(n);
      } catch (err) {
        notice(err.message);
        e.target.disabled = false;
      }
    });
    root.querySelector("#sendMessage")?.addEventListener("click", async (e) => {
      const body = input.value.trim();
      if (!body) {
        input.focus();
        return;
      }
      const b = e.target;
      b.disabled = true;
      try {
        const r = await post("/tickets" + (t.new ? "" : "/" + t.id), { body });
        drafts.delete(key);
        if (n !== generation) return;
        currentTicket = { id: t.new ? r.id : t.id };
        await drawTicket(n);
        notice(CT.html("Сообщение отправлено"));
      } catch (err) {
        root.querySelector("#ticketError").textContent = err.message;
        b.disabled = false;
      }
    });
  } catch (e) {
    if (n === generation) errorBox(e, () => drawTicket(n));
  }
}
tg?.ready?.();
tg?.expand?.();
tg?.setHeaderColor?.("secondary_bg_color");
tg?.onEvent?.("activated", () => {
  if (pendingPayment) checkPayment(pendingPayment);
});
window.addEventListener("DOMContentLoaded", boot);

let addonCatalog = null;
async function mountAddonEntry(){
  const box=root.querySelector('#addonsEntry');
  try{
    const catalog=await api('/addons');
    if(!box?.isConnected)return;
    addonCatalog=catalog;
    const kinds=['traffic','devices'].filter(kind=>catalog.items.some(p=>p.kind===kind));
    if(!kinds.length && !catalog.grants.length)return;
    box.innerHTML=CT.html`<section class="addon-entry"><div class="addon-entry-head"><strong>Расширить подписку</strong><span>Без смены тарифа</span></div><div class="addon-entry-actions">${kinds.map(kind=>`<button class="btn soft" data-addon-kind="${kind}">${kind==='traffic'?CT.html('Добавить трафик'):CT.html('Добавить устройства')}<svg viewBox="0 0 24 24" aria-hidden="true"><path d="m9 5 7 7-7 7"/></svg></button>`).join('')}</div>${catalog.grants.length?CT.html('<button class="addon-history" id="myAddons">Мои докупки</button>'):''}</section>`;
    box.querySelectorAll('[data-addon-kind]').forEach(b=>b.onclick=()=>drawAddons(b.dataset.addonKind));
    box.querySelector('#myAddons')?.addEventListener('click',()=>drawAddons());
  }catch(e){if(box?.isConnected)box.innerHTML=CT.html('<button class="btn line" id="retryAddons">Загрузить доступные докупки</button>');box?.querySelector('#retryAddons')?.addEventListener('click',mountAddonEntry);}
}
function addonTerm(p){return p.expires_at?CT.html('Действует до ')+date(p.expires_at):CT.html('На срок действующей подписки');}
async function drawAddons(kind){
  showDialog(kind==='traffic'?CT.html('Добавить трафик'):kind==='devices'?CT.html('Добавить устройства'):CT.html('Мои докупки'),CT.html('<p>Загружаем пакеты…</p>'));
  const pane=dialog.querySelector('.dialog-body');
  try{
    const catalog=await api('/addons');
    if(!pane.isConnected||!dialog.open)return;
    addonCatalog=catalog;
    const items=catalog.items.filter(p=>!kind||p.kind===kind);
    pane.innerHTML=CT.html`<p class="intro">${kind==='traffic'?CT.html('Дополнительный трафик прибавится к лимиту и действует до ближайшего сброса или окончания подписки.'):kind==='devices'?CT.html('Дополнительные места действуют до конца текущего оплаченного срока. При продлении они не продлеваются автоматически.'):CT.html('Оплаченные пакеты и их сроки действия.')} Срок самой подписки не меняется.</p><div class="addon-packages">${items.map(p=>`<button class="addon-package" data-package="${p.id}"><span><strong>${esc(p.title)}</strong><small>${esc(addonTerm(p))}</small></span><span class="addon-price">${money(p.amount_minor,catalog.currency)}<svg viewBox="0 0 24 24" aria-hidden="true"><path d="m9 5 7 7-7 7"/></svg></span></button>`).join('')}</div>${!items.length&&kind?CT.html('<div class="warn-box">Для текущей подписки нет доступных пакетов. Проверьте тариф или обратитесь в поддержку.</div>'):''}${catalog.grants.length?CT.html`<section class="addon-owned"><h3>Уже оплачено</h3>${catalog.grants.filter(p=>!kind||p.kind===kind).map(p=>`<div class="addon-owned-row"><strong>+${p.quantity} ${p.kind==='traffic'?CT.html('ГБ'):CT.html('устр.')}</strong><span>${p.pending?CT.html('Ожидает действующую подписку'):esc(addonTerm(p))}</span></div>`).join('')}</section>`:''}`;
    pane.querySelectorAll('[data-package]').forEach(b=>b.onclick=()=>buyAddon(catalog.items.find(p=>p.id===Number(b.dataset.package)),catalog));
  }catch(e){if(pane.isConnected){pane.innerHTML='<p role="alert">'+esc(e.message)+CT.html('</p><button class="btn line" id="retryAddonSheet">Повторить</button>');pane.querySelector('button').onclick=()=>drawAddons(kind);}}
}
function buyAddon(pack,catalog){
  const methods=[...(catalog.methods_by_currency[catalog.currency]||[]).map(m=>({...m,currency:catalog.currency,amount:pack.amount_minor})),...(catalog.currency!=='XTR'&&pack.stars_minor?(catalog.methods_by_currency.XTR||[]).map(m=>({...m,currency:'XTR',amount:pack.stars_minor})):[])];
  showDialog(pack.title,CT.html`<div class="addon-checkout-summary"><strong>${esc(pack.title)}</strong><span>${esc(addonTerm(pack))}</span></div><p>Пакет добавится после подтверждения оплаты. Тариф и срок подписки останутся прежними.</p><p class="checkout-total">К оплате <strong>${money(pack.amount_minor,catalog.currency)}</strong></p><div class="btns" id="addonMethods">${methods.map((m,i)=>`<button class="btn ${i?'line':''}" data-addon-method="${i}">${esc(m.title)}${m.currency==='XTR'?' · '+money(m.amount,'XTR'):''}</button>`).join('')||CT.html('<div class="warn-box">Оплата пока не настроена. Обратитесь в поддержку.</div>')}</div><div id="addonPayError" role="alert"></div><button class="addon-history" id="backToAddons">Другие пакеты</button>`);
  const pane=dialog.querySelector('.dialog-body');let busy=false;
  pane.querySelector('#backToAddons').onclick=()=>drawAddons(pack.kind);
  pane.querySelectorAll('[data-addon-method]').forEach(button=>button.onclick=async()=>{
    if(busy)return;busy=true;pane.querySelectorAll('button').forEach(b=>b.disabled=true);
    try{
      const method=methods[Number(button.dataset.addonMethod)];
      const invoice=await post('/addons/pay',{package_id:pack.id,provider:method.id,currency:method.currency});
      pendingPayment=invoice;
      if(!pane.isConnected||!dialog.open){notice(CT.html('Счёт создан. Он доступен в истории платежей.'));return;}
      pane.innerHTML=CT.html`<div class="addon-checkout-summary"><strong>${esc(pack.title)}</strong><span>Счёт №${invoice.payment_id}</span></div><p class="checkout-total">К оплате <strong>${money(invoice.amount_minor,invoice.currency)}</strong></p>${invoice.instructions?'<p class="instruction-text">'+esc(invoice.instructions)+'</p>':''}<p class="note">Если оплата поступит после окончания подписки, пакет сохранится и включится после её активации.</p><div class="btns">${safeURL(invoice.url)?CT.html('<button class="btn" id="openAddonInvoice">Перейти к оплате</button>'):''}<button class="btn line" id="checkAddonInvoice">Проверить оплату</button></div>`;
      pane.querySelector('#openAddonInvoice')?.addEventListener('click',()=>openInvoice(invoice));
      pane.querySelector('#checkAddonInvoice').onclick=e=>checkPayment(invoice.payment_id,e.target);
    }catch(e){if(pane.isConnected){pane.querySelector('#addonPayError').textContent=e.message;pane.querySelectorAll('button').forEach(b=>b.disabled=false);}}
    finally{busy=false;}
  });
}
