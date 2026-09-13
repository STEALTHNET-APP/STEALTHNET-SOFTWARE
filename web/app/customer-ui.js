"use strict";
let cabinetCsrf = "", freshCode = "", savedCode = true, selectedOffer = null;
const UI_PATHS = {
  user:'<circle cx="12" cy="6.5" r="3.5"/><path d="M5.2 21v-2.4a6.8 6.8 0 0 1 13.6 0V21"/>',
  devices:'<path d="M13 18H3a1.5 1.5 0 0 1-1.5-1.5v-12A1.5 1.5 0 0 1 3 3h14a1.5 1.5 0 0 1 1.5 1.5V7"/><rect x="15" y="9" width="7" height="13" rx="1.5"/>',
  traffic:'<ellipse cx="12" cy="5" rx="8" ry="3"/><path d="M4 5v14c0 4 16 4 16 0V5M4 12c0 4 16 4 16 0"/>',
  house:'<path d="m3 10 9-8 9 8v11h-6v-7H9v7H3Z"/>',
  phone:'<rect x="7" y="2" width="10" height="20" rx="2"/><path d="M11 18h2"/>',
  family:'<circle cx="12" cy="7" r="3.5"/><path d="M6.5 21v-3a5.5 5.5 0 0 1 11 0v3M5 5a3 3 0 0 0 0 6M19 5a3 3 0 0 1 0 6M2 20v-3a4 4 0 0 1 3-4M22 20v-3a4 4 0 0 0-3-4"/>',
  key:'<path d="M10.5 13.5a6 6 0 1 1 4 1.5l-2 2H10v3H7v2H3v-4Z"/><circle cx="16.5" cy="6.5" r=".7"/>',
  arrow:'<path d="M5 12h14m-5-5 5 5-5 5"/>',
  chevron:'<path d="m9 5 7 7-7 7"/>',
  check:'<path d="m6 12 4 4 8-9"/>',
  refresh:'<path d="M20 8A8 8 0 1 0 20 16M20 3v5h-5"/>',
  logout:'<path d="M9 3H4v18h5m-1-9h13m-5-5 5 5-5 5"/>',
  copy:'<rect x="8" y="8" width="13" height="13" rx="2"/><path d="M16 8V3H3v13h5"/>'
};
const uiIcon = key => `<svg viewBox="0 0 24 24" aria-hidden="true">${UI_PATHS[key] || ICON[key] || ""}</svg>`;
function applyCustomerBrand() {
  document.title = cabinetMode ? config.seo_title : config.brand;
  const brand=document.querySelector("#brand");
  brand.innerHTML=safeURL(config.logo)?`<img class="${config.logo_dark?'brand-light':''}" src="${esc(config.logo)}" alt="${esc(config.brand)}">${config.logo_dark?`<img class="brand-dark" src="${esc(config.logo_dark)}" alt="${esc(config.brand)}">`:''}`:esc(config.brand);
  for(const [key,token] of [["accent","--a1"],["accent_end","--a2"]]) if(/^#[0-9a-f]{6}$/i.test(config[key]||""))document.documentElement.style.setProperty(token,config[key]);
  if(config.favicon){let link=document.querySelector('link[rel="icon"]');if(!link){link=document.createElement('link');link.rel='icon';document.head.append(link);}link.href=config.favicon;}
}
function mountCustomerHeader() {
  const header=document.querySelector('.app-header');header.hidden=false;
  let actions=header.querySelector('.header-actions');
  if(!actions){actions=document.createElement('div');actions.className='header-actions';header.append(actions);actions.append(document.querySelector('#refresh'));}
  const refresh=document.querySelector('#refresh');refresh.innerHTML=uiIcon('refresh');refresh.className='icon-btn';
  let theme=header.querySelector('.theme-trigger');
  if(!theme){theme=document.createElement('button');theme.className='theme-trigger icon-btn';theme.onclick=showThemeSettings;actions.append(theme);}
  let profile=header.querySelector('#profile');
  if(!profile){profile=document.createElement('button');profile.id='profile';profile.className='icon-btn';profile.setAttribute('aria-label',CT.html('Аккаунт'));profile.innerHTML=uiIcon('user');actions.append(profile);}
  profile.onclick=()=>cabinetMode&&!me?showLogin():drawAccount();
  refresh.hidden=cabinetMode&&!me;profile.hidden=false;
  const language=header.querySelector('.language-trigger');if(language&&language.parentElement!==actions)actions.append(language);
  CT.mount(actions);
  applyCustomerBrand();
  let heading=header.querySelector('.cabinet-heading');if(cabinetMode&&me&&!heading){heading=document.createElement('span');heading.className='cabinet-heading';heading.textContent=CT.html('Ваш кабинет');header.insertBefore(heading,actions);}if(heading)heading.hidden=!me;
  syncTheme();
}
function drawNav() {
  if(cabinetMode&&!me){tabs.hidden=true;return;}
  tabs.hidden=false;
  const items=[["home",CT.html("Главная")],[config.shop_enabled?"shop":"payments",CT.html("Платежи")],...(config.referral_enabled?[["ref",CT.html("Пригласить")]]:[]),["help",CT.html("Поддержка")]];
  tabs.innerHTML=items.map(([id,label])=>`<button class="tab ${section===id||(id==='shop'&&section==='payments')?'on':''}" data-nav="${id}" aria-current="${section===id?'page':'false'}">${uiIcon(id==='payments'?'shop':id)}<span>${label}</span></button>`).join('');
  tabs.querySelectorAll('[data-nav]').forEach(b=>b.onclick=()=>navigate(b.dataset.nav));
  if(cabinetMode){
    const nav=document.querySelector('#desktopNav');nav.hidden=false;
    nav.innerHTML=CT.html`<a class="desktop-brand" href="/">${safeURL(config.logo)?`<img class="${config.logo_dark?'brand-light':''}" src="${esc(config.logo)}" alt="${esc(config.brand)}">${config.logo_dark?`<img class="brand-dark" src="${esc(config.logo_dark)}" alt="${esc(config.brand)}">`:''}`:esc(config.brand)}</a><nav aria-label="Кабинет">${[['home',CT.html('Главная'),'house'],...(config.shop_enabled?[['shop',CT.html('Тарифы и покупки'),'shop']]:[]),...(config.devices_enabled?[['devices',CT.html('Устройства'),'devices']]:[]),['payments',CT.html('Платежи'),'shop'],...(config.referral_enabled?[['ref',CT.html('Приглашения'),'ref']]:[]),['help',CT.html('Поддержка'),'help']].map(([id,label,icon])=>`<button class="${section===id?'selected':''}" data-desk="${id}">${uiIcon(icon)}<span>${label}</span></button>`).join('')}</nav><div class="desktop-account"><button id="deskAccount">${uiIcon('user')}Аккаунт</button><button id="deskLogout">${uiIcon('logout')}Выйти</button></div>`;
    nav.querySelectorAll('[data-desk]').forEach(b=>b.onclick=()=>b.dataset.desk==='devices'?drawDevices():navigate(b.dataset.desk));
    nav.querySelector('#deskAccount').onclick=drawAccount;nav.querySelector('#deskLogout').onclick=signOut;
  }
}
function drawHome() {
  const active=me.status==='active',limited=me.status==='limited',hasPlan=!!me.tariff;
  const used=me.used==null?null:Number(me.used),limit=me.limit==null?null:Number(me.limit);
  const remaining=limit===null||used===null?null:Math.max(0,limit-used),pct=limit>0&&used!==null?Math.min(100,100*used/limit):0;
  const ending=me.expires_at?new Date(me.expires_at).toLocaleDateString(CT.locale,{day:'numeric',month:'long'}):null;
  const title=active?CT.html('Подключим ваше устройство'):limited?CT.html('Закончился трафик'):me.status==='disabled'?CT.html('Доступ приостановлен'):hasPlan?CT.html('Продлим вашу подписку'):CT.html('Начнём с выбора тарифа');
  const help=active?CT.html`Установите приложение и подключитесь к защищённой сети ${config.brand}.`:limited?CT.html('Дождитесь обновления лимита или выберите дополнительный пакет.'):me.status==='disabled'?CT.html('Обратитесь в поддержку, чтобы уточнить причину.'):CT.html('Выберите подходящий срок и количество устройств. После оплаты здесь появится подключение.');
  root.innerHTML=CT.html`<div class="customer-dashboard"><div class="dashboard-main"><div class="subscription-status ${active?'is-active':''}"><i></i><span>${hasPlan?CT.html('Подписка ')+esc((statusLabel[me.status]||'').toLocaleLowerCase(CT.locale)): CT.html('Подписка не оформлена')}${active&&ending?CT.html(' · до ')+ending:''}</span></div><section class="connect-card"><h1>${title}</h1><p>${help}</p><div class="connect-actions"><button class="btn" id="primaryStep">${active?CT.html('Подключиться'):limited?CT.html('Посмотреть пакеты'):me.status==='disabled'?CT.html('Написать в поддержку'):config.shop_enabled?CT.html('Выбрать тариф'):CT.html('Написать в поддержку')}${uiIcon('arrow')}</button>${active&&me.sub_url?CT.html`<button class="btn line copy-sub" id="copySub">${uiIcon('copy')}Скопировать ссылку</button>`:''}</div><ol class="connection-steps" aria-label="Этапы подключения">${[[CT.html('Тариф'),hasPlan],[CT.html('Доступ'),active||limited],[CT.html('Подключение'),false]].map(([label,done],i)=>`<li class="${done?'complete':(active&&i===2)||(!hasPlan&&i===0)?'current':''}"><span>${done?uiIcon('check'):active&&i===2?'':i+1}</span><b>${label}</b></li>`).join('')}</ol></section><div class="resource-grid"><section class="resource-card"><div class="resource-heading"><span class="round-icon">${uiIcon('devices')}</span><h2>Устройства</h2></div><button class="resource-value" id="devicesValue" ${!config.devices_enabled?'disabled':''}><strong>${me.devices??'—'}${me.device_limit?CT.html('<span class="device-separator"><span class="mobile-only"> / </span><span class="desktop-only"> из </span></span>')+me.device_limit:''}</strong>${!me.device_limit?CT.html('<span>без лимита</span>'):''}${uiIcon('chevron')}</button><p>Используется устройств</p><button class="btn soft" id="deviceAddon" hidden>Добавить<span class="desktop-only"> устройство</span></button></section><section class="resource-card"><div class="resource-heading"><span class="round-icon cyan">${uiIcon('traffic')}</span><h2>Трафик</h2></div><button class="resource-value" id="trafficValue"><strong class="mobile-only">${limit===null?(hasPlan?CT.html('Безлимит'):'—'):used===null?CT.html('Нет данных'):bytes(used).split(' ').at(-1)===bytes(limit).split(' ').at(-1)?bytes(used).split(' ')[0]:bytes(used)}</strong>${limit!==null?CT.html`<span class="mobile-only">из ${bytes(limit)}</span>`:''}<strong class="desktop-only">${remaining===null?(hasPlan?CT.html('Безлимит'):'—'):bytes(remaining)}</strong>${remaining!==null?CT.html('<span class="desktop-only">осталось</span>'):''}${uiIcon('chevron')}</button>${limit!==null&&used!==null?CT.html`<div class="resource-track" role="progressbar" aria-label="Использовано трафика" aria-valuenow="${Math.round(pct)}" aria-valuemin="0" aria-valuemax="100"><i style="width:${pct}%"></i></div>`:''}<p>${remaining===null?(hasPlan?CT.html('Без ограничения трафика'):CT.html('Выберите тариф')): CT.html('<span class="mobile-only">Осталось ')+bytes(remaining)+CT.html('</span><span class="desktop-only">из ')+bytes(limit)+'</span>'}</p><button class="btn soft" id="trafficAddon" hidden>Докупить<span class="desktop-only"> трафик</span></button></section></div><div id="grantsEntry"></div></div><aside class="dashboard-side"><section class="subscription-summary"><div><h2>Ваша подписка</h2><span class="state-label ${active?'is-active':''}"><i></i>${esc(statusLabel[me.status]||CT.html('Не оформлена'))}</span></div><div class="subscription-inset"><strong>${esc(me.tariff||CT.html('Нет тарифа'))}</strong><p>${me.expires_at?CT.html('до ')+date(me.expires_at):hasPlan?CT.html('Без ограничения срока'):CT.html('Доступ появится после оформления')}</p></div>${config.shop_enabled?'<button class="btn soft" id="renew">'+(hasPlan?CT.html('Продлить'):CT.html('Выбрать тариф'))+'</button>':''}</section><button class="access-card" id="accessCode"><span class="round-icon">${uiIcon('key')}</span><span><strong>Код доступа</strong><b class="masked-code" aria-hidden="true">•••• •••• •••• ••••</b><small><span class="mobile-only">Управление</span><span class="desktop-only">Управление доступом</span></small></span>${uiIcon('chevron')}</button></aside><button class="support-card" id="connectHelp"><span class="round-icon blue">${uiIcon('help')}</span><span><strong>Нужна помощь с подключением?</strong><small>Инструкции, ответы на вопросы и поддержка.</small></span><span class="support-action desktop-only">Открыть инструкции${uiIcon('chevron')}</span><span class="mobile-only support-chevron">${uiIcon('chevron')}</span></button></div>`;
  root.querySelector('#primaryStep').onclick=()=>active?openURL(me.sub_url):limited?drawAddons('traffic'):navigate(me.status==='disabled'||!config.shop_enabled?'help':'shop');
  root.querySelector('#copySub')?.addEventListener('click',()=>copy(me.sub_url));
  root.querySelector('#devicesValue').onclick=drawDevices;
  root.querySelector('#trafficValue').onclick=()=>{showDialog(CT.html('Ваш трафик'),`<p>${remaining===null?CT.html('У тарифа нет ограничения трафика.'):CT.html('Осталось ')+bytes(remaining)+CT.html(' из ')+bytes(limit)+'.'}</p>${me.traffic_reset_at&&me.reset_strategy!=='no_reset'?CT.html('<p>Лимит обновится ')+date(me.traffic_reset_at)+'.</p>':''}${addonCatalog?.items?.some(p=>p.kind==='traffic')?CT.html('<button class="btn" id="trafficMore">Докупить трафик</button>'):''}`);dialog.querySelector('#trafficMore')?.addEventListener('click',()=>{dialog.close();drawAddons('traffic');});};
  root.querySelector('#renew')?.addEventListener('click',()=>navigate('shop'));
  root.querySelector('#accessCode').onclick=showAccess;
  root.querySelector('#connectHelp').onclick=()=>navigate('help');
  if(config.shop_enabled) mountGuidedAddons();
  mountCustomerHeader();
}
async function mountGuidedAddons(){
  const box=root.querySelector('.resource-grid');
  try{const catalog=await api('/addons');if(!box?.isConnected)return;addonCatalog=catalog;
    for(const [kind,id] of [['devices','deviceAddon'],['traffic','trafficAddon']]){const button=root.querySelector('#'+id);button.hidden=!catalog.items.some(p=>p.kind===kind);button.onclick=()=>drawAddons(kind);}
    if(catalog.grants.length){const entry=root.querySelector('#grantsEntry');entry.innerHTML=CT.html('<button class="addon-history" id="myGrants">Мои докупки</button>');entry.firstChild.onclick=()=>drawAddons();}
  }catch(e){if(box?.isConnected){const entry=root.querySelector('#grantsEntry');entry.innerHTML=CT.html('<button class="addon-history" id="retryGrants">Повторить загрузку докупок</button>');entry.firstChild.onclick=mountGuidedAddons;}}
}
function drawAccount(){
  legacyDrawAccount();
  const body=dialog.querySelector('.dialog-body');const actions=document.createElement('div');actions.className='btns account-extra';
  actions.innerHTML=CT.html`<button class="btn line" id="accountTheme">Оформление</button>${me.sub_url?CT.html('<button class="btn line" id="accountCopySub">Скопировать ссылку подключения</button>'):''}<button class="btn soft" id="accountAccess">${uiIcon('key')}Код доступа к сайту</button>${cabinetMode?CT.html('<button class="btn line" id="linkTelegram">Связать с Telegram</button><button class="btn line" id="logoutAll">Выйти из всех сеансов сайта</button><button class="btn line" id="logout">Выйти</button>'):''}`;body.append(actions);
  actions.querySelector('#accountAccess').onclick=showAccess;
  actions.querySelector('#accountTheme').onclick=showThemeSettings;
  actions.querySelector('#accountCopySub')?.addEventListener('click',()=>copy(me.sub_url));
  actions.querySelector('#linkTelegram')?.addEventListener('click',async e=>{e.target.disabled=true;try{const r=await post('/auth/telegram');openURL(r.url);notice(CT.html('Подтвердите привязку в боте, затем обновите кабинет.'));}catch(err){notice(err.message);}finally{e.target.disabled=false;}});
  actions.querySelector('#logout')?.addEventListener('click',()=>signOut());
  actions.querySelector('#logoutAll')?.addEventListener('click',()=>signOut(true));
}
async function showAccess(){
  showDialog(CT.html('Код доступа'),CT.html('<p>Загружаем код доступа…</p>'));
  try {
    const status=cabinetMode?{issued:true,sites:[]}:await api('/access');
    const result=status.issued?await post(cabinetMode?'/auth/code':'/access/reveal'):{code:null};
    const sites=status.sites?.length?`<div class="document-links">${status.sites.map(s=>`<a class="document-link" href="${esc(s)}" target="_blank" rel="noopener noreferrer"><span>${esc(new URL(s).hostname)}</span>${uiIcon('arrow')}</a>`).join('')}</div>`:'';
    if(result.code){
      showDialog(CT.html('Код доступа'),codeMarkup(result.code,false)+sites+CT.html('<details class="recovery-help"><summary>Заменить код доступа</summary><p>Старый код перестанет работать, остальные сеансы сайта завершатся. Подписка и покупки сохранятся.</p><button class="btn line" id="replaceCode">Выпустить новый код</button><p id="codeError" role="alert"></p></details>'));
      wireCode(result.code,false);
      dialog.querySelector('#replaceCode').onclick=e=>replaceAccessCode(e.currentTarget,result.code);
    }else if(status.issued){
      showDialog(CT.html('Код доступа'),CT.html`<p>Этот код был создан до появления повторного просмотра. ${cabinetMode?CT.html('Введите его один раз — после этого вы сможете смотреть и копировать его здесь.'):CT.html('Войдите на сайт по вашему коду — после этого он будет доступен здесь. Или выпустите новый код для этого же аккаунта.')}</p>${sites}${cabinetMode?CT.html('<form id="rememberCode"><label class="field-label" for="existingCode">Ваш код доступа</label><input class="code-input" id="existingCode" autocomplete="off" spellcheck="false" autocapitalize="characters" required><button class="btn" type="submit">Сохранить для просмотра</button></form>'):''}<button class="btn line" id="replaceCode">Выпустить новый код</button><p id="codeError" role="alert"></p>`);
      dialog.querySelector('#rememberCode')?.addEventListener('submit',async e=>{e.preventDefault();const b=e.currentTarget.querySelector('button');b.disabled=true;try{await post('/auth/code-remember',{code:dialog.querySelector('#existingCode').value});await showAccess();}catch(err){dialog.querySelector('#codeError').textContent=err.status===401?CT.html('Код не подошёл. Проверьте символы и попробуйте ещё раз.'):err.message;b.disabled=false;}});
      dialog.querySelector('#replaceCode').onclick=e=>replaceAccessCode(e.currentTarget,cabinetMode?dialog.querySelector('#existingCode').value:'');
    }else{
      showDialog(CT.html('Код доступа'),CT.html`<p>Получите код, чтобы входить на сайт в этот же аккаунт. Подписка и покупки будут общими с Telegram.</p>${sites}<button class="btn" id="issueCode">Получить код</button><p id="codeError" role="alert"></p>`);
      dialog.querySelector('#issueCode').onclick=async e=>{const b=e.currentTarget;b.disabled=true;try{await post('/access',{replace:false});await showAccess();}catch(err){dialog.querySelector('#codeError').textContent=err.message;b.disabled=false;}};
    }
  }catch(e){showDialog(CT.html('Код доступа'),CT.html`<p role="alert">${esc(e.message)}</p><button class="btn line" id="retryCode">Повторить</button>`);dialog.querySelector('#retryCode').onclick=showAccess;}
}
async function replaceAccessCode(button,code){
  if(!confirm(CT.html('Заменить код? Старый код перестанет работать, остальные сеансы сайта завершатся.')))return;
  button.disabled=true;
  try{
    const r=await post(cabinetMode?'/auth/rotate':'/access',cabinetMode?{code}:{replace:true});
    if(cabinetMode){cabinetCsrf=r.csrf;savedCode=false;freshCode=r.code;dialog.close();renderSaveCode();}
    else await showAccess();
  }catch(err){dialog.querySelector('#codeError').textContent=err.status===401?CT.html('Для замены введите текущий код. Если он потерян, выпустите новый в привязанном Mini App.'):err.message;button.disabled=false;}
}
function codeMarkup(code,registration){return CT.html`<p>Этот код открывает ваш аккаунт. Сохраните его и не передавайте другим.</p><div class="secret-code" aria-label="Код доступа"><bdi>${esc(code)}</bdi></div><div class="code-actions"><button class="btn soft" id="copyCode">${uiIcon('copy')}Скопировать код</button><button class="btn line" id="downloadCode">Скачать</button></div><p class="code-feedback" id="codeFeedback" role="status" aria-live="polite" aria-atomic="true"></p>${registration?CT.html('<label class="code-confirm"><input id="codeStored" type="checkbox">Я сохранил код доступа</label><button id="continueCode" class="btn" disabled>Перейти в кабинет</button><p id="codeError" role="alert"></p>'):CT.html('<button class="text-button" id="hideCode" aria-pressed="false">Скрыть код</button><p class="note">Код можно посмотреть снова в разделе «Код доступа».</p>')}`;}
function wireCode(code,registration){const scope=registration?root:dialog;
  const feedback=scope.querySelector('#codeFeedback'),copyButton=scope.querySelector('#copyCode');
  copyButton.onclick=async()=>{
    copyButton.disabled=true;feedback.textContent='';feedback.classList.remove('error');
    try{await navigator.clipboard.writeText(code);copyButton.innerHTML=uiIcon('check')+CT.html('Код скопирован');copyButton.classList.add('copied');feedback.textContent=CT.html('Код скопирован в буфер обмена.');try{tg?.HapticFeedback?.notificationOccurred?.('success');}catch{}}
    catch{copyButton.innerHTML=uiIcon('copy')+CT.html('Скопировать код');copyButton.classList.remove('copied');feedback.textContent=CT.html('Не удалось скопировать. Выделите код и скопируйте его вручную.');feedback.classList.add('error');const box=scope.querySelector('.secret-code');box.textContent=code;const range=document.createRange();range.selectNodeContents(box);const selection=window.getSelection();selection.removeAllRanges();selection.addRange(range);const hide=scope.querySelector('#hideCode');if(hide){hide.textContent=CT.html('Скрыть код');hide.setAttribute('aria-pressed','false');}}
    finally{copyButton.disabled=false;}
  };
  scope.querySelector('#downloadCode').onclick=()=>{const blob=new Blob([CT.html`${config.brand}\nКод доступа: ${code}\n${cabinetMode?location.origin:''}\nХраните код в безопасном месте.\n`],{type:'text/plain;charset=utf-8'});const url=URL.createObjectURL(blob),a=document.createElement('a');a.href=url;a.download='access-code.txt';a.click();setTimeout(()=>URL.revokeObjectURL(url),1000);feedback.textContent=CT.html('Файл с кодом подготовлен для скачивания.');};
  scope.querySelector('#hideCode')?.addEventListener('click',e=>{const b=e.currentTarget,hide=b.getAttribute('aria-pressed')!=='true';b.setAttribute('aria-pressed',String(hide));b.textContent=hide?CT.html('Показать код'):CT.html('Скрыть код');scope.querySelector('.secret-code').textContent=hide?'•••• •••• •••• •••• ••••':code;});
  if(registration){scope.querySelector('#codeStored').onchange=e=>scope.querySelector('#continueCode').disabled=!e.target.checked;scope.querySelector('#continueCode').onclick=async e=>{e.target.disabled=true;try{await post('/auth/code-saved');freshCode='';savedCode=true;sessionStorage.removeItem('sn.cabinet.register');await enterCabinet();}catch(err){scope.querySelector('#codeError').textContent=err.message;e.target.disabled=false;}};}
}
function renderSaveCode(){tabs.hidden=true;document.querySelector('#desktopNav').hidden=true;document.body.className='public-site';root.innerHTML=CT.html`<section class="auth-card"><h1>Сохраните код доступа</h1>${codeMarkup(freshCode,true)}</section>`;wireCode(freshCode,true);}
async function cabinetBoot(){
  try{config=await api('/config');applyCustomerBrand();mountCustomerHeader();
    try{const auth=await api('/auth/session');cabinetCsrf=auth.csrf;savedCode=auth.code_saved;await enterCabinet();}
    catch(e){if(e.status!==401)throw e;await renderStorefront();if(location.hash==='#login')showLogin();}
  }catch(e){errorBox(e,cabinetBoot);}
}
async function enterCabinet(){
  me=await api('/me');shop=null;document.body.className='customer-site';
  if(!savedCode&&!freshCode){const r=await post('/auth/code');freshCode=r.code||'';}
  if(!savedCode){if(freshCode){renderSaveCode();return;}showDialog(CT.html('Сохраните код'),CT.html`<p>Сохраните код, с которым вы вошли. Если не успели его записать, выпустите новый.</p><div class="btns"><button class="btn" id="savedExisting">Код сохранён</button><button class="btn line" id="replaceLost">Получить новый код</button></div>`);dialog.querySelector('#savedExisting').onclick=async()=>{try{await post('/auth/code-saved');savedCode=true;dialog.close();await enterCabinet();}catch(e){notice(e.message);}};dialog.querySelector('#replaceLost').onclick=async()=>{try{const r=await post('/auth/rotate',{code:''});cabinetCsrf=r.csrf;freshCode=r.code;dialog.close();renderSaveCode();}catch(e){notice(e.message);}};}
  navigate(location.hash==='#payments'?'payments':'home');
  if(selectedOffer&&savedCode){const offer=selectedOffer;selectedOffer=null;await drawShop(generation);section='shop';drawNav();buySheet(offer.id,offer.days);}
}
async function signOut(all=false){try{await post(all?'/auth/logout-all':'/auth/logout');dialog.close();me=null;cabinetCsrf='';freshCode='';await renderStorefront();}catch(e){notice(e.message);}}
function showLogin(){
  showDialog(CT.html('Войти в кабинет'),CT.html`<p>Введите код доступа из Mini App или код, который получили при регистрации на сайте.</p><form id="codeLogin"><label class="field-label" for="loginCode">Код доступа</label><input id="loginCode" class="code-input" autocomplete="off" spellcheck="false" autocapitalize="characters" required><button class="btn" type="submit">Войти</button><p id="loginError" role="alert"></p></form>${config.registration_enabled?CT.html('<button class="btn soft" id="newAccount">Зарегистрироваться</button>'):''}<details class="recovery-help"><summary>Где взять код или как восстановить доступ?</summary><p>Откройте Mini App бота и нажмите «Код доступа». В нём можно посмотреть код вашего аккаунта или выпустить новый.</p>${config.bot?CT.html('<button class="btn line" id="loginBot">Открыть бота</button>'):''}${config.support_url?'<a class="document-link" href="'+esc(config.support_url)+CT.html('">Написать в поддержку</a>'):''}</details>`);
  dialog.querySelector('#codeLogin').onsubmit=async e=>{e.preventDefault();const b=e.target.querySelector('button');b.disabled=true;try{const r=await post('/auth/login',{code:dialog.querySelector('#loginCode').value});cabinetCsrf=r.csrf;savedCode=r.code_saved;dialog.close();await enterCabinet();}catch(err){if(dialog.querySelector('#loginError'))dialog.querySelector('#loginError').textContent=err.status===401?CT.html('Код не подошёл. Проверьте его и попробуйте ещё раз.'):err.message;b.disabled=false;}};
  dialog.querySelector('#newAccount')?.addEventListener('click',registerAccount);
  dialog.querySelector('#loginBot')?.addEventListener('click',()=>openURL('https://t.me/'+config.bot));
}
async function registerAccount(e){const button=e?.currentTarget;try{if(button)button.disabled=true;let key=sessionStorage.getItem('sn.cabinet.register');if(!key){key=Array.from(crypto.getRandomValues(new Uint8Array(32)),b=>b.toString(16).padStart(2,'0')).join('');sessionStorage.setItem('sn.cabinet.register',key);}const r=await post('/auth/register',{request_key:key});cabinetCsrf=r.csrf;freshCode=r.code;savedCode=false;dialog.close();renderSaveCode();}catch(err){notice(err.message);if(button)button.disabled=false;}}
async function renderStorefront(){
  me=null;tabs.hidden=true;document.querySelector('#desktopNav').hidden=true;document.body.className='public-site';mountCustomerHeader();
  if(CT.locale==='en'&&!config.language_content_ready){root.innerHTML='<section class="auth-card"><h1>'+CT.text('Содержимое на английском ещё не опубликовано.')+'</h1><button class="btn" id="publicLogin">'+CT.text('Войти по коду')+'</button></section>';root.querySelector('#publicLogin').onclick=showLogin;return;}
  const catalog=await api('/catalog');currency=catalog.currency;
  const plans=catalog.tariffs.map(t=>{const price=t.prices.filter(p=>p.currency===currency).sort((a,b)=>a.days-b.days)[0];if(!price)return '';return CT.html`<article class="public-plan"><div class="public-plan-heading"><span class="round-icon ${t.device_limit===1?'':t.device_limit<=3?'blue':'cyan'}">${uiIcon(t.device_limit===1?'phone':t.device_limit<=3?'devices':'family')}</span><div><h3>${esc(t.title)}</h3>${t.description?`<p>${esc(t.description)}</p>`:''}</div></div><div class="public-plan-spec">${deviceLabel(t.device_limit)}${t.traffic_limit_bytes==null?CT.html(' · Безлимитный трафик'):' · '+bytes(t.traffic_limit_bytes)}</div><div class="public-price"><strong>${price.amount_minor===0?CT.html('Бесплатно'):money(price.amount_minor,currency)}</strong><span>за ${duration(price.days)}</span></div><button class="btn" data-public-plan="${t.id}" data-days="${price.days}">Выбрать тариф${uiIcon('arrow')}</button></article>`;}).join('');
  root.innerHTML=CT.html`<nav class="public-links" aria-label="О сайте"><a href="#tariffs">Тарифы</a>${config.steps?.length?CT.html('<a href="#how">Как подключиться</a>'):''}${config.faq?.length?CT.html('<a href="#faq">Помощь</a>'):''}<button class="btn line" id="publicLogin">${uiIcon('key')}Войти по коду</button></nav><section class="landing-hero"><div><h1>${esc(config.headline)}</h1><p>${esc(config.description)}</p><a class="btn" href="#tariffs">Выбрать тариф${uiIcon('arrow')}</a><button class="text-button" id="existingCode">У меня уже есть код</button></div>${config.steps?.length?`<ol class="landing-steps" id="how">${config.steps.map((step,i)=>`<li>${i>0?'<svg class="step-connector" viewBox="0 0 60 110" aria-hidden="true"><path d="M22 2C-12 8 2 87 52 100m-8-8 8 8-11 3"/></svg>':''}<span>${i+1}</span><div><strong>${esc(step.title)}</strong><p>${esc(step.text)}</p></div>${uiIcon('check')}</li>`).join('')}</ol>`:''}</section><section class="public-tariffs" id="tariffs"><h2>${esc(config.tariffs_heading)}</h2><div class="public-plans">${plans||CT.html('<p>Сейчас нет доступных тарифов.</p>')}</div></section>${config.faq?.length?`<section class="public-faq" id="faq"><h2>${esc(config.faq_heading)}</h2><div>${config.faq.map(f=>`<details><summary><span>${esc(f.question)}</span>${uiIcon('chevron')}</summary><p>${esc(f.answer)}</p></details>`).join('')}</div></section>`:''}<footer class="public-footer"><span>${esc(config.brand)}</span>${(config.docs_links||[]).map(d=>`<a href="${esc(d.url)}" target="_blank" rel="noopener noreferrer">${esc(d.title)}</a>`).join('')}${config.support_url?`<a href="${esc(config.support_url)}" target="_blank" rel="noopener noreferrer">${esc(config.support_label)}</a>`:''}</footer>`;
  root.querySelector('#publicLogin').onclick=showLogin;root.querySelector('#existingCode').onclick=showLogin;
  root.querySelectorAll('[data-public-plan]').forEach(b=>b.onclick=()=>{selectedOffer={id:Number(b.dataset.publicPlan),days:Number(b.dataset.days)};showLogin();});
}
