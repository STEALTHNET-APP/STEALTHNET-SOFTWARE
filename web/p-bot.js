/* Управление ботом и Telegram Mini App. Один экран настроек для двух поверхностей. */
"use strict";
const BOT_GROUPS = [
  {
    id: "appearance",
    title: "Оформление и меню",
    note: "Название, приветствие и основные действия клиента.",
    fields: [
      [
        "bot.brand_name",
        "Название сервиса",
        "text",
        "Название из конфигурации сервера",
      ],

      [
        "bot.welcome_text",
        "Приветственное сообщение",
        "textarea",
        "Пусто — сразу открыть меню",
      ],
      ["bot.welcome_always", "Приветствие при каждом /start", "check"],
      [
        "bot.menu_note",
        "Объявление под меню бота",
        "textarea",
        "Новости, акция или плановые работы",
      ],
      ["bot.button.buy", "Купить доступ", "text", "🚀 Купить доступ"],
      ["bot.button.renew", "Продлить", "text", "🔄 Продлить"],
      ["bot.button.connect", "Подключиться", "text", "🔌 Подключиться"],
      ["bot.button.subscription", "Моя подписка", "text", "🔑 Моя подписка"],
      [
        "bot.button.referral",
        "Пригласить друга",
        "text",
        "🎁 Пригласить друга",
      ],
      ["bot.button.tickets", "Обращения", "text", "✉️ Мои обращения"],
      ["bot.button.docs", "Документы", "text", "📄 Документы"],
      [
        "bot.connect_inline",
        "Показывать кнопку подключения в боте",
        "check",
        true,
      ],
    ],
  },
  {
    id: "miniapp",
    title: "Mini App",
    note: "Подписка и покупки внутри Telegram. Настройки применяются при обновлении приложения.",
    fields: [
      ["bot.miniapp_enabled", "Включить Mini App", "check"],
      [
        "bot.miniapp_label",
        "Подпись кнопки Mini App",
        "text",
        "🚀 Открыть приложение",
      ],
      ["bot.miniapp_shop", "Покупка и продление в Mini App", "check", true],
      [
        "bot.miniapp_devices",
        "Самостоятельная отвязка устройств",
        "check",
        true,
      ],
    ],
  },
  {
    id: "support",
    title: "Поддержка и документы",
    note: "Контакты и документы в боте. Содержимое сайта и Mini App настраивается в клиентском кабинете.",
    fields: [
      [
        "bot.support_url",
        "Ссылка на поддержку",
        "url",
        "https://t.me/ваша_поддержка",
      ],
      [
        "bot.support_label",
        "Подпись кнопки поддержки",
        "text",
        "💬 Связь с поддержкой",
      ],
      [
        "bot.support_text",
        "Подсказка перед обращением",
        "textarea",
        "Опишите проблему: устройство, приложение и что происходит при подключении.",
      ],
      [
        "bot.tickets_enabled",
        "Принимать обращения из бота и Mini App",
        "check",
        true,
      ],
      [
        "bot.docs_text",
        "Текст документов",
        "textarea",
        "Условия использования сервиса",
      ],
    ],
  },
  {
    id: "alerts", title: "Уведомления команды",
    note: "События сервиса в отдельной Telegram-группе. Настройки не меняют уведомления клиентов.",
    fields: [
      ["bot.admin_alerts_enabled", "Отправлять уведомления в группу", "check", false],
      ["bot.alert_chat_id", "ID группы", "text", "-1001234567890"],
      ["bot.alert_thread_id", "ID темы · необязательно", "text", "Пусто — в общий чат"],
      ["bot.alert_payments_enabled", "Оплаты и возвраты · в том числе докупки", "check", true],
      ["bot.alert_clients_enabled", "Новые клиенты", "check", true],
      ["bot.alert_tickets_enabled", "Новые тикеты и ответы клиентов", "check", true],
      ["bot.alert_service_enabled", "Проблемы сервиса и восстановление", "check", true],
    ],
  },
  {
    id: "growth",
    title: "Приглашения и напоминания",
    note: "Реферальная программа и уведомления о завершении подписки.",
    fields: [
      ["bot.referral_enabled", "Показывать приглашение друзей", "check"],
      [
        "bot.referral_percent",
        "Процент для новых участников программы",
        "number",
        "10",
      ],
      ["bot.notify_enabled", "Напоминать об окончании подписки", "check", true],
      ["bot.notify_days", "За сколько дней напоминать", "days", "3, 1"],
      [
        "bot.notify_text",
        "Текст напоминания",
        "textarea",
        "Пусто — стандартное напоминание с командой /buy",
      ],
    ],
  },
  {
    id: "channel",
    title: "Обязательный канал",
    note: "Проверка подписки перед использованием бота.",
    fields: [
      ["bot.require_channel", "Канал", "text", "@имя_канала или -100…"],
      [
        "bot.channel_url",
        "Ссылка на канал",
        "url",
        "Для закрытого канала укажите приглашение",
      ],
      [
        "bot.channel_text",
        "Сообщение перед проверкой",
        "textarea",
        "Чтобы пользоваться ботом, подпишитесь на наш канал.",
      ],
    ],
  },
  {
    id: "images",
    title: "Картинки экранов",
    note: "HTTPS-ссылка или file_id Telegram. Пустое поле наследует общую картинку.",
    fields: [
      ["bot.image.default", "Общая картинка", "text", "https://…/banner.jpg"],
      ...[
        ["menu", "Главное меню"],
        ["buy", "Тарифы"],
        ["sub", "Подписка"],
        ["docs", "Документы"],
        ["tickets", "Обращения"],
      ].map(([k, t]) => ["bot.image." + k, t, "text", "Общая картинка"]),
    ],
  },
];
const BOT_MENU_BUILTINS = [
  { id:"miniapp", title:"Mini App", icon:"browser", key:"bot.miniapp_label", fallback:"🚀 Открыть приложение", note:"Если Mini App включено и выбран работающий кабинет" },
  { id:"connect", title:"Подключение", icon:"plug", key:"bot.button.connect", fallback:"🔌 Подключиться", note:"При активной подписке и включённой кнопке подключения" },
  { id:"purchase", title:"Покупка и продление", icon:"card", key:"bot.button.buy", fallback:"🚀 Купить доступ", note:"Новый клиент видит покупку, активный — продление" },
  { id:"addons", title:"Докупки трафика и устройств", icon:"plus", key:"bot.button.addons", fallback:"Докупки трафика и устройств", note:"Пакеты и цены задаются в тарифах. Только для подходящей подписки" },
  { id:"subscription", title:"Моя подписка", icon:"shield", key:"bot.button.subscription", fallback:"🔑 Моя подписка", note:"При активной подписке" },
  { id:"referral", title:"Приглашение друзей", icon:"users2", key:"bot.button.referral", fallback:"🎁 Пригласить друга", note:"Если включена реферальная программа" },
  { id:"tickets", title:"Обращения", icon:"chat", key:"bot.button.tickets", fallback:"✉️ Мои обращения", note:"Если включён приём обращений" },
  { id:"docs", title:"Документы", icon:"file", key:"bot.button.docs", fallback:"📄 Документы", note:"Документы и условия сервиса" },
  { id:"support", title:"Поддержка", icon:"send", key:"bot.support_label", fallback:"💬 Связь с поддержкой", note:"Если указана ссылка на поддержку" },
];
function defaultBotMenu() { return BOT_MENU_BUILTINS.map(b => ({id:b.id,enabled:true})); }
function normalizeBotMenu(value) {
  const entries = Array.isArray(value) ? value.map(e => ({...e,enabled:e.enabled !== false})) : [];
  for (const b of defaultBotMenu()) if (!entries.some(e => e.id === b.id)) entries.push(b);
  return entries;
}
function botEmojiID(value) {
  const id=String(value||"").trim().replace(/^\[([0-9]+)\]$/u,"$1");
  return /^[0-9]{1,20}$/u.test(id) && /[1-9]/u.test(id) ? id : null;
}
function botIconLabel(label) {
  return label.replace(/^[\s\p{Extended_Pictographic}\p{Regional_Indicator}\uFE0F\u200D\u20E3]+/u,"") || label;
}
function botMenuURL(value) {
  if (value.length > 2048 || /[\s\x00-\x1f\x7f]/u.test(value)) return false;
  try { const u=new URL(value);return ["https:","http:","tg:"].includes(u.protocol) && !!u.hostname && !u.username && !u.password; } catch { return false; }
}
function moveBotMenu(entries,id,index) {
  const next=entries.slice(),from=next.findIndex(e=>e.id===id);
  if(from<0)return next;
  const [entry]=next.splice(from,1);next.splice(Math.max(0,Math.min(index,next.length)),0,entry);return next;
}
function botMenuPreview(entries, values, active) {
  const flag=(key,fallback=false)=>values[key] == null ? fallback : values[key] === true;
  const text=(key,fallback)=>String(values[key]||"").trim()||fallback;
  return entries.filter(e=>e.enabled).flatMap(e=>{
    const b=BOT_MENU_BUILTINS.find(b=>b.id===e.id);
    if(!b)return [{label:e.label?.trim()||"Новая кнопка",custom:true,emoji:botEmojiID(e.icon_custom_emoji_id)}];
    if(e.id==="miniapp" && (!flag("bot.miniapp_enabled") || !String(values["_cabinet.miniapp_url"] || "").startsWith("https://")))return [];
    if(e.id==="connect" && (!active || !flag("bot.connect_inline",true) || !String(values["subscription.public_url"] || "").startsWith("https://")))return [];
    if(e.id==="subscription" && !active)return [];
    if(e.id==="referral" && !flag("bot.referral_enabled"))return [];
    if(e.id==="tickets" && !flag("bot.tickets_enabled",true))return [];
    if(e.id==="support" && !text("bot.support_url",""))return [];
    return [{label:e.id==="purchase" && active ? text("bot.button.renew","🔄 Продлить") : text(b.key,b.fallback),custom:false,emoji:botEmojiID(e.icon_custom_emoji_id)}];
  });
}
const menuLabels = BOT_GROUPS[0].fields.filter(([key]) => key.startsWith("bot.button."));
BOT_GROUPS[0].fields = BOT_GROUPS[0].fields.filter(([key]) => !key.startsWith("bot.button."));
BOT_GROUPS[0].title = "Оформление";
BOT_GROUPS.splice(1,0,{id:"menu",title:"Кнопки меню",note:"Соберите главное меню бота: измените порядок, скройте лишнее и добавьте свои ссылки.",fields:menuLabels});
function botMenuEditor() {
  return `<div class="bot-menu-tools"><button class="btn primary" id="menuAdd" disabled>${I("plus",14)} Добавить свою кнопку</button><button class="btn" id="menuReset" disabled>Сбросить порядок</button></div><details class="bot-menu-emoji-help"><summary>Как добавить Premium-эмодзи</summary><p>Откройте «Эмодзи кнопки» и вставьте числовой ID, например <code>[5215361191051798408]</code>. Подойдёт ID со скобками или без них. Нужен Telegram Premium у владельца бота либо дополнительный username бота через Fragment. Если Telegram отклонит иконку, меню придёт без неё.</p><p>Саму анимацию показывает Telegram; в предпросмотре отмечено место иконки.</p></details><div id="menuError" role="alert"></div><div id="menuAnnouncement" class="sr-only" role="status"></div><div class="bot-menu-designer"><div><p class="hint menu-order-hint">Перетаскивайте за ${I("grip",14)} или меняйте порядок стрелками. Скрытая кнопка сохраняет своё место.</p><ol class="bot-menu-list" id="menuList" aria-label="Порядок кнопок бота"></ol></div><aside class="bot-menu-preview"><h3>Предпросмотр</h3><div class="bot-menu-audience" role="group" aria-label="Состояние клиента"><button class="btn sm active" data-audience="active" aria-pressed="true">С подпиской</button><button class="btn sm" data-audience="new" aria-pressed="false">Новый клиент</button></div><div class="bot-menu-message"><strong id="menuPreviewBrand"></strong><p id="menuPreviewStatus"></p></div><div id="menuPreviewButtons"></div><p class="sub-note">Действия зависят от подписки и включённых разделов. Нажимать кнопки в предпросмотре не нужно.</p></aside></div><details class="bot-menu-labels"><summary>Подписи стандартных кнопок</summary>`;
}
const BOT_FIELDS = BOT_GROUPS.flatMap((g) =>
  g.fields.map(([k, label, type, def]) => ({ k, label, type, def })),
);
registerPage({
  id: "bot",
  title: "Телеграм-бот",
  group: "Продажи",
  icon: "chat",
  render() {
    const field = ([k, label, type, def]) => {
      const id = k.replaceAll(".", "-");
      if (type === "check")
        return `<label class="bot-toggle"><span>${esc(label)}</span><input id="${id}" data-k="${k}" type="checkbox"></label>`;
      return `<div class="field ${type === "textarea" ? "bot-wide" : ""}"><label for="${id}">${esc(label)}</label>${type === "textarea" ? `<textarea id="${id}" class="inp" data-k="${k}" rows="3" maxlength="${k === "bot.menu_note" ? 1000 : 3000}" placeholder="${esc(def || "")}"></textarea>` : `<input id="${id}" class="inp" data-k="${k}" type="${type === "number" ? "number" : "text"}" ${type === "number" ? 'min="0" max="100" step="0.1"' : ""} placeholder="${esc(def || "")}" ${type === "url" ? 'inputmode="url"' : ""}>`}</div>`;
    };
    return `<div class="page-head"><div><h1>Бот и Mini App</h1><div class="desc">Настройте путь клиента: от первого запуска до подключения и поддержки.</div></div><div class="actions"><button class="btn section-help" id="botHelp">${I("info", 14)} Инструкция</button><button class="btn" id="botCheck">${I("pulse", 14)} Проверить бота</button></div></div>
  <div id="botStatus" role="status"></div><div id="botLoadError" role="alert"></div>
  <div class="bot-workspace"><nav class="bot-sections" aria-label="Настройки бота">${BOT_GROUPS.map((g, i) => `<button class="btn ${i ? "" : "active"}" data-section="${g.id}" aria-pressed="${!i}">${g.title}</button>`).join("")}<div class="bot-related"><a href="#/tariffs">Тарифы и пробный доступ</a><a href="#/providers">Способы оплаты</a><a href="#/support">Обращения клиентов</a><a href="#/subpage">Приложения для подключения</a></div></nav>
  <div class="bot-content">${BOT_GROUPS.map((g, i) => `<section data-panel="${g.id}" ${i ? "hidden" : ""}><h2>${g.title}</h2><p class="sub-note">${g.note}</p>${g.id === "menu" ? botMenuEditor() : ""}<div class="bot-fields">${g.fields.map(field).join("")}</div>${g.id === "menu" ? "</details>" : ""}${g.id === "support" ? `<h3>Ссылки на документы</h3><div id="docsList"></div><button class="btn sm" id="docsAdd">${I("plus", 14)} Добавить документ</button>` : ""}${g.id === "alerts" ? `<div class="bot-alerts-guide"><h3>Как подключить группу</h3><ol><li>Добавьте этого бота в рабочую группу и разрешите ему отправлять сообщения.</li><li>Напишите /chatid в группе или нужной теме. Бот покажет ID группы и темы — перенесите их в поля выше.</li><li>Выберите события, сохраните настройки и отправьте тест.</li></ol><p>Ноды: нет отчёта агента более 3 минут, ошибка VPN-движка, CPU или память от 95%. Также приходят ошибки фоновых задач и сообщения о восстановлении. Повторяющаяся проблема не создаёт новое уведомление каждый цикл.</p><p>Доставку выполняет фоновый сервис, обычно в течение 10 секунд. Полную недоступность сервера нужно отслеживать внешним мониторингом.</p><div class="bot-alert-actions"><button class="btn primary" id="alertsTest" disabled>${I("send",14)} Отправить тест</button><button class="btn" id="alertsRefresh">${I("refresh",14)} Статус доставки</button><button class="btn" id="alertsRetry" disabled>Повторить недоставленные</button></div><div id="alertsStatus" role="status"></div></div>` : ""}${g.id === "growth" ? '<p class="hint">До 5 порогов от 1 до 30 дней. В тексте доступны {days} — число дней и {date} — дата окончания. Пустой текст использует стандартное сообщение. Существующие индивидуальные ставки меняются в «Партнёрке».</p>' : ""}${g.id === "channel" ? '<p class="hint">Бот должен быть администратором канала. При ошибке Telegram проверка пропускает клиента; диагностика покажет проблему. Проверка канала относится к боту.</p>' : ""}${g.id === "miniapp" ? '<p class="hint">Mini App работает только на установленном кабинете. <a href="#/cabinet">Выбрать сервер и настроить брендинг</a>. Выключение Mini App запрещает вход в приложение. Выключение покупок запрещает новые счета из Mini App; история платежей и подписка остаются доступны.</p>' : ""}${g.id === "appearance" ? '<p class="hint">В приветствии и объявлении бота доступен Telegram HTML. Пустые подписи кнопок используют стандартные названия.</p>' : ""}</section>`).join("")}</div></div>
  <div class="bot-savebar"><span id="botDirty" role="status">Загружаем настройки…</span><button class="btn primary" id="botSave" disabled>Сохранить изменения</button></div>`;
  },
  async bind(root) {
    const save = root.querySelector("#botSave"),
      status = root.querySelector("#botStatus"),
      dirty = root.querySelector("#botDirty"),
      docs = root.querySelector("#docsList");
    const readonly = ["readonly", "support"].includes(DB.admin?.role);
    let loaded = false,
      initial = "",
      saving = false,
      menu = defaultBotMenu(),
      audience = "active",
      savedContext = {},
      dragId = null;
    const field = (k) => root.querySelector(`[data-k="${k}"]`);
    function values() {
      const b = {};
      for (const f of BOT_FIELDS) {
        let el = field(f.k),
          v = el.value.trim();
        b[f.k] =
          f.type === "check"
            ? el.checked
            : f.type === "number"
              ? Number(v || f.def)
              : f.type === "days"
                ? (v || f.def).split(/[,\s]+/).map(Number)
                : v;
      }
      b["bot.docs_links"] = [...docs.querySelectorAll(".doc-row")]
        .map((r) => ({
          label: r.querySelector("[data-doc=label]").value.trim(),
          url: r.querySelector("[data-doc=url]").value.trim(),
        }))
        .filter((d) => d.label || d.url);
      b["bot.menu_layout"] = menu.map(entry => ({...entry}));
      return b;
    }
    function changed() {
      drawMenuPreview();
      const change = loaded && JSON.stringify(values()) !== initial;
      save.disabled = !loaded || readonly || saving || !change;
      root.querySelector('#alertsTest').disabled=!loaded||readonly||saving||change||!field('bot.alert_chat_id').value.trim();
      root.querySelector('#alertsRetry').disabled=!loaded||readonly||saving||change||!field('bot.alert_chat_id').value.trim()||!Number(root.querySelector('#alertsRetry').dataset.failed);
      dirty.textContent = readonly
        ? "Режим просмотра"
        : saving
          ? "Сохраняем…"
          : change
            ? "Есть несохранённые изменения"
            : "Все изменения сохранены";
    }
    const menuList=root.querySelector("#menuList");
    function drawMenuPreview() {
      const settings={...savedContext,...values()};
      const entries=botMenuPreview(menu,settings,audience==="active");
      root.querySelector("#menuPreviewBrand").textContent=settings["bot.brand_name"]?.trim() || savedContext["brand.name"] || "Ваш VPN-сервис";
      root.querySelector("#menuPreviewStatus").textContent=audience==="active" ? "Подписка активна" : "Нет активной подписки";
      root.querySelector("#menuPreviewButtons").innerHTML=entries.map(e=>`<div class="bot-menu-preview-button">${e.emoji ? `<span class="bot-menu-emoji-mark" title="Premium-эмодзи: ${esc(e.emoji)}" aria-label="Premium-эмодзи">${I("star",14)}</span>` : ""}${esc(e.emoji ? botIconLabel(e.label) : e.label)}${e.custom ? I("external",14) : ""}</div>`).join("") || '<p class="hint">Для этого клиента все кнопки скрыты.</p>';
      root.querySelector("#menuAdd").disabled=readonly || !loaded || menu.filter(e=>e.id.startsWith("link_")).length>=20;
      root.querySelector("#menuReset").disabled=readonly || !loaded;
    }
    function reorder(id, index) {
      if(readonly || !loaded)return;
      menu=moveBotMenu(menu,id,index);drawMenu();changed();
      const item=menu.find(e=>e.id===id),name=BOT_MENU_BUILTINS.find(b=>b.id===id)?.title || item.label || "Своя кнопка";
      root.querySelector("#menuAnnouncement").textContent=`${name}: позиция ${menu.findIndex(e=>e.id===id)+1} из ${menu.length}`;
    }
    function drawMenu() {
      menuList.innerHTML=menu.map((e,index)=>{
        const builtin=BOT_MENU_BUILTINS.find(b=>b.id===e.id),name=builtin?.title||e.label||"Своя кнопка";
        return `<li class="bot-menu-row ${e.enabled ? "" : "is-muted"}" data-menu-id="${esc(e.id)}"><button class="btn icon-only menu-drag" data-drag draggable="${!readonly}" aria-label="Перетащить ${esc(name)}" title="Перетащить" ${readonly ? "disabled" : ""}>${I("grip",16)}</button><div class="bot-menu-item"><div class="bot-menu-item-title">${I(builtin?.icon || "external",17)}<strong>${esc(builtin?.title || "Своя ссылка")}</strong></div>${builtin ? `<p>${esc(builtin.note)}</p>` : `<div class="bot-menu-link-fields"><label>Название<input class="inp" data-menu-label maxlength="64" value="${esc(e.label || "")}" placeholder="Наш канал" aria-label="Название своей кнопки ${index+1}" ${readonly ? "disabled" : ""}></label><label>Ссылка<input class="inp" data-menu-url maxlength="2048" value="${esc(e.url || "")}" placeholder="https://t.me/…" inputmode="url" aria-label="Ссылка своей кнопки ${index+1}" ${readonly ? "disabled" : ""}></label></div>`}<details class="bot-menu-emoji" ${e.icon_custom_emoji_id ? "open" : ""}><summary>Эмодзи кнопки${e.icon_custom_emoji_id ? " · задано" : ""}</summary><label>Premium ID<input class="inp" data-menu-emoji value="${esc(e.icon_custom_emoji_id || "")}" placeholder="[5215361191051798408]" maxlength="22" inputmode="text" aria-label="ID эмодзи: ${esc(name)}" ${readonly ? "disabled" : ""}></label></details></div><div class="bot-menu-controls"><div class="bot-menu-move"><button class="btn icon-only" data-up aria-label="Выше: ${esc(name)}" ${readonly || index===0 ? "disabled" : ""}><svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="1.8"><path d="m6 14 6-6 6 6"/></svg></button><button class="btn icon-only" data-down aria-label="Ниже: ${esc(name)}" ${readonly || index===menu.length-1 ? "disabled" : ""}>${I("chevD",16)}</button></div><label class="bot-menu-visible"><input type="checkbox" data-menu-enabled aria-label="Показывать ${esc(name)}" ${e.enabled ? "checked" : ""} ${readonly ? "disabled" : ""}><span>Показывать</span></label>${!builtin ? `<button class="btn icon-only danger" data-menu-delete aria-label="Удалить ${esc(name)}" ${readonly ? "disabled" : ""}>${I("trash",14)}</button>` : ""}</div></li>`;
      }).join("");
      menuList.querySelectorAll("[data-menu-id]").forEach(row=>{
        const id=row.dataset.menuId,item=menu.find(e=>e.id===id);
        row.querySelector("[data-menu-label]")?.addEventListener("input",event=>{item.label=event.target.value;changed();});
        row.querySelector("[data-menu-url]")?.addEventListener("input",event=>{item.url=event.target.value.trim();changed();});
        row.querySelector("[data-menu-emoji]").addEventListener("input",event=>{
          const value=event.target.value.trim();
          if(value)item.icon_custom_emoji_id=botEmojiID(value)||value;else delete item.icon_custom_emoji_id;
          changed();
        });
        row.querySelector("[data-menu-enabled]").onchange=event=>{item.enabled=event.target.checked;row.classList.toggle("is-muted",!item.enabled);changed();};
        for(const [selector,delta] of [["[data-up]",-1],["[data-down]",1]])row.querySelector(selector).onclick=()=>{reorder(id,menu.findIndex(e=>e.id===id)+delta);menuList.querySelector(`[data-menu-id="${id}"] ${selector}`)?.focus();};
        row.querySelector("[data-menu-delete]")?.addEventListener("click",()=>{if(readonly)return;menu=menu.filter(e=>e.id!==id);drawMenu();changed();});
        row.querySelector("[data-drag]").ondragstart=event=>{if(readonly || !loaded){event.preventDefault();return;}dragId=id;event.dataTransfer.setData("text/plain",id);event.dataTransfer.effectAllowed="move";row.classList.add("is-dragging");};
        row.ondragover=event=>{if(!dragId || dragId===id)return;event.preventDefault();row.classList.add("drop-target");event.dataTransfer.dropEffect="move";};
        row.ondragleave=()=>row.classList.remove("drop-target");
        row.ondrop=event=>{event.preventDefault();if(dragId && dragId!==id)reorder(dragId,menu.findIndex(e=>e.id===id));dragId=null;};
      });
    }
    menuList.ondragend=()=>{dragId=null;menuList.querySelectorAll(".is-dragging,.drop-target").forEach(r=>r.classList.remove("is-dragging","drop-target"));};
    root.querySelector("#menuAdd").onclick=()=>{
      if(readonly || !loaded || menu.filter(e=>e.id.startsWith("link_")).length>=20)return;
      menu.push({id:"link_"+crypto.randomUUID(),enabled:true,label:"",url:""});drawMenu();changed();menuList.lastElementChild.querySelector("[data-menu-label]").focus();
    };
    root.querySelector("#menuReset").onclick=()=>{
      if(readonly || !loaded)return;
      menu=[...BOT_MENU_BUILTINS.map(b=>menu.find(e=>e.id===b.id)),...menu.filter(e=>e.id.startsWith("link_"))];drawMenu();changed();
    };
    root.querySelectorAll("[data-audience]").forEach(button=>button.onclick=()=>{
      audience=button.dataset.audience;
      root.querySelectorAll("[data-audience]").forEach(b=>{b.classList.toggle("active",b===button);b.setAttribute("aria-pressed",String(b===button));});drawMenuPreview();
    });
    function docRow(label = "", url = "") {
      const row = document.createElement("div");
      row.className = "doc-row";
      row.innerHTML = `<input class="inp" data-doc="label" aria-label="Название документа" maxlength="64" placeholder="Оферта" value="${esc(label)}"><input class="inp" data-doc="url" aria-label="Ссылка на документ" placeholder="https://…" value="${esc(url)}"><button class="btn icon-only" data-del aria-label="Удалить документ">${I("trash", 14)}</button>`;
      row.querySelector("[data-del]").onclick = () => {
        row.remove();
        changed();
      };
      docs.append(row);
    }
    root.querySelector("#docsAdd").onclick = () => {
      if (docs.children.length >= 20) {
        toast("Можно добавить до 20 документов", "err");
        return;
      }
      docRow();
      changed();
    };
    root.addEventListener("input", changed);
    root.addEventListener("change", changed);
    root.querySelectorAll("[data-section]").forEach(
      (button) =>
        (button.onclick = () => {
          root.querySelectorAll("[data-section]").forEach((b) => {
            b.classList.toggle("active", b === button);
            b.setAttribute("aria-pressed", String(b === button));
          });
          root
            .querySelectorAll("[data-panel]")
            .forEach(
              (p) => (p.hidden = p.dataset.panel !== button.dataset.section),
            );
        }),
    );
    const check = async () => {
      const btn = root.querySelector("#botCheck");
      btn.disabled = true;
      status.textContent = "Проверяем Telegram и обработку сообщений…";
      try {
        const r = await API.call("/api/bot/status");
        status.innerHTML = `<div class="notice ${r.ok && r.polling ? "ok" : "warn"}">${r.ok ? `@${esc(r.username)} · Telegram доступен. ${r.polling ? "Бот получает обновления." : "Свежего ответа процесса нет — проверьте службу sn-bot."}` : esc(r.error || "Telegram недоступен")}${r.channel_ok === false ? "<br>Канал недоступен для проверки: " + esc(r.channel_error || "боту нужны права администратора") : ""}</div>`;
      } catch (e) {
        status.textContent = "Проверка не завершилась: " + e.message;
      } finally {
        btn.disabled = false;
      }
    };
    root.querySelector("#botCheck").onclick = check;
    root.querySelector("#botHelp").onclick = () => {
      const d = document.createElement("dialog");
      d.className = "bot-help";
      d.setAttribute("aria-labelledby", "botHelpTitle");
      d.innerHTML = `<h2 id="botHelpTitle">Как подготовить бота</h2><ol><li>Создайте бота через @BotFather и задайте BOT_TOKEN при установке панели. Токен общий для API, бота, платежей и уведомлений.</li><li>Добавьте тарифы, цены и способы оплаты в разделах продаж.</li><li>Задайте название, контакты поддержки и документы.</li><li>В «Кнопки меню» меняйте порядок перетаскиванием или стрелками, добавляйте свои ссылки и проверяйте результат для двух состояний клиента. Сохранённые настройки применяются при следующем /start или возврате в меню. Уже отправленное сообщение обновляется после действия клиента.</li><li>Включите кнопку Mini App. Сначала установите кабинет и выберите его сервер в «Кабинет и Mini App». Приложение работает на домене кабинета.</li><li>В @BotFather можно отдельно настроить главное Mini App, экран загрузки и разрешённый домен.</li><li>Нажмите «Проверить бота». Бот читает сохранённые настройки при каждом новом действии клиента; напоминания обновляются в течение минуты.</li></ol><p>Команды клиента: /start, /buy, /addons, /sub, /docs, /support, /paysupport и /cancel. Кнопки показывают доступные действия по текущей подписке.</p><p>В Mini App доступны проверка оплаты, промокоды, история платежей, устройства и общая переписка с поддержкой. Деньги и лимиты задаются в тарифах и платёжных модулях.</p><button class="btn primary">Понятно</button>`;
      document.body.append(d);
      d.querySelector("button").onclick = () => d.close();
      d.onclose = () => d.remove();
      d.showModal();
    };
    let alertsBusy=false;
    const alertsState=root.querySelector('#alertsStatus');
    const updateAlerts=async()=>{
      try {
        const data=await API.call('/api/bot/alerts');
        alertsState.innerHTML=`<div class="bot-alert-metrics"><span>В очереди <b>${Number(data.pending)||0}</b></span><span>Ошибок за сутки <b>${Number(data.failed)||0}</b></span></div><p class="hint">Последняя доставка: ${data.last_sent?esc(new Date(data.last_sent).toLocaleString()):'ещё не было'}</p>${data.last_error?`<p class="notice error">${esc(data.last_error)}</p>`:''}`;
        root.querySelector('#alertsRetry').dataset.failed=String(data.failed||0);changed();
      } catch(e){alertsState.textContent='Не удалось получить статус: '+e.message;}
    };
    root.querySelector('#alertsRefresh').onclick=updateAlerts;
    root.querySelector('#alertsTest').onclick=async()=>{
      if(alertsBusy||readonly)return;alertsBusy=true;root.querySelector('#alertsTest').disabled=true;
      try{await API.call('/api/bot/alerts/test',{method:'POST',body:{}});toast('Тестовое уведомление отправлено в группу');}
      catch(e){toast(e.message,'err');alertsState.textContent=e.message;}
      finally{alertsBusy=false;changed();}
    };
    root.querySelector('#alertsRetry').onclick=async()=>{
      if(alertsBusy||readonly)return;alertsBusy=true;root.querySelector('#alertsRetry').disabled=true;
      try{const r=await API.call('/api/bot/alerts/retry',{method:'POST',body:{}});toast('В очередь возвращено: '+r.queued);await updateAlerts();}
      catch(e){toast(e.message,'err');alertsState.textContent='Повтор не выполнен: '+e.message;}finally{alertsBusy=false;changed();}
    };
    updateAlerts();
    const load = async () => {
      save.disabled = true;
      try {
        const saved = await API.call("/api/settings");
        const cabinet=await API.call("/api/cabinet-service");
        saved["_cabinet.miniapp_url"]=cabinet.miniapp_url;
        savedContext = saved;
        menu = normalizeBotMenu(saved["bot.menu_layout"]);
        for (const f of BOT_FIELDS) {
          const el = field(f.k),
            v = saved[f.k];
          if (f.type === "check")
            el.checked = v == null ? f.def === true : v === true;
          else el.value = Array.isArray(v) ? v.join(", ") : (v ?? "");
        }
        docs.replaceChildren();
        (saved["bot.docs_links"] || []).forEach((d) => docRow(d.label, d.url));
        loaded = true;
        drawMenu();
        initial = JSON.stringify(values());
        root.querySelector("#botLoadError").replaceChildren();
        if (readonly)
          root
            .querySelectorAll("input,textarea,#docsAdd,[data-del]")
            .forEach((el) => (el.disabled = true));
        changed();
      } catch (e) {
        root.querySelector("#botLoadError").innerHTML =
          `<div class="notice err">Не удалось загрузить настройки: ${esc(e.message)} <button class="btn sm" id="botRetry">Повторить</button></div>`;
        root.querySelector("#botRetry").onclick = load;
        dirty.textContent = "Настройки не загружены";
      }
    };
    save.onclick = async () => {
      const body = values();
      if(!loaded || readonly || saving)return;
      root.querySelector("#menuError").textContent="";
      const invalidEmoji=menu.find(e=>e.icon_custom_emoji_id && !botEmojiID(e.icon_custom_emoji_id));
      if(invalidEmoji){
        root.querySelector("[data-section=menu]").click();root.querySelector("#menuError").textContent="ID эмодзи должен содержать только цифры. Можно вставить его в квадратных скобках.";
        const input=menuList.querySelector(`[data-menu-id="${invalidEmoji.id}"] [data-menu-emoji]`);input.closest("details").open=true;input.focus();return;
      }
      const invalidButton=menu.find(e=>e.id.startsWith("link_") && (!e.label.trim() || [...e.label].length>64 || /[\x00-\x1f\x7f]/u.test(e.label) || !botMenuURL(e.url)));
      if(invalidButton){
        root.querySelector("[data-section=menu]").click();
        root.querySelector("#menuError").textContent="У своей кнопки нужны название до 64 символов и ссылка https://, http:// или tg://.";
        menuList.querySelector(`[data-menu-id="${invalidButton.id}"] ${invalidButton.label.trim() ? "[data-menu-url]" : "[data-menu-label]"}`).focus();return;
      }
      const bad = body["bot.docs_links"].find((d) => !d.label || !d.url);
      if (bad) {
        toast("У документа нужны название и ссылка", "err");
        root.querySelector("[data-section=support]").click();
        return;
      }
      saving = true;
      changed();
      try {
        await API.call("/api/settings", { method: "PATCH", body });
        initial = JSON.stringify(body);
        toast("Настройки бота и Mini App сохранены");
      } catch (e) {
        toast(e.message, "err");
      } finally {
        saving = false;
        changed();
      }
    };
    await load();
    check();
  },
});
