/* ═══════════ PAGES: Клиенты (покупатель+VPN-юзер) и Поддержка ═══════════ */
'use strict';

const STRATEGY = {NO_RESET:'Без сброса', DAY:'Сброс: день', WEEK:'Сброс: неделя', MONTH:'Сброс: месяц'};

/* Когда счётчик обнулится в следующий раз.
 *
 * Без этой строки обнуление по расписанию выглядит поломкой: человек
 * видит «использовано 0» там, где вчера было два гигабайта, и решает,
 * что счётчик сломался, — хотя сброс отработал по тарифу час назад. */
function resetNote(u){
  if (u.strategy === 'NO_RESET' || !u.resetAt) {
    return `<div class="hint" style="margin-top:6px">${I('info',12)}
              Счётчик сам не обнуляется — только вручную или новой покупкой.</div>`;
  }
  const at = new Date(u.resetAt);
  const days = Math.ceil((at - Date.now()) / 86400000);
  const when = days <= 0 ? 'сегодня' : days === 1 ? 'завтра' : `через ${days} дн.`;
  return `<div class="hint" style="margin-top:6px">${I('refresh',12)}
            Обнулится <b>${when}</b>, ${fmtDT(u.resetAt)} — ${STRATEGY[u.strategy].toLowerCase()}.</div>`;
}
let selectedUsers = new Set();
let usersPartnerFilter='';
let usersTariffFilter='';

registerPage({
  id:'users', title:'Клиенты', group:'Клиенты', icon:'user',
  render(){
    selectedUsers = new Set();
    return `
    <div class="page-head">
      <div><h1>Клиенты</h1><div class="desc">Единая карточка: покупатель из бота/сайта (платежи, тариф, Telegram) + VPN-пользователь (трафик, устройства, ключи).</div></div>
      <div class="actions">
        <button class="btn" id="bulkAllBtn">${I('layers',14)} Массовые операции</button>
        <button class="btn primary" id="createUserBtn">${I('plus',14)} Создать клиента</button>
      </div>
    </div>
    <div class="toolbar">
      <div class="search-inp">${I('search',14)}<input class="inp" id="uSearch" placeholder="Поиск: @username, email, UUID, Telegram ID, тег…"></div>
      <select class="inp" style="width:160px" id="uStatusF"><option value="">Все статусы</option>${Object.entries(USER_STATUS).map(([k,v])=>`<option value="${k}">${v.t}</option>`).join('')}</select>
      <select class="inp" style="width:150px" id="uTariffF"><option value="">Все тарифы</option>${DB.tariffs.map(t=>`<option ${usersTariffFilter===t.name?'selected':''}>${esc(t.name)}</option>`).join('')}</select>
      <select class="inp" style="width:180px" id="uPartnerF"><option value="">Все партнёры</option>${DB.partners.map(p=>`<option value="${esc(p.slug)}" ${usersPartnerFilter===p.slug?'selected':''}>${esc(p.user)}</option>`).join('')}</select>
      <button class="btn icon-only" title="Колонки" id="colsBtn">${I('columns',14)}</button>
      <button class="btn icon-only" title="Обновить" onclick="refreshDB()">${I('refresh',14)}</button>
    </div>
    <div id="bulkBar"></div>
    <div class="tbl-wrap"><table class="tbl">
      <thead><tr>
        <th style="width:34px"><label class="check"><input type="checkbox" id="selAll"></label></th>
        <th>Клиент</th><th>Статус</th><th>Тариф</th><th>Трафик</th><th>Истекает</th><th>HWID</th><th>Онлайн</th><th style="width:36px"></th>
      </tr></thead>
      <tbody id="uRows"></tbody>
    </table>
    <div class="tbl-foot"><span id="uCount"></span><div class="pages" id="uPages"></div></div>
    </div>`;
  },
  bind(root){
    // Скрытые колонки восстанавливаем сразу: иначе настройка выглядит
    // потерянной при каждом заходе на страницу.
    setTimeout(applyColumns, 0);
    const drawBulk = ()=>{
      const bar = root.querySelector('#bulkBar');
      if(!selectedUsers.size){ bar.innerHTML=''; return; }
      bar.innerHTML = `<div class="bulk-bar">
        <b>Выбрано: ${selectedUsers.size}</b>
        <button class="btn sm" id="bulkActs">${I('zap',12)} Действия</button>
        <button class="btn sm" id="bulkUpd">${I('edit',12)} Изменить поля</button>
        <div style="flex:1"></div>
        <button class="btn ghost sm" id="bulkClear">${I('x',12)} Снять выбор</button>
      </div>`;
      bar.querySelector('#bulkClear').addEventListener('click', ()=>{ selectedUsers.clear(); draw(); });
      bar.querySelector('#bulkActs').addEventListener('click', ()=>bulkActionsModal([...selectedUsers], false));
      bar.querySelector('#bulkUpd').addEventListener('click', ()=>bulkUpdateDrawer([...selectedUsers], false));
    };
    let offset=0, requestId=0;
    const pageSize=50;
    const draw = async ()=>{
      const request=++requestId;
      const q = root.querySelector('#uSearch').value.toLowerCase();
      const sf = root.querySelector('#uStatusF').value;
      const tf = root.querySelector('#uTariffF').value;
      let result;
      try {
        result = await API.call('/api/clients?'+new URLSearchParams({q, status:sf.toLowerCase(), tariff:tf, partner:root.querySelector('#uPartnerF').value, limit:pageSize, offset}));
      } catch(e) { if(request===requestId) toast('Клиенты не загрузились: '+e.message,'err'); return; }
      if(request!==requestId || !root.isConnected) return;
      const rows=result.items.map(mapClient);
      for(const u of rows){ const i=DB.users.findIndex(x=>x.id===u.id); if(i<0) DB.users.push(u); else DB.users[i]=u; }
      DB.usersFilteredTotal=result.total;
      root.querySelector('#uCount').textContent = result.total ? `${offset+1}–${offset+rows.length} из ${fmtN(result.total)} клиентов` : 'Клиентов не найдено';
      root.querySelector('#uPages').innerHTML=`<button class="btn sm" data-prev ${offset===0?'disabled':''}>Назад</button><button class="btn sm" data-next ${offset+pageSize>=result.total?'disabled':''}>Далее</button>`;
      root.querySelector('[data-prev]').onclick=()=>{offset=Math.max(0,offset-pageSize);draw();};
      root.querySelector('[data-next]').onclick=()=>{offset+=pageSize;draw();};
      root.querySelector('#uRows').innerHTML = rows.map(u=>{
        const dl = daysLeft(u.paidUntil);
        const pct = u.limitGb ? Math.min(100, u.usedGb/u.limitGb*100) : null;
        return `<tr data-uid="${u.id}" class="${selectedUsers.has(u.id)?'selected':''}" style="cursor:pointer">
          <td><label class="check"><input type="checkbox" data-sel ${selectedUsers.has(u.id)?'checked':''}></label></td>
          <td><div class="cell-main">
            <div class="avatar-sm">${u.username[0].toUpperCase()}</div>
            <div><b>@${esc(u.username)} ${u.tag?`<span class="tag-pill">${esc(u.tag)}</span>`:''}</b>
            <span class="sub mono">${esc(u.email || u.tg || u.shortUuid)}</span></div></div></td>
          <td>${statusBadge(u.status)}</td>
          <td><span class="chip">${esc(u.tariff)}</span>${u.autoRenew?`<span class="sub" style="color:var(--ok-ink)">автопродление</span>`:''}</td>
          <td class="traffic-cell">
            <span class="num">${fmtB(u.usedGb)} <span style="color:var(--text-3)">/ ${fmtB(u.limitGb)}</span></span>
            ${pct!==null?`<div class="progress" style="margin-top:5px"><i style="width:${pct}%" class="${pct>=100?'err':pct>80?'warn':''}"></i></div>`:''}
            <span class="sub">${STRATEGY[u.strategy]}</span></td>
          <td>${u.paidUntil?`<span class="num" style="font-size:12px">${fmtDate(u.paidUntil)}</span><span class="sub" style="color:${dl<0?'var(--err)':dl<4?'var(--warn)':'var(--text-3)'}">${dl<0?'истёк '+(-dl)+' дн назад':'через '+dl+' дн'}</span>`:'—'}</td>
          <td class="num">${u.hwid}<span style="color:var(--text-3)">/${u.hwidLimit}</span></td>
          <td>${u.online?'<span class="bdg ok live"><span class="dot"></span>online</span>':`<span style="font-size:11.5px;color:var(--text-3)">${fmtDT(u.lastOnline)}</span>`}</td>
          <td><button class="btn ghost icon-only" data-umenu>${I('more',14)}</button></td>
        </tr>`;
      }).join('') || `<tr><td colspan="9"><div class="empty">${I('search',32)}<b>Никого не нашли</b><span>Измените фильтры или создайте клиента</span></div></td></tr>`;
      drawBulk();
      root.querySelectorAll('[data-uid]').forEach(tr=>{
        const u = findUser(tr.dataset.uid);
        tr.querySelector('[data-sel]').addEventListener('click', e=>{
          e.stopPropagation();
          e.target.checked ? selectedUsers.add(u.id) : selectedUsers.delete(u.id);
          tr.classList.toggle('selected', e.target.checked); drawBulk();
        });
        tr.querySelector('[data-umenu]').addEventListener('click', e=>{ e.stopPropagation(); userMenu(e.currentTarget, u); });
        tr.addEventListener('click', ()=>openUserView(u));
      });
    };
    root.querySelector('#selAll').addEventListener('change', e=>{
      selectedUsers = e.target.checked ? new Set([...root.querySelectorAll('[data-uid]')].map(tr=>tr.dataset.uid)) : new Set(); draw();
    });
    let searchTimer;
    ['#uSearch','#uStatusF','#uTariffF','#uPartnerF'].forEach(s=>root.querySelector(s).addEventListener(s==='#uSearch'?'input':'change', ()=>{usersPartnerFilter=root.querySelector('#uPartnerF').value;usersTariffFilter=root.querySelector('#uTariffF').value;offset=0;clearTimeout(searchTimer);searchTimer=setTimeout(draw,s==='#uSearch'?180:0);}));
    root.querySelector('#createUserBtn').addEventListener('click', ()=>createUserModal());
    root.querySelector('#colsBtn').addEventListener('click', e=>menu(e.currentTarget, [
      {title:'Колонки таблицы'},
      // Скрытые колонки помним между заходами: настраивать таблицу
      // заново при каждом открытии — работа впустую.
      ...['Статус','Тариф','Трафик','Истекает','HWID','Онлайн','LTV','Сквады'].map((c,i)=>{
        const hidden = (JSON.parse(localStorage.getItem('sn_cols') || '[]'));
        return {
          label: (hidden.includes(c) ? '○ ' : '● ') + c,
          icon: hidden.includes(c) ? 'eye' : 'check',
          onClick: () => {
            const set = new Set(JSON.parse(localStorage.getItem('sn_cols') || '[]'));
            set.has(c) ? set.delete(c) : set.add(c);
            localStorage.setItem('sn_cols', JSON.stringify([...set]));
            applyColumns();
            toast(set.has(c) ? `Колонка «${c}» скрыта` : `Колонка «${c}» показана`);
          },
        };
      }),
    ]));
    root.querySelector('#bulkAllBtn').addEventListener('click', e=>menu(e.currentTarget, [
      {title:'Для ВСЕХ по текущему фильтру'},
      {label:'Массовые действия…', icon:'zap', onClick:()=>bulkActionsModal(null, true)},
      {label:'Массовое изменение полей…', icon:'edit', onClick:()=>bulkUpdateDrawer(null, true)},
    ]));
    draw();
  }
});

function userMenu(anchor, u){
  menu(anchor, [
    {label:'Открыть карточку', icon:'eye', onClick:()=>openUserView(u)},
    {label:'Детальная информация', icon:'info', onClick:()=>userDetailDrawer(u)},
    {label:'Ключи подключения', icon:'key', onClick:()=>connectionKeysDrawer(u)},
    {label:'QR подписки', icon:'qr', onClick:()=>subQrModal(u)},
    '-',
    {label:'Начислить дни и трафик', icon:'gift', onClick:()=>grantModal(u)},
    {label:'Сбросить трафик', icon:'refresh', onClick:()=>resetTraffic(u)},
    {label:'Отозвать подписку', icon:'rotate', onClick:()=>revokeSub(u)},
    {label:u.status==='DISABLED'?'Включить':'Отключить', icon:u.status==='DISABLED'?'power':'ban', onClick:()=>toggleClient(u)},
    '-',
    {label:'Удалить', icon:'trash', danger:true, onClick:()=>deleteClient(u)},
  ]);
}

/* ── карточка клиента (view/edit) — флагман объединения ── */
/* Карточка клиента.
   Строка списка не содержит ни Telegram, ни почты, ни заметки — они
   приходят только подробным ответом по одному клиенту. Раньше карточка
   рисовалась прямо из строки списка, поэтому эти поля были пустыми
   всегда, чем бы их ни заполняли. Дочитываем перед показом. */
async function openUserView(u){
  if (!u?.id) { toast('Клиент не найден. Обновите список.', 'err'); return; }
  if (u.id) {
    try {
      const [full, payments, devices, externalSquads] = await Promise.all([
        API.call('/api/clients/' + u.id), API.call('/api/clients/' + u.id + '/payments'),
        API.call('/api/clients/' + u.id + '/devices'), API.call('/api/ext-squads').catch(()=>null),
      ]);
      Object.assign(u, mapClient(full), { _full: true });
      applyIdentities(u, full.identities);
      u._externalSquads = externalSquads;
      u._payments = payments.map(p=>mapPayment({...p,username:full.username,client_id:full.id}));
      u._devices = devices.map(d=>({hwid:d.hwid,platform:d.platform||'—',model:d.model||'—',app:d.app_version||'—',last:d.last_seen_at}));
    } catch (e) {
      // Не смогли дочитать — показываем что есть, но честно предупреждаем.
      toast('Карточка не загрузилась: ' + e.message, 'err'); return;
    }
  }
  return openUserViewRender(u);
}

function openUserViewRender(u){
  const editable = DB.admin?.role !== 'readonly';
  const manage = !['readonly','support'].includes(DB.admin?.role);
  openDrawer({
    title:'@'+esc(u.username), sub:`Клиент с ${fmtDate(u.createdAt)} · ${u.online?'недавно передавал трафик':u.lastOnline?'активность '+fmtDT(u.lastOnline):'активность пока не зарегистрирована'}`, icon:'user', size:'lg', className:'client-dialog', initialFocus:'title',
    body:`
      <div class="client-summary">
        <div class="client-summary-access">${statusBadge(u.status)}<strong>${esc(u.tariff&&u.tariff!=='—'?u.tariff:'Без тарифа')}</strong><span>${u.paidUntil?(u.status==='EXPIRED'?'Доступ закончился ':'Доступ до ')+fmtDate(u.paidUntil):u.status==='EXPIRED'?'Нет активной подписки':'Срок не ограничен'}</span></div>
        <div><span>Трафик за период</span><strong>${fmtB(u.usedGb)} <small>/ ${u.limitGb==null?'∞':fmtB(u.limitGb)}</small></strong></div>
        <div><span>Устройства</span><strong>${u.hwid??0} <small>/ ${u.hwidLimit??'∞'}</small></strong></div>
        <div><span>Оплачено за всё время</span><strong>${clientLtv(u)}</strong></div>
      </div>
      <div class="client-quick-actions">
        <button class="btn primary" data-go="messages">${I('send',14)} Написать в бот</button>
        ${manage?`<button class="btn" data-act="grant">${I('gift',14)} Начислить доступ</button>`:''}
        <button class="btn" data-copy="${esc(subLink(u.shortUuid))}" ${subLink(u.shortUuid)?'':'disabled'}>${I('link',14)} Ссылка подключения</button>
        <button class="btn ghost" data-go="prof">${I('edit',14)} ${editable?'Изменить профиль':'Профиль'}</button>
      </div>
      <div class="tabs" id="uvTabs">
        <button class="on" data-t="overview">Обзор</button>
        <button data-t="prof">Профиль</button>
        <button data-t="sub">Подписка и VPN</button>
        <button data-t="pays">Платежи <span class="nav-badge" style="margin-left:4px">${u._payments?.length||0}</span></button>
        <button data-t="dev">Устройства</button>
        <button data-t="messages">Сообщения</button>
        <button data-t="act">Активность</button>
      </div>
      <div class="tab-pane on" data-p="overview">
        <div class="client-overview-grid">
          <section class="client-section"><h4>О клиенте</h4>
            <dl class="client-facts"><div><dt>Telegram</dt><dd>${u.tgId?`<span class="mono">${esc(u.tgId)}</span><button class="btn ghost icon-only" data-copy="${esc(u.tgId)}" aria-label="Скопировать Telegram ID">${I('copy',13)}</button>`:'Не привязан'}</dd></div>
              <div><dt>Email</dt><dd>${esc(u.email||'Не привязан')}</dd></div><div><dt>Источник</dt><dd>${esc(u.referrer||'Самостоятельная регистрация')}</dd></div>
              <div><dt>Тег</dt><dd>${u.tag?`<span class="chip accent">${esc(u.tag)}</span>`:'Без тега'}</dd></div></dl>
            ${u.desc?`<div class="client-note-preview"><span>Заметка команды</span><p>${esc(u.desc)}</p></div>`:'<p class="hint">В профиле можно добавить заметку для команды.</p>'}
          </section>
          <section class="client-section"><h4>Доступ и обслуживание</h4>
            <p class="section-description">${u.status==='ACTIVE'?'Подписка активна. Управляйте сроком, лимитами и доступными локациями.':u.status==='DISABLED'?'Доступ отключён администратором. Включить его можно в разделе управления доступом.':u.status==='LIMITED'?'Лимит трафика исчерпан. Начислите трафик или измените условия доступа.':u.paidUntil?'Срок подписки истёк. Продлите доступ или назначьте новый тариф.':'Активной подписки нет. Назначьте тариф или начислите доступ.'}</p>
            <button class="client-overview-link" data-go="sub">${I('shield',16)}<span><b>Подписка и VPN</b><small>${u.squads.length} сквадов · ${u.autoRenew?'автопродление включено':'автопродление выключено'}</small></span>${I('chevR',14)}</button>
            <button class="client-overview-link" data-go="dev">${I('smartphone',16)}<span><b>Устройства</b><small>${u.hwid??0} зарегистрировано · лимит ${u.hwidLimit}</small></span>${I('chevR',14)}</button>
            <button class="client-overview-link" data-go="pays">${I('card',16)}<span><b>История платежей</b><small>${u._payments?.length||0} записей · оплаты, ошибки и возвраты</small></span>${I('chevR',14)}</button>
            <button class="client-overview-link" data-go="act">${I('pulse',16)}<span><b>Диагностика и активность</b><small>Трафик по нодам, обращения и блокировки</small></span>${I('chevR',14)}</button>
          </section>
        </div>
      </div>
      <div class="tab-pane" data-p="messages">
        <div class="client-chat">
          <div class="client-chat-head"><span>${I('send',14)} Сообщения от вашего бота</span><button class="btn sm" data-message-refresh>Обновить</button></div>
          <div class="client-message-status" role="status" data-message-status>Загружаем переписку…</div>
          <div class="client-message-history" data-message-history><div class="empty"><b>Переписка с клиентом</b><span>Здесь появятся сообщения из карточки и обращений поддержки.</span></div></div>
          <div class="client-message-composer">
            <div class="field"><label for="clientMessage">Сообщение клиенту</label><textarea id="clientMessage" class="inp" maxlength="4000" rows="3" placeholder="Напишите клиенту от имени вашего VPN-сервиса…" ${editable?'':'disabled'}></textarea></div>
            <div class="client-message-tools"><button class="btn sm" data-message-link ${subLink(u.shortUuid)?'':'disabled'}>${I('link',12)} Вставить ссылку подключения</button><span class="sub-note" data-message-length>0 / 4000</span><button class="btn primary" data-message-send disabled>${I('send',14)} Отправить в бот</button></div>
          </div>
        </div>
      </div>
      <div class="tab-pane" data-p="prof">
        <div class="client-profile-grid">
          <section class="client-section">
            <h4>Контактные данные</h4><p class="section-description">Идентификаторы клиента для входа и связи.</p>
            <div class="two-col">
              <div class="field"><label>Имя пользователя</label><input class="inp" id="uUsername" value="${esc(u.username)}" autocomplete="off"></div>
              <div class="field"><label>Telegram ID</label><div class="inp-row"><input class="inp mono" id="uTelegram" inputmode="numeric" value="${esc(u.tgId||'')}" placeholder="Не привязан"><button class="btn icon-only" title="Открыть Telegram клиента" aria-label="Открыть Telegram клиента" data-tgopen ${u.tgId?'':'disabled'}>${I('send',13)}</button></div></div>
              <div class="field client-email"><label>Email</label><input class="inp" type="email" id="uEmail" value="${esc(u.email||'')}" placeholder="Не привязан"></div>
            </div>
          </section>
          <section class="client-section client-notes">
            <h4>Для команды</h4><p class="section-description">Эти сведения видны только администраторам.</p>
            <div class="field"><label>Тег</label><input class="inp" id="uTag" value="${esc(u.tag||'')}" placeholder="Например, VIP"></div>
            <div class="field"><label>Заметка</label><textarea class="inp" id="uNote" placeholder="Договорённости, особенности, история обращения…">${esc(u.desc||'')}</textarea></div>
          </section>
        </div>
        ${manage?`<section class="client-section" style="margin-top:20px"><h4>${I('key',15)} Код доступа к кабинету</h4><p class="section-description">Просмотр и сброс кода для входа на сайт. Подписка, платежи и Telegram остаются на аккаунте.</p><button class="btn" data-cabinet-code>${I('key',14)} Управление кодом</button></section>`:''}
        ${editable?`<details class="client-access-actions"><summary>${I('shield',14)} Управление доступом</summary>
          <p class="hint">Отключение приостанавливает VPN. Отзыв меняет ссылку и ключи: клиенту понадобится новая подписка.</p>
          <div class="client-actions">
            <button class="btn sm" data-dz="disable">${I('ban',12)} ${u.status==='DISABLED'?'Включить доступ':'Отключить доступ'}</button>
            <button class="btn sm" data-dz="revoke">${I('rotate',12)} Заменить ссылку и ключи</button>
            ${manage?`<button class="btn danger sm" data-dz="delete">${I('trash',12)} Удалить клиента</button>`:''}
          </div>
        </details>`:''}
      </div>

      <div class="tab-pane" data-p="sub">
        <section class="client-section"><h4>Условия доступа</h4><p class="section-description">Тариф, срок и лимиты сохраняются одной кнопкой внизу.</p>
        <div class="two-col">
          <div class="field"><label>Тариф</label><select class="inp" id="uTariff">${DB.tariffs.some(t=>t.name===u.tariff)?'':`<option value="">${esc(u.tariff||'Не назначен')}</option>`}${DB.tariffs.map(t=>`<option value="${t.id}" ${t.name===u.tariff?'selected':''}>${esc(t.name)}</option>`).join('')}</select></div>
          <div class="field"><label>Оплачено до</label><input class="inp" type="date" id="uUntil" value="${dateInputValue(u.paidUntil)}">
            <div class="quick-chips"><button data-q="30">+30 дн</button><button data-q="90">+90 дн</button><button data-q="365">+1 год</button><button data-q="inf">∞</button></div></div>
          <div class="field"><label>Лимит трафика</label><div class="inp-group"><input class="inp num" id="uLimit" value="${u.limitGb??''}" placeholder="∞ (пусто = безлимит)"><span class="suffix">GiB</span></div></div>
          <div class="field"><label>Стратегия сброса</label>
            <select class="inp" id="uStrategy">${Object.entries(STRATEGY).map(([k,v])=>`<option value="${k.toLowerCase()}" ${k===u.strategy?'selected':''}>${v}</option>`).join('')}</select>
            <div class="hint">Счётчик обнуляется по этому расписанию. «Без сброса» — только вручную или новой покупкой.</div></div>
          <div class="field"><label>Лимит устройств (HWID)</label><input class="inp num" id="uHwid" value="${u.hwidLimit}"></div>
          <div class="field"><label>Использовано</label>
            <div style="padding-top:6px"><span class="num">${fmtB(u.usedGb)} / ${fmtB(u.limitGb)}</span>
            <div class="progress" style="margin-top:6px"><i style="width:${u.limitGb?Math.min(100,u.usedGb/u.limitGb*100):8}%"></i></div></div>
            ${resetNote(u)}
            <button class="btn sm" style="margin-top:8px" data-act="reset">${I('refresh',12)} Сбросить счётчик</button></div>
        </div>
        <div class="switch-row"><label class="switch"><input type="checkbox" id="uAutorenew" ${u.autoRenew?'checked':''}><span class="tr"></span></label>
          <div class="sw-txt"><b>Автопродление</b><span>Продление поддерживаемым способом оплаты. Если автосписание недоступно, бот отправит напоминание об оплате.</span></div></div>
        </section>
        <div class="form-section"><h4>${I('squads',13)} Внутренние сквады</h4>
          <div class="chips-select">${DB.squadsInt.map(s=>`<button type="button" class="chip-opt ${u.squads.includes(s.name)?'on':''}" data-squad="${esc(s.name)}" aria-pressed="${u.squads.includes(s.name)}">${esc(s.name)}</button>`).join('')}</div>
          <div class="hint" style="margin-top:8px">Доступы отмеченных внутренних сквадов объединяются. Они определяют инбаунды и ноды клиента.</div></div>
        <div class="form-section"><h4>${I('globe',13)} Внешний сквад</h4><div class="field"><label for="uExternalSquad">Настройки подписки</label><select class="inp" id="uExternalSquad" ${!manage||!u._externalSquads?'disabled':''}><option value="">Глобальные настройки</option>${(u._externalSquads||[{id:u.externalSquadId,name:'Текущий сквад · список недоступен'}]).filter(s=>s.is_active||String(s.id)===u.externalSquadId).map(s=>`<option value="${s.id}" ${String(s.id)===u.externalSquadId?'selected':''}>${esc(s.name)}${s.is_active===false?' · отключён':''}</option>`).join('')}</select><p class="hint">Один внешний сквад переопределяет шаблоны, роутинг и оформление. Доступ к нодам остаётся за внутренними сквадами.${u._externalSquads?'':' Список не загрузился — откройте карточку ещё раз.'}</p></div></div>
        <div class="form-section"><h4>${I('link',13)} Подписка</h4>
          ${subLink(u.shortUuid)
            ? `<div class="copy-block"><span class="txt">${esc(subLink(u.shortUuid))}</span>
                 <button data-copy="${esc(subLink(u.shortUuid))}" data-copy-msg="Ссылка подписки скопирована">${I('copy',14)}</button></div>`
            : `<div class="hint" style="color:var(--warn-ink)">${I('alert',12)} Адрес сервиса подписок не задан — укажите его в «Настройках», иначе ссылку клиенту дать неоткуда.</div>`}
          <div style="display:flex;gap:8px;margin-top:10px;flex-wrap:wrap">
            <button class="btn sm" data-open="qr">${I('qr',12)} QR-код</button>
            <button class="btn sm" data-open="keys">${I('key',12)} Ключи подключения</button>
            <button class="btn sm" data-open="nodes">${I('server',12)} Доступные ноды</button>
            <button class="btn sm" data-open="srh">${I('history',12)} Запросы подписки</button>
          </div></div>
      </div>

      <div class="tab-pane" data-p="pays">
        ${(u._payments||[]).map(p=>{
          const [t,c] = payStatus(p.status);
          return `<button class="client-payment" data-client-payment="${p.id}">
            <span class="client-record-main"><b>${esc(p.item)}</b><small>${fmtDT(p.at)} · ${esc(p.method)}</small></span>
            <span class="client-record-end"><strong class="num">${fmtMoney(p.amount,p.currency)}</strong><span class="bdg ${c}">${t}</span></span>${I('chevR',14)}</button>`;
        }).join('') || `<div class="empty">${I('card',32)}<b>Платежей ещё нет</b><span>Клиент на триале или создан вручную</span></div>`}
        <div style="display:flex;gap:8px;margin-top:14px">
          ${manage?`<button class="btn sm" data-act="grant">${I('gift',12)} Начислить дни и трафик</button>`:''}
        </div>
      </div>

      <div class="tab-pane" data-p="dev">
        ${(u._devices||[]).map(d=>`
          <div class="client-device">
            <span class="client-device-icon">${I(d.platform.startsWith('iOS')||d.platform.startsWith('Android')?'smartphone':'monitor',20)}</span>
            <div class="client-record-main"><b>${esc(d.model)}</b><small>${esc(d.platform)} · ${esc(d.app)}</small><code>${esc(d.hwid)}</code><small>Последнее обращение: ${fmtDT(d.last)}</small></div>
            ${editable?`<button class="btn sm" data-unbind="${esc(d.hwid)}" aria-label="Отвязать устройство ${esc(d.model)}">${I('trash',13)} Отвязать</button>`:''}
          </div>`).join('') || `<div class="empty">${I('smartphone',32)}<b>Устройства не зарегистрированы</b><span>Появятся при первом подключении приложения с HWID</span></div>`}
        <div class="sub-note" style="margin-top:12px">Занято ${u.hwid} из ${u.hwidLimit} слотов. Лимит меняется на вкладке «Подписка и VPN».</div>
      </div>

      <div class="tab-pane" data-p="act">
        <div style="display:flex;gap:8px;flex-wrap:wrap;margin-bottom:16px">
          <button class="btn sm" data-open="usage">${I('chart',12)} Трафик по нодам</button>
          <button class="btn sm" data-open="sessions">${I('pulse',12)} Активные сессии</button>
          <button class="btn sm" data-open="torrent">${I('magnet',12)} Торрент-репорты</button>
        </div>
        <div class="info-rows">
          <div class="info-row"><span class="k">UUID</span><span class="v mono">${esc(u.uuid)} <button class="btn ghost icon-only" style="width:24px;height:24px;padding:4px" onclick="copyText('${esc(u.uuid)}')">${I('copy',12)}</button></span></div>
          <div class="info-row"><span class="k">Short UUID</span><span class="v mono">${esc(u.shortUuid)}</span></div>
          <div class="info-row"><span class="k">Создан</span><span class="v">${fmtDate(u.createdAt)}</span></div>
          <div class="info-row"><span class="k">Последний онлайн</span><span class="v">${u.online?'<span class="bdg ok live"><span class="dot"></span>трафик за последние 5 минут</span>':fmtDT(u.lastOnline)}</span></div>
          <div class="info-row"><span class="k">Трафик за всё время</span><span class="v num">${fmtBytes(u.trafficTotal||0)}</span></div>
          <div class="info-row"><span class="k">Источник</span><span class="v">${u.referrer
            ? `Партнёр «${esc(u.referrer)}»` + (u.referrerSlug ? ` <span class="chip">${esc(u.referrerSlug)}</span>` : '')
            : 'пришёл сам'}</span></div>
        </div>
      </div>`,
    footer:`<span class="client-save-state" role="status">${editable?'Нет изменений':'Режим просмотра'}</span><div class="spacer"></div><button class="btn" data-close>Закрыть</button><button class="btn primary" data-save disabled ${editable?'':'hidden'}>Сохранить изменения</button>`,
    onMount(layer, close){
      layer.querySelectorAll('[data-client-payment]').forEach(b=>b.addEventListener('click',()=>payDetails(u._payments.find(p=>p.id===b.dataset.clientPayment))));
      const save = layer.querySelector('[data-save]');
      const fields = [...layer.querySelectorAll('[data-p="prof"] input,[data-p="prof"] textarea,[data-p="sub"] input,[data-p="sub"] select')];
      const snapshot = () => JSON.stringify([fields.map(f=>f.type==='checkbox'?f.checked:f.value), [...layer.querySelectorAll('[data-squad].on')].map(c=>c.dataset.squad)]);
      const original = snapshot();
      let saving = false;
      const updateSave = () => {
        const changed = snapshot() !== original;
        save.disabled = !editable || !changed || saving;
        save.hidden = !editable || (!changed && !['prof','sub'].includes(layer.querySelector('#uvTabs .on')?.dataset.t));
        layer.querySelector('.client-save-state').textContent = saving?'Сохраняем…':!editable?'Режим просмотра':changed?'Есть несохранённые изменения':'Нет изменений';
      };
      fields.forEach(f=>{ f.disabled = !editable || (!manage&&!['uTag','uNote'].includes(f.id)) || (f.id==='uExternalSquad'&&!u._externalSquads); f.addEventListener('input',updateSave); f.addEventListener('change',updateSave); });
      layer.querySelectorAll('[data-squad], [data-q]').forEach(b=>b.disabled=!manage);
      layer.querySelectorAll('[data-act="reset"]').forEach(b=>b.disabled=!editable);
      const tabs = layer.querySelector('#uvTabs');
      tabs.querySelectorAll('button').forEach(b=>b.addEventListener('click', ()=>{
        tabs.querySelectorAll('button').forEach(x=>x.classList.remove('on')); b.classList.add('on');
        layer.querySelectorAll('.tab-pane').forEach(p=>p.classList.toggle('on', p.dataset.p===b.dataset.t)); updateSave();
        if(b.dataset.t==='messages') messages.loadOnce();
        b.scrollIntoView({block:'nearest',inline:'nearest'});
      }));
      const messages=bindClientMessages(layer,u,editable);
      layer.querySelector('[data-cabinet-code]')?.addEventListener('click',()=>clientCodeModal(u));
      layer.querySelectorAll('[data-go]').forEach(b=>b.onclick=()=>tabs.querySelector('[data-t="'+b.dataset.go+'"]').click());
      updateSave();
      layer.querySelectorAll('.chip-opt').forEach(c=>c.addEventListener('click', ()=>{ if(!manage)return; c.classList.toggle('on'); c.setAttribute('aria-pressed',c.classList.contains('on')); updateSave(); }));
      // Быстрые кнопки двигают дату в поле, а не шлют запрос: сохранение
      // одно, и человек видит итоговую дату до того, как нажмёт «Сохранить».
      layer.querySelectorAll('.quick-chips button').forEach(b=>b.addEventListener('click', ()=>{
        const inp = layer.querySelector('#uUntil');
        if (b.dataset.q === 'inf') { inp.value = ''; updateSave(); return; }
        const base = inp.value ? new Date(inp.value) : new Date();
        // Продлеваем от большей из дат: у истёкшей подписки отсчёт от сегодня,
        // иначе начисленные дни уходят в прошлое.
        const from = base > new Date() ? base : new Date();
        from.setDate(from.getDate() + parseInt(b.dataset.q, 10));
        inp.value = dateInputValue(from); updateSave();
      }));

      save.addEventListener('click', async ()=>{
        if(save.disabled) return;
        const invalid = fields.find(f=>!f.checkValidity());
        if(invalid){ tabs.querySelector('[data-t="'+invalid.closest('[data-p]').dataset.p+'"]').click(); invalid.reportValidity(); return; }
        const limit = layer.querySelector('#uLimit').value.trim();
        const devices = Number(layer.querySelector('#uHwid').value);
        if(manage&&((limit && (!Number.isFinite(Number(limit)) || Number(limit)<0)) || !Number.isInteger(devices) || devices<1)){ toast('Проверьте лимиты: трафик от 0, устройства — целое число от 1.', 'err'); return; }

        const gb = layer.querySelector('#uLimit').value.trim();
        const until = layer.querySelector('#uUntil').value;
        const body = {
          username: layer.querySelector('#uUsername').value.trim(),
          tag: layer.querySelector('#uTag').value.trim(),
          telegram_id: layer.querySelector('#uTelegram').value.trim() || null,
          email: layer.querySelector('#uEmail').value.trim() || null,
          note: layer.querySelector('#uNote').value.trim(),
          autorenew: layer.querySelector('#uAutorenew').checked,
          tariff_id: parseInt(layer.querySelector('#uTariff').value, 10),
          // Пусто = безлимит; поле должно уйти как null, иначе прежний лимит останется.
          traffic_limit_bytes: gb === '' ? null : Math.round(parseFloat(gb) * 1024 ** 3),
          device_limit: parseInt(layer.querySelector('#uHwid').value, 10) || 1,
          reset_strategy: layer.querySelector('#uStrategy').value,
          squad_names: [...layer.querySelectorAll('.chip-opt.on')].map(c=>c.dataset.squad),
        };
        if(manage && u._externalSquads && layer.querySelector('#uExternalSquad').value!==u.externalSquadId) body.external_squad_id=layer.querySelector('#uExternalSquad').value?Number(layer.querySelector('#uExternalSquad').value):null;
        // Saving a note must not change expiry or restart the traffic-reset schedule.
        if (until !== dateInputValue(u.paidUntil)) body.expires_at = until ? new Date(until + 'T23:59:59').toISOString() : null;
        if (body.reset_strategy.toUpperCase() === u.strategy) delete body.reset_strategy;
        const selectedTariff = DB.tariffs.find(t => String(t.id) === String(body.tariff_id));
        if (!selectedTariff || selectedTariff.name === u.tariff) delete body.tariff_id;
        if (!manage) for (const key of Object.keys(body)) if (!['tag','note'].includes(key)) delete body[key];
        saving = true; updateSave();
        try {
          await API.call('/api/clients/'+u.id, { method:'PATCH', body });
          close();
          toast('Клиент сохранён. Изменения доедут на ноды за ~15 сек');
          await refreshDB();
        } catch(e){ toast('Не сохранилось: '+e.message, 'err'); }
        finally { saving = false; updateSave(); }
      });

      // Диалог в Telegram открываем по ID, а не рапортуем об успехе.
      layer.querySelectorAll('[data-tgopen]').forEach(b=>b.addEventListener('click', ()=>{
        if (!u.tgId) { toast('У клиента не привязан Telegram', 'err'); return; }
        window.open('tg://user?id=' + encodeURIComponent(u.tgId), '_blank');
      }));

      layer.querySelectorAll('[data-unbind]').forEach(b=>b.addEventListener('click', ()=>unbindDevice(u, b.dataset.unbind)));
      layer.querySelectorAll('[data-act]').forEach(b=>b.addEventListener('click', ()=>{
        if (b.dataset.act === 'reset') return resetTraffic(u);
        if (b.dataset.act === 'grant') return grantModal(u, close);
      }));

      const dz = a => ({
        disable: ()=>toggleClient(u),
        revoke: ()=>revokeSub(u),
        delete: ()=>deleteClient(u, close),
      })[a]();
      layer.querySelectorAll('[data-dz]').forEach(b=>b.addEventListener('click', ()=>dz(b.dataset.dz)));
      const opens = {qr:()=>subQrModal(u), keys:()=>connectionKeysDrawer(u), nodes:()=>accessibleNodesModal(u), srh:()=>subRequestsModal(u), usage:()=>userUsageModal(u), sessions:()=>userSessionsModal(u), torrent:()=>userTorrentModal(u)};
      layer.querySelectorAll('[data-open]').forEach(b=>b.addEventListener('click', ()=>opens[b.dataset.open]()));
    }
  });
}

function bindClientMessages(layer,u,editable){
  const box=layer.querySelector('[data-message-history]'),status=layer.querySelector('[data-message-status]'),input=layer.querySelector('#clientMessage'),send=layer.querySelector('[data-message-send]');
  let loaded=false,busy=false,allowed=false,items=[],before=null,pending=null;
  const update=()=>{send.disabled=!editable||!allowed||busy||!input.value.trim();layer.querySelector('[data-message-length]').textContent=input.value.length+' / 4000';};
  const render=()=>{box.innerHTML=(before?'<button class="btn sm" data-message-older>Загрузить предыдущие</button>':'')+items.map(m=>`<article class="client-message ${m.author_kind==='admin'?'outgoing':'incoming'}"><div class="client-message-meta"><b>${esc(m.author_kind==='admin'?(m.admin_name||'Поддержка'):'@'+u.username)}</b><time>${fmtDT(m.created_at)}</time></div><p>${esc(m.body)}</p><div class="client-message-delivery">${m.author_kind==='admin'?(m.delivered===true?`${I('check',12)} Telegram принял сообщение`:m.delivered===false?`${I('alert',12)} ${esc(m.delivery_error||'Не доставлено в Telegram')}`:`${I('clock',12)} Доставка пока не подтверждена`):'Ответ клиента'} · Обращение #${m.ticket_id}</div></article>`).join('')||'<div class="empty"><b>Начните диалог</b><span>Сообщение придёт клиенту от вашего бота. Его ответ появится здесь и в поддержке.</span></div>';
    box.querySelector('[data-message-older]')?.addEventListener('click',()=>load(true));
  };
  const load=async(older=false)=>{const height=box.scrollHeight,top=box.scrollTop;try{const data=await API.call('/api/clients/'+u.id+'/messages'+(older&&before?'?before='+before:''));if(!layer.isConnected)return;allowed=data.telegram_linked&&data.bot_configured;before=data.next_before;items=older?[...data.items,...items]:data.items;loaded=true;
      status.textContent=!data.telegram_linked?'У клиента не привязан Telegram. Укажите ID в профиле и сохраните изменения.':!data.bot_configured?'Бот не настроен. Подключите BOT_TOKEN сервиса, чтобы отправлять сообщения.':!editable?'Доступен только просмотр переписки.':'Получатель: @'+u.username+' · Telegram '+u.tgId+'. Клиент должен ранее открыть вашего бота.';
      render();box.scrollTop=older?top+box.scrollHeight-height:box.scrollHeight;update();
    }catch(e){status.textContent='Переписка не загрузилась: '+e.message;allowed=false;update();}}
  if(!editable)layer.querySelector('[data-message-link]').disabled=true;
  input.addEventListener('input',update);layer.querySelector('[data-message-refresh]').onclick=()=>load();layer.querySelector('[data-message-link]').onclick=()=>{input.value=(input.value.trim()?input.value.trim()+'\n\n':'')+'Подключение и инструкция: '+subLink(u.shortUuid);input.value=input.value.slice(0,4000);update();input.focus();};
  send.onclick=async()=>{if(send.disabled)return;const body=input.value.trim();if(!pending||pending.body!==body)pending={body,request_id:crypto.randomUUID()};busy=true;update();status.textContent='Отправляем сообщение…';
    try{const result=await API.call('/api/clients/'+u.id+'/messages',{method:'POST',body:pending});input.value='';pending=null;await load();status.textContent=result.delivered===true?'Telegram принял сообщение. Ответ клиента появится в переписке.':result.error||'Доставка пока не подтверждена. Обновите переписку перед повторной отправкой.';}
    catch(e){status.textContent='Отправка не подтверждена: '+e.message+'. Текст сохранён в поле; повторное нажатие не создаст дубль.';}
    finally{busy=false;update();}
  };return {loadOnce:()=>{if(!loaded)load();}};
}

/* ── создание клиента ── */
function createUserModal(){
  openModal({
    title:'Создать клиента', sub:'Вручную, минуя бот', icon:'plus', size:'lg',
    body:`
      <div class="two-col">
        <div class="field"><label>Username <span class="req">*</span></label><input class="inp" id="cuName" placeholder="ivan_petrov"></div>
        <div class="field"><label>Telegram ID</label><input class="inp mono" id="cuTg" placeholder="123456789"></div>
        <div class="field"><label>Email</label><input class="inp" id="cuMail" placeholder="для слияния с веб-аккаунтом"></div>
        <div class="field"><label>Тег</label><input class="inp" id="cuTag" placeholder="VIP"></div>
      </div>
      <div class="form-section"><h4>${I('layers',13)} Подписка</h4>
        <div class="two-col">
          <div class="field"><label>Тариф</label><select class="inp" id="cuTariff">${DB.tariffs.map(t=>`<option value="${t.id}">${esc(t.name)}</option>`).join('')}</select></div>
          <div class="field"><label>Срок подписки</label>
            <div class="inp-group"><input class="inp num" id="cuDays" type="number" value="30"><span class="suffix">дней</span></div>
            <div class="quick-chips" id="cuQuick"><button data-d="7">7</button><button data-d="30">30</button><button data-d="90">90</button><button data-d="365">365</button></div></div>
          <div class="field"><label>Лимит трафика</label><div class="inp-group"><input class="inp num" id="cuGb" placeholder="из тарифа"><span class="suffix">GiB</span></div>
            <div class="hint">Пусто — берётся из тарифа.</div></div>
          <div class="field"><label>Лимит устройств</label><input class="inp num" id="cuHwid" placeholder="из тарифа"></div>
        </div>
        <div class="field"><label>Сквады</label>
          <div class="chips-select">${DB.squadsInt.map(s=>`<span class="chip-opt" data-squad="${esc(s.name)}">${esc(s.name)}</span>`).join('')}</div>
          <div class="hint">Пусто — клиент получит сквады выбранного тарифа.</div></div>
      </div>`,
    footer:`<div class="spacer"></div><button class="btn" data-close>Отмена</button><button class="btn primary" data-save>Создать клиента</button>`,
    onMount(layer, close){
      layer.querySelectorAll('.chip-opt').forEach(c=>c.addEventListener('click', ()=>c.classList.toggle('on')));
      layer.querySelectorAll('#cuQuick button').forEach(b=>b.addEventListener('click', ()=>{
        layer.querySelector('#cuDays').value = b.dataset.d;
      }));

      const btn = layer.querySelector('[data-save]');
      btn.addEventListener('click', async ()=>{
        const name = layer.querySelector('#cuName').value.trim();
        if (!name) { toast('Нужен username', 'err'); return; }
        if (!DB.tariffs.length) { toast('Сначала создайте тариф', 'err'); return; }

        btn.disabled = true;
        try {
          const r = await API.call('/api/clients', { method:'POST', body:{
            username: name,
            telegram_id: layer.querySelector('#cuTg').value.trim() || null,
            email: layer.querySelector('#cuMail').value.trim() || null,
            tag: layer.querySelector('#cuTag').value.trim() || null,
            tariff_id: parseInt(layer.querySelector('#cuTariff').value, 10),
            expires_days: parseInt(layer.querySelector('#cuDays').value, 10) || 30,
          }});

          // Лимиты и сквады отличаются от тарифных — досылаем отдельно,
          // чтобы создание клиента оставалось одной простой операцией.
          const gb = layer.querySelector('#cuGb').value.trim();
          const hwid = layer.querySelector('#cuHwid').value.trim();
          const squads = [...layer.querySelectorAll('.chip-opt.on')].map(c=>c.dataset.squad);
          const extra = {};
          if (gb !== '') extra.traffic_limit_bytes = Math.round(parseFloat(gb) * 1024 ** 3);
          if (hwid !== '') extra.device_limit = parseInt(hwid, 10);
          if (squads.length) extra.squad_names = squads;
          if (Object.keys(extra).length) {
            await API.call('/api/clients/'+r.id, { method:'PATCH', body: extra });
          }

          close();
          toast('Клиент создан, ссылка подписки готова');
          await refreshDB();
        } catch(e){ toast('Не создался: '+e.message, 'err'); btn.disabled = false; }
      });
    }
  });
}

/* ── детальная информация ── */
function userDetailDrawer(u){
  openDrawer({
    title:'Детальная информация', sub:'@'+esc(u.username), icon:'info',
    body:`
      <div class="info-rows">
        <div class="info-row"><span class="k">UUID</span><span class="v mono">${esc(u.uuid)}</span></div>
        <div class="info-row"><span class="k">Short UUID</span><span class="v mono">${esc(u.shortUuid)}</span></div>
        <div class="info-row"><span class="k">Ссылка подписки</span><span class="v mono">${subLink(u.shortUuid)
          ? `${esc(subLink(u.shortUuid))} <button class="btn ghost icon-only" style="width:24px;height:24px;padding:4px" data-copy="${esc(subLink(u.shortUuid))}" data-copy-msg="Ссылка подписки скопирована">${I('copy',12)}</button>`
          : '— адрес сервиса подписок не задан'}</span></div>
        <div class="info-row"><span class="k">Статус</span><span class="v">${statusBadge(u.status)}</span></div>
        <div class="info-row"><span class="k">Тариф · период</span><span class="v">${esc(u.tariff)} · ${esc(u.period)}</span></div>
        <div class="info-row"><span class="k">LTV / платежей</span><span class="v num">${clientLtv(u)} · ${u.payments}</span></div>
        <div class="info-row"><span class="k">Трафик</span><span class="v num">${fmtB(u.usedGb)} / ${fmtB(u.limitGb)} (${STRATEGY[u.strategy]})</span></div>
        <div class="info-row"><span class="k">HWID</span><span class="v num">${u.hwid} / ${u.hwidLimit}</span></div>
        <div class="info-row"><span class="k">Сквады</span><span class="v">${u.squads.map(s=>`<span class="chip accent">${s}</span>`).join(' ')}</span></div>
        <div class="info-row"><span class="k">Создан</span><span class="v">${fmtDate(u.createdAt)}</span></div>
        <div class="info-row"><span class="k">Оплачено до</span><span class="v">${fmtDate(u.paidUntil)}</span></div>
        <div class="info-row"><span class="k">Последний онлайн</span><span class="v">${fmtDT(u.lastOnline)}</span></div>
      </div>`,
    footer:`<div class="spacer"></div><button class="btn" data-close>Закрыть</button><button class="btn primary" data-edit>Открыть карточку</button>`,
    onMount(layer, close){ layer.querySelector('[data-edit]').addEventListener('click', ()=>{ close(); openUserView(u); }); }
  });
}

/* ── QR подписки ── */
function subQrModal(u){
  const url = subLink(u.shortUuid);
  openModal({title:'QR-код подписки', sub:'@'+esc(u.username), icon:'qr', size:'sm', initialFocus:'title',
    body:`<div class="qr-box" data-qr style="display:grid;place-items:center;min-height:220px"><span class="sub-note">${url?'Загружаем QR-код…':'Адрес сервиса подписок не настроен.'}</span></div>
      ${url?`<div class="copy-block"><span class="txt">${esc(url)}</span><button data-copy="${esc(url)}" aria-label="Скопировать ссылку подписки">${I('copy',14)}</button></div>`:''}
      <p class="hint" style="margin-top:12px">Сканируйте QR-код в приложении VPN или откройте ссылку, чтобы перейти к инструкции подключения.</p>`,
    footer:'<div class="spacer"></div><button class="btn" data-close>Закрыть</button>',
    onMount(layer){if(!url)return;const img=new Image();img.alt='QR-код ссылки подписки';img.width=220;img.height=220;img.onload=()=>layer.querySelector('[data-qr]').replaceChildren(img);img.onerror=()=>{layer.querySelector('[data-qr]').textContent='QR-код не загрузился. Ссылку можно скопировать ниже.';};img.src=url+'/qr.svg';}
  });
}

function connectionKeysDrawer(u){
  openDrawer({title:'Ключи подключения',sub:'@'+esc(u.username)+' · отдельный ключ для каждой локации',icon:'key',size:'lg',initialFocus:'title',
    body:'<p class="hint">Ключи формируются из текущих хостов, профилей и доступов клиента. После смены ключей или конфигурации их потребуется скопировать заново.</p><div data-keys style="margin-top:18px" role="status">Загружаем ключи…</div>',
    footer:'<button class="btn" data-copy-all disabled>Копировать все</button><div class="spacer"></div><button class="btn" data-close>Закрыть</button>',
    onMount(layer){let links=[];layer.querySelector('[data-copy-all]').onclick=()=>copyText(links.map(x=>x.link).join('\n'),'Ключи скопированы');
      const load=async()=>{const box=layer.querySelector('[data-keys]');box.textContent='Загружаем ключи…';try{
        const data=await API.call('/api/clients/'+u.id+'/connection-links');if(!layer.isConnected)return;
        links=data.links;layer.querySelector('[data-copy-all]').disabled=!links.length;
        box.innerHTML=links.map(k=>`<section class="client-section" style="margin-bottom:12px"><h4>${esc(k.name)} <span class="chip">${esc(k.protocol)}</span></h4><div class="copy-block"><span class="txt">${esc(k.link)}</span><button data-copy="${esc(k.link)}" aria-label="Скопировать ключ ${esc(k.name)}">${I('copy',14)}</button></div></section>`).join('')||`<div class="empty">${I('key',30)}<b>${data.status==='active'?'Доступных ключей нет':'Подписка не активна'}</b><span>${data.status==='active'?'Проверьте сквады клиента, включённые хосты и состояние нод.':'Продлите или включите доступ на вкладке «Подписка и VPN».'}</span></div>`;
        enhancePanelUI(layer);
      }catch(e){box.innerHTML=`<div class="empty"><b>Ключи не загрузились</b><span>${esc(e.message)}</span><button class="btn" data-retry>Повторить</button></div>`;box.querySelector('[data-retry]').onclick=load;}};load();}
  });
}

function squadNodeTags(s,n){
  const profileId=n.profileId??DB.profiles.find(p=>p.name===n.profile)?.id;
  return (n.inbounds||[]).filter(tag=>(s?.inboundRefs||[]).some(ref=>String(ref.profile_id)===String(profileId)&&ref.tag===tag));
}
function accessibleNodesModal(u){
  openModal({title:'Ноды клиента',sub:'@'+esc(u.username)+' · назначенные доступы',icon:'server',size:'lg',initialFocus:'title',
    body:'<div data-access-nodes role="status">Загружаем доступы…</div>',footer:'<div class="spacer"></div><button class="btn" data-close>Закрыть</button>',
    async onMount(layer){const box=layer.querySelector('[data-access-nodes]');try{
      const [client,nodes,squads]=await Promise.all([API.call('/api/clients/'+u.id),API.call('/api/nodes'),API.call('/api/squads')]);
      box.innerHTML=(client.squads||[]).map(name=>{const squad=squads.find(s=>s.name===name);if(!squad)return `<p class="hint">Сквад ${esc(name)} больше не найден. Обновите карточку.</p>`;
        const related=nodes.map(mapNode).map(n=>({n,tags:squadNodeTags({inboundRefs:squad.inbound_refs},n)})).filter(x=>x.tags.length);
        return `<section class="client-section" style="margin-bottom:12px"><h4>${esc(name)}</h4>${related.map(({n,tags})=>`<div class="info-row"><span>${ccChip(n.cc)}</span><span class="v"><b>${esc(n.name)}</b> · ${esc(tags.join(', '))}</span><span class="bdg ${n.status==='online'?'ok':'neutral'}">${n.status==='online'?'Агент на связи':'Нет связи'}</span></div>`).join('')||'<p class="hint">На инбаунды этого сквада пока не назначены ноды.</p>'}</section>`;
      }).join('')||'<div class="empty"><b>Сквады не назначены</b><span>Выберите доступы на вкладке «Подписка и VPN».</span></div>';
      box.insertAdjacentHTML('beforeend','<p class="hint">Список показывает назначение доступов. Ключи выдаются для активной подписки через включённые хосты и работающие ноды.</p>');
    }catch(e){box.innerHTML=`<p role="alert">Не удалось загрузить доступы: ${esc(e.message)}</p>`;}}
  });
}

/* Paged inspectors read the selected entity on the server, never the first cached page. */
function recordsDrawer({title,sub,icon,endpoint,render,empty}){
  return openDrawer({title,sub,icon,size:'lg',initialFocus:'title',body:'<div data-records role="status">Загружаем…</div>',
    footer:'<span data-count class="sub-note"></span><div class="spacer"></div><button class="btn sm" data-prev disabled>Назад</button><button class="btn sm" data-next disabled>Далее</button><button class="btn" data-close>Закрыть</button>',
    onMount(layer){let offset=0,seq=0;const draw=async()=>{const request=++seq,box=layer.querySelector('[data-records]');box.textContent='Загружаем…';layer.querySelector('[data-prev]').disabled=true;layer.querySelector('[data-next]').disabled=true;
      try{const data=await API.call(endpoint+(endpoint.includes('?')?'&':'?')+new URLSearchParams({paginated:true,offset,limit:50}));if(request!==seq||!layer.isConnected)return;
        box.innerHTML=data.items.map(render).join('')||`<div class="empty">${I(icon,30)}<b>Записей нет</b><span>${esc(empty)}</span></div>`;
        layer.querySelector('[data-count]').textContent=data.total?`${offset+1}–${offset+data.items.length} из ${data.total}`:'0 записей';
        layer.querySelector('[data-prev]').disabled=offset===0;layer.querySelector('[data-next]').disabled=offset+50>=data.total;
        box.querySelectorAll('[data-open-client]').forEach(b=>b.onclick=()=>openUserView({id:b.dataset.openClient}));enhancePanelUI(layer);
      }catch(e){box.innerHTML=`<div class="empty"><b>Не удалось загрузить записи</b><span>${esc(e.message)}</span><button class="btn" data-retry>Повторить</button></div>`;box.querySelector('[data-retry]').onclick=draw;}};
      layer.querySelector('[data-prev]').onclick=()=>{offset=Math.max(0,offset-50);draw();};layer.querySelector('[data-next]').onclick=()=>{offset+=50;draw();};draw();}
  });
}
function subRequestsModal(u){
  return recordsDrawer({title:'Запросы подписки',sub:'@'+esc(u.username)+' · обращения приложений к сервису подписок',icon:'history',endpoint:'/api/srh?client_id='+u.id,
    empty:'Сервис подписок пока не зарегистрировал обращений этого клиента.',
    render:r=>`<div class="client-device" style="grid-template-columns:minmax(0,1fr) auto"><div class="client-record-main"><b>${esc(r.user_agent||'Приложение не указано')}</b><small>${fmtDT(r.requested_at)} · ${esc(r.ip||'IP не указан')}</small></div><span class="chip">${esc(r.response_code||'—')}</span></div>`});
}

/* ── трафик по нодам ── */
function userUsageModal(u){
  openModal({
    title:'Трафик клиента', sub:'@'+esc(u.username), icon:'chart', size:'lg',
    body:`
      <div class="seg" id="uuRange" style="margin-bottom:14px">
        <button data-d="7">7 дней</button><button data-d="30" class="on">30 дней</button><button data-d="90">90 дней</button>
      </div>
      <div id="uuBody" class="sub-note">Загружаем…</div>`,
    footer:`<div class="spacer"></div><button class="btn" data-close>Закрыть</button>`,
    onMount(layer){
      const draw = async () => {
        const days = layer.querySelector('#uuRange .on').dataset.d;
        let d;
        try { d = await API.call('/api/clients/'+u.id+'/traffic?days='+days); }
        catch(e){ layer.querySelector('#uuBody').textContent = 'Не загрузилось: '+e.message; return; }

        if (!d.days.length) {
          layer.querySelector('#uuBody').innerHTML =
            `<div class="empty" style="padding:28px">${I('chart',30)}<b>Трафика не было</b>
               <span>За выбранный период счётчики движка ничего не зафиксировали.</span></div>`;
          return;
        }

        const max = Math.max(...d.days.map(x=>x.bytes), 1);
        const maxNode = Math.max(...d.nodes.map(x=>x.bytes), 1);
        const total = d.days.reduce((a,x)=>a+x.bytes, 0);

        layer.querySelector('#uuBody').innerHTML = `
          <div style="display:flex;align-items:flex-end;gap:3px;height:130px;padding:0 2px">
            ${d.days.map(x=>`
              <div title="${x.day}: ${fmtBytes(x.bytes)}"
                style="flex:1;min-width:3px;height:${Math.max(2, x.bytes/max*100)}%;
                       background:var(--accent);opacity:.85;border-radius:2px 2px 0 0"></div>`).join('')}
          </div>
          <div class="sub-note" style="display:flex;justify-content:space-between;margin-top:6px">
            <span>${d.days[0].day}</span><span>всего ${fmtBytes(total)}</span><span>${d.days[d.days.length-1].day}</span>
          </div>
          <div class="info-rows" style="margin-top:16px">
            ${d.nodes.map(n=>`
              <div class="info-row"><span class="v">${ccChip(n.country_code)}</span>
                <span class="v mono" style="flex:1">${esc(n.name)}</span>
                <div class="progress" style="flex:2"><i style="width:${n.bytes/maxNode*100}%"></i></div>
                <span class="num" style="width:80px;text-align:right">${fmtBytes(n.bytes)}</span></div>`).join('')
              || '<div class="sub-note">Разбивки по нодам нет.</div>'}
          </div>`;
      };
      layer.querySelectorAll('#uuRange button').forEach(b=>b.addEventListener('click', ()=>{
        layer.querySelectorAll('#uuRange button').forEach(x=>x.classList.remove('on'));
        b.classList.add('on'); draw();
      }));
      draw();
    }
  });
}

/* ── активные сессии клиента ── */
function activityRecord(r){return `<div class="client-payment"><div class="client-record-main"><button class="link" data-open-client="${r.client_id}">@${esc(r.username)}</button><small>${esc(r.node)} · ${fmtDT(r.at)}</small></div><div class="client-record-end"><small>Последний отчёт</small><span class="num">↓ ${fmtBytes(r.download_bytes)} · ↑ ${fmtBytes(r.upload_bytes)}</span></div></div>`;}
function activityDrawer(title,filter){return recordsDrawer({title,sub:'Клиенты с трафиком за последние 5 минут. Объёмы — за последний отчёт агента, не скорость соединения.',icon:'pulse',endpoint:'/api/sessions/activity?'+new URLSearchParams(filter),render:activityRecord,empty:'За последние 5 минут агент не передавал ненулевой трафик. Это не перечень отдельных TCP-соединений.'});}
function userSessionsModal(u){return activityDrawer('Активность · @'+esc(u.username),{client_id:u.id});}
function userTorrentModal(u){return recordsDrawer({title:'Торрент-репорты',sub:'@'+esc(u.username)+' · последние 30 дней',icon:'magnet',endpoint:'/api/reports/torrents?days=30&client_id='+u.id,
  empty:'За выбранный период агент не передал срабатываний правила блокировки.',render:r=>`<div class="client-device" style="grid-template-columns:minmax(0,1fr) auto"><div class="client-record-main"><b>${esc(r.node||'Нода удалена')}</b><small>${fmtDate(r.day)}</small><code>${esc(r.last_target||'Цель не указана')}</code></div><span class="bdg warn">${fmtN(r.hits)} срабатываний</span></div>`});}

/* Текущий фильтр списка. Режим «для всех» должен затрагивать ровно тех,
   кого человек видит на экране, поэтому те же условия уходят на сервер —
   иначе «выбрать всех» означало бы только первую страницу. */
function currentUserFilter(){
  const main = document.getElementById('main');
  if (!main) return {};
  const q  = main.querySelector('#uSearch');
  const st = main.querySelector('#uStatusF');
  return {
    q: q ? q.value.trim() : '',
    status: st ? st.value.toLowerCase() : '',
    tariff: main.querySelector('#uTariffF')?.value || null,
  };
}

/* ── bulk: действия ── */
function bulkActionsModal(ids, forAll){
  const n = forAll ? `всем по фильтру (${fmtN(DB.usersFilteredTotal??DB.usersTotal??0)})` : ids.length + ' выбранным';
  openModal({
    title:'Массовые действия', sub:`Применится: ${n}`, icon:'zap', iconTone: forAll?'danger':'',
    body:`
      ${forAll?`<div class="hint" style="color:var(--warn-ink);margin-bottom:14px;font-size:12px">${I('alert',13)} Режим «для всех»: действие затронет каждого клиента, попадающего под текущий фильтр списка. Дважды проверьте фильтр.</div>`:''}
      ${[['enable','power','Включить','вернуть доступ отключённым'],
         ['disable','ban','Отключить','подписки перестанут работать'],
         ['reset','refresh','Сбросить трафик','обнулить счётчики'],
         ['revoke','rotate','Отозвать подписки','заменить short-UUID, старые ссылки умрут'],
         ['delete','trash','Удалить','безвозвратно, вместе с ключами']].map(([v,ic,t,d],i)=>`
        <label class="check" style="padding:10px 12px;border:1px solid var(--border);border-radius:10px;margin-bottom:8px;${v==='delete'?'border-color:color-mix(in srgb, var(--err) 30%, transparent)':''}">
          <input type="radio" name="bulkAct" ${i===0?'checked':''} value="${v}" style="appearance:auto;accent-color:var(--accent)">
          <span style="display:flex;align-items:center;gap:9px;flex:1">${I(ic,14)}<span><b style="font-size:12.5px;display:block;${v==='delete'?'color:var(--err-ink)':''}">${t}</b><span style="font-size:11px;color:var(--text-3)">${d}</span></span></span>
        </label>`).join('')}`,
    footer:`<div class="spacer"></div><button class="btn" data-close>Отмена</button><button class="btn primary" data-ok>Применить</button>`,
    onMount(layer, close){
      layer.querySelector('[data-ok]').addEventListener('click', ()=>{
        const act = layer.querySelector('input[name=bulkAct]:checked').value;
        close();
        confirmModal({title:'Подтвердите операцию', danger: act==='delete'||act==='revoke',
          text:`Действие «<b>${{enable:'Включить',disable:'Отключить',reset:'Сброс трафика',revoke:'Отзыв подписок',delete:'Удаление'}[act]}</b>» будет применено ${n}.`,
          okText:'Выполнить',
          onOk:async ()=>{
            // Раньше здесь крутился индикатор и появлялось «Массовая
            // операция выполнена», но запроса не было вовсе.
            const map = {enable:'enable', disable:'disable', reset:'reset_traffic',
                         revoke:'revoke', delete:'delete'};
            try {
              const r = await API.call('/api/clients/bulk', { method:'POST', body:{
                ids: forAll ? null : ids.map(Number),
                filter: forAll ? currentUserFilter() : {},
                action: map[act],
              }});
              toast(`Готово: затронуто клиентов — ${fmtN(r.clients||0)}`);
              await refreshDB();
            } catch(e){ toast('Не выполнилось: '+e.message, 'err'); }
          }});
      });
    }
  });
}

/* ── bulk: изменение полей ── */
function bulkUpdateDrawer(ids, forAll){
  const n = forAll ? `всех по фильтру (${fmtN(DB.usersFilteredTotal??DB.usersTotal??0)})` : ids.length + ' выбранных';
  const row = (id, label, control) => `
    <div class="bulk-field" style="--bulk-row:1">
      <label class="check" style="padding-top:8px"><input type="checkbox" data-en="${id}"></label>
      <div style="flex:1"><label style="font-size:12px;font-weight:600;color:var(--text-2);display:block;margin-bottom:6px">${label}</label>${control}</div>
    </div>`;
  openDrawer({
    title:'Массовое изменение полей', sub:`Для ${n} · отметьте, что менять`, icon:'edit', size:'lg',
    body:`
      ${row('tariff','Тариф',`<select class="inp" disabled>${DB.tariffs.map(t=>`<option ${usersTariffFilter===t.name?'selected':''}>${esc(t.name)}</option>`).join('')}</select>`)}
      ${row('expire','Срок действия',`<div class="inp-row"><select class="inp" disabled><option>Продлить на…</option><option>Установить дату…</option></select><input class="inp num" placeholder="30 дней" disabled></div>`)}
      ${row('traffic','Лимит трафика',`<div class="inp-group"><input class="inp num" placeholder="500" disabled><span class="suffix">GiB</span></div>`)}
      ${row('strategy','Стратегия сброса',`<select class="inp" disabled>${Object.values(STRATEGY).map(s=>`<option>${s}</option>`).join('')}</select>`)}
      ${row('hwid','Лимит устройств',`<input class="inp num" placeholder="3" disabled>`)}
      ${row('squads','Сквады',`<div class="chips-select">${DB.squadsInt.map(s=>`<span class="chip-opt" style="pointer-events:none;opacity:.5">${s.name}</span>`).join('')}</div>`)}
      ${row('tag','Тег',`<input class="inp" placeholder="WINBACK" disabled>`)}`,
    footer:`<div class="spacer"></div><button class="btn" data-close>Отмена</button><button class="btn primary" data-ok>Применить изменения</button>`,
    onMount(layer, close){
      layer.querySelectorAll('[data-en]').forEach(cb=>cb.addEventListener('change', ()=>{
        const box = cb.closest('div[style]').querySelectorAll('.inp,.chip-opt');
        box.forEach(el=>{ el.disabled = !cb.checked; el.style.pointerEvents = cb.checked?'auto':'none'; el.style.opacity = cb.checked?1:.5; });
      }));
      layer.querySelector('[data-ok]').addEventListener('click', ()=>{
        const en = [...layer.querySelectorAll('[data-en]:checked')];
        if(!en.length) return toast('Отметьте хотя бы одно поле','err');
        close();
        // Собираем только отмеченные поля: неотмеченное трогать нельзя,
        // иначе массовая правка тега заодно снесла бы всем лимиты.
        const set = {};
        const val = (id) => layer.querySelector(`[data-en="${id}"]`).closest('div[style]').querySelector('.inp');
        for (const cb of en) {
          const id = cb.dataset.en;
          if (id === 'tariff')   { const t = DB.tariffs.find(x=>x.name===val(id).value); if (t) set.tariff_id = Number(t.id); }
          if (id === 'expire')   {
            const box = cb.closest('div[style]');
            const mode = box.querySelector('select').value;
            const num  = box.querySelector('.inp.num').value;
            if (mode.startsWith('Продлить')) set.add_days = Number(num) || 0;
            else if (num) set.expires_at = new Date(num).toISOString();
          }
          if (id === 'traffic')  {
            const gb = parseFloat(val(id).value);
            if (!gb) set.unlimit_traffic = true; else set.traffic_limit_bytes = Math.round(gb * 1024 ** 3);
          }
          if (id === 'strategy') {
            const label = val(id).value;
            const code = Object.keys(STRATEGY).find(k=>STRATEGY[k]===label) || 'month';
            set.reset_strategy = code.toLowerCase();
          }
          if (id === 'hwid')     set.device_limit = Number(val(id).value) || 1;
          if (id === 'tag')      set.tag = val(id).value;
          if (id === 'squads')   set.squads = [...cb.closest('div[style]').querySelectorAll('.chip-opt.on')]
                                              .map(c=>Number(DB.squadsInt.find(s2=>s2.name===c.textContent.trim())?.id))
                                              .filter(Boolean);
        }
        confirmModal({title:'Применить изменения?', danger:forAll,
          text:`Будет изменено полей: <b>${en.length}</b> у ${n}.`, okText:'Применить',
          onOk:async ()=>{
            try {
              const r = await API.call('/api/clients/bulk', { method:'POST', body:{
                ids: forAll ? null : ids.map(Number),
                filter: forAll ? currentUserFilter() : {},
                action: 'update', set,
              }});
              toast(`Обновлено клиентов: ${fmtN(r.clients||0)}`);
              await refreshDB();
            } catch(e){ toast('Не обновилось: '+e.message, 'err'); }
          }});
      });
    }
  });
}

/* ── ПОДДЕРЖКА ── */
const TICKET_STATUS={open:['Нужен ответ','err'],pending:['Ожидает клиента','warn'],closed:['Закрыт','neutral']};
function mapTicket(t){return {id:String(t.id),clientId:String(t.client_id),user:t.username,subject:t.subject,status:t.status,prio:t.priority,updated:t.updated_at,lastMessage:t.last_message||''};}
registerPage({
  id:'support',title:'Поддержка',group:'Клиенты',icon:'chat',badge:()=>DB.tickets.filter(t=>t.status==='open').length||'',badgeTone:'alert',
  render(){return `<div class="page-head"><div><h1>Поддержка</h1><div class="desc">Переписка с клиентами, история обращений и результат доставки ответов в Telegram.</div></div>
    <div class="actions"><div class="seg" id="ticketFilters"><button class="on" data-status="unclosed">Открытые</button><button data-status="">Все</button><button data-status="closed">Закрытые</button></div></div></div>
    <div class="toolbar"><div class="search-inp">${I('search',14)}<input class="inp" id="ticketSearch" aria-label="Поиск обращений" placeholder="Тема или имя клиента"></div></div>
    <div class="tbl-wrap"><table class="tbl"><thead><tr><th>Обращение</th><th>Клиент</th><th>Приоритет</th><th>Статус</th><th>Обновлено</th><th></th></tr></thead><tbody id="ticketRows"></tbody></table>
      <div class="tbl-foot"><span id="ticketCount"></span><div class="pages" id="ticketPages"></div></div></div>`;},
  bind(root){
    let status='unclosed',offset=0,seq=0,timer;
    const draw=async()=>{
      const request=++seq;let data;
      try{data=await API.call('/api/tickets?'+new URLSearchParams({paginated:'true',limit:50,offset,status,search:root.querySelector('#ticketSearch').value.trim()}));}
      catch(e){if(request===seq&&root.isConnected){root.querySelector('#ticketRows').innerHTML=`<tr><td colspan="6">${esc(e.message)} <button class="btn sm" data-ticket-retry>Повторить</button></td></tr>`;root.querySelector('[data-ticket-retry]').onclick=draw;}return;}
      if(request!==seq||!root.isConnected)return;
      const rows=data.items.map(mapTicket);
      root.querySelector('#ticketRows').innerHTML=rows.map(t=>{const [label,tone]=TICKET_STATUS[t.status]||[t.status,'neutral'];return `<tr data-tk="${t.id}" class="click"><td><b>${esc(t.subject)}</b><span class="sub">${esc(t.lastMessage.slice(0,90))}</span></td><td>@${esc(t.user)}</td><td>${esc({high:'Высокий',normal:'Обычный',low:'Низкий'}[t.prio]||t.prio)}</td><td><span class="bdg ${tone}">${esc(label)}</span></td><td>${fmtDT(t.updated)}</td><td>${I('chevR',14)}</td></tr>`;}).join('')||'<tr><td colspan="6"><div class="empty"><b>Обращений не найдено</b><span>Измените поиск или фильтр статуса</span></div></td></tr>';
      root.querySelector('#ticketCount').textContent=data.total?`${offset+1}–${offset+rows.length} из ${data.total}`:'0 обращений';
      root.querySelector('#ticketPages').innerHTML=`<button class="btn sm" data-prev ${offset===0?'disabled':''}>Назад</button><button class="btn sm" data-next ${offset+50>=data.total?'disabled':''}>Далее</button>`;
      root.querySelector('[data-prev]').onclick=()=>{offset=Math.max(0,offset-50);draw();};root.querySelector('[data-next]').onclick=()=>{offset+=50;draw();};
      root.querySelectorAll('[data-tk]').forEach(b=>b.onclick=()=>ticketDrawer(rows.find(t=>t.id===b.dataset.tk)));
    };
    root.querySelectorAll('#ticketFilters button').forEach(b=>b.onclick=()=>{status=b.dataset.status;offset=0;root.querySelectorAll('#ticketFilters button').forEach(x=>x.classList.toggle('on',x===b));draw();});
    root.querySelector('#ticketSearch').oninput=()=>{offset=0;clearTimeout(timer);timer=setTimeout(draw,180);};draw();
  }
});
function ticketMessages(messages,user){
  return messages.map(m=>`<div style="margin-bottom:14px"><div style="padding:10px 12px;border-radius:12px;background:${m.author_kind==='admin'?'var(--accent-soft)':'var(--surface-2)'};white-space:pre-wrap">${esc(m.body)}</div><div class="hint" style="margin-top:4px">${esc(m.author_kind==='admin'?(m.admin_name||'Поддержка'):'@'+user)} · ${fmtDT(m.created_at)}${m.author_kind==='admin'?(m.delivered===false?' · <span style="color:var(--err)">Сохранено, не доставлено в Telegram</span>':m.delivered===true?' · Доставлено':' · Доставка не подтверждена'):''}</div></div>`).join('')||'<div class="empty">В этом обращении пока нет сообщений</div>';
}
async function ticketDrawer(t){
  let full;
  try{full=await API.call('/api/tickets/'+t.id);}catch(e){toast('Переписка не загрузилась: '+e.message,'err');return;}
  Object.assign(t,mapTicket(full));
  let u = userByName(t.user);
  if(!u){try{u=mapClient(await API.call('/api/clients/'+t.clientId));}catch(_){} }
  openDrawer({
    title:esc(t.subject), sub:'@'+esc(t.user)+(u?` · ${esc(u.tariff)} · LTV ${clientLtv(u)}`:''), icon:'chat', size:'lg',className:'support-dialog',initialFocus:'title',
    body:`
      <div style="display:flex;gap:8px;margin-bottom:16px;flex-wrap:wrap">
        <button class="btn sm" data-openuser>${I('user',12)} Карточка клиента</button>
        <button class="btn sm" data-tgrant>${I('gift',12)} Начислить дни</button>
        <button class="btn sm" data-treset>${I('refresh',12)} Сбросить трафик</button>
      </div>
      <div id="ticketMessages">${ticketMessages(full.messages,t.user)}</div>
      <div class="support-composer"><div class="field"><label>Ответ клиенту</label><textarea class="inp" rows="3" maxlength="4000" placeholder="Ответ клиенту… (уйдёт в Telegram от имени бота)"></textarea></div>
      <div style="display:flex;gap:6px;flex-wrap:wrap">
        ${['Проверьте версию приложения','Пришлите скриншот ошибки','Попробуйте другую локацию'].map(x=>`<button class="btn ghost sm" data-tpl="${esc(x)}">${x}</button>`).join('')}
      </div></div>`,
    footer:`<button class="btn sm" data-closetk>${I('check',12)} Закрыть тикет</button><div class="spacer"></div><button class="btn" data-close>Свернуть</button><button class="btn primary" data-reply>${I('send',13)} Ответить</button>`,
    onMount(layer, close){
      const history=layer.querySelector('#ticketMessages');history.scrollTop=history.scrollHeight;
      const reply=layer.querySelector('[data-reply]');
      if(DB.admin?.role==='readonly')layer.querySelectorAll('textarea,[data-reply],[data-closetk],[data-tgrant],[data-treset],[data-tpl]').forEach(b=>b.disabled=true);
      if(DB.admin?.role==='support')layer.querySelector('[data-tgrant]').hidden=true;
      reply.onclick=async()=>{
        const ta=layer.querySelector('textarea'),body=ta.value.trim();if(!body){toast('Введите ответ','err');return;}reply.disabled=true;
        try{
          const result=await API.call('/api/tickets/'+t.id+'/reply',{method:'POST',body:{body}});
          ta.value='';
          toast(result.delivered?'Ответ доставлен клиенту':'Ответ сохранён, но не доставлен в Telegram. Проверьте подключение бота.',result.delivered?'ok':'warn');
          const updated=await API.call('/api/tickets/'+t.id);
          if(layer.isConnected){history.innerHTML=ticketMessages(updated.messages,t.user);history.scrollTop=history.scrollHeight;}
        }catch(e){toast(e.message,'err');}finally{reply.disabled=false;}
      };
      // Шаблон вставляем в поле ответа, а не «куда-то»: иначе кнопка
      // рапортует об успехе, а текст никуда не попадает.
      layer.querySelectorAll('[data-tpl]').forEach(b=>b.addEventListener('click', ()=>{
        const ta = layer.querySelector('textarea');
        ta.value = (ta.value ? ta.value.trim() + ' ' : '') + b.dataset.tpl;
        ta.focus();
      }));

      layer.querySelector('[data-closetk]').addEventListener('click', async ()=>{
        try {
          await API.call('/api/tickets/'+t.id+'/close', { method:'POST' });
          close();
          toast('Тикет закрыт');
          await refreshDB();
        } catch(e){ toast('Не закрылся: '+e.message, 'err'); }
      });
      const ub = layer.querySelector('[data-openuser]');
      ub.addEventListener('click', ()=>openUserView(u||{id:t.clientId}));

      // Быстрые действия по клиенту прямо из тикета — самый частый сценарий
      // поддержки: не уходя из переписки, начислить дни или сбросить трафик.
      const tg = layer.querySelector('[data-tgrant]'), tr = layer.querySelector('[data-treset]');
      if (u) {
        tg.addEventListener('click', ()=>grantModal(u));
        tr.addEventListener('click', ()=>resetTraffic(u));
      } else {
        tg.disabled = true; tr.disabled = true;
      }
    }
  });
}


/* ═══════════ Операции над клиентом ═══════════
   Собраны в одном месте: их вызывают и меню строки, и карточка клиента.
   Разъехавшиеся копии — верный способ получить «в списке работает, в
   карточке нет». */

/* Начисление: продлить срок и добавить трафик.

   Отдельная операция, а не правка даты в карточке: сервер продлевает от
   БОЛЬШЕЙ из дат, поэтому у истёкшей подписки отсчёт идёт от сегодня, а
   не от даты в прошлом. */
function grantModal(u, closeParent){
  openModal({
    title:'Начисление · @'+esc(u.username), icon:'gift', size:'sm',
    body:`
      <div class="field"><label>Добавить дней</label>
        <input class="inp num" id="gDays" type="number" value="30" placeholder="30">
        <div class="quick-chips" id="gQuick"><button data-d="7">+7</button><button data-d="30">+30</button><button data-d="90">+90</button><button data-d="365">+365</button></div>
        <div class="hint">Отрицательное число отнимает дни.</div></div>
      <div class="field"><label>Добавить трафика</label>
        <div class="inp-group"><input class="inp num" id="gGb" type="number" step="1" placeholder="0"><span class="suffix">GiB</span></div>
        <div class="hint">У безлимитной подписки трафик не меняется — прибавлять к «без ограничения» нечего.</div></div>
      <div class="field"><label>Причина</label>
        <input class="inp" id="gReason" placeholder="компенсация за простой">
        <div class="hint">Попадёт в журнал действий.</div></div>`,
    footer:`<div class="spacer"></div><button class="btn" data-close>Отмена</button>
            <button class="btn primary" data-save>Начислить</button>`,
    onMount(layer, close){
      layer.querySelectorAll('#gQuick button').forEach(b=>b.addEventListener('click', ()=>{
        layer.querySelector('#gDays').value = b.dataset.d;
      }));
      const btn = layer.querySelector('[data-save]');
      btn.addEventListener('click', async ()=>{
        const days = parseInt(layer.querySelector('#gDays').value, 10) || 0;
        const gb = parseFloat(layer.querySelector('#gGb').value) || 0;
        if (!days && !gb) { toast('Укажите дни или трафик', 'err'); return; }
        btn.disabled = true;
        try {
          const r = await API.call('/api/clients/'+u.id+'/grant', { method:'POST',
            body:{ days, traffic_gb: gb, reason: layer.querySelector('#gReason').value.trim() || null } });
          close();
          if (closeParent) closeParent();
          toast('Начислено. Подписка до '+(r.expires_at ? fmtDate(r.expires_at) : '—'));
          await refreshDB();
        } catch(e){ toast('Не начислилось: '+e.message, 'err'); btn.disabled = false; }
      });
    }
  });
}

function resetTraffic(u){
  confirmModal({
    title:'Сбросить трафик?', danger:false,
    text:`Счётчик <b>@${esc(u.username)}</b> обнулится (${fmtB(u.usedGb)} → 0).`,
    okText:'Сбросить',
    onOk:async ()=>{
      try {
        await API.call('/api/clients/'+u.id+'/reset-traffic', { method:'POST' });
        toast('Трафик сброшен');
        await refreshDB();
      } catch(e){ toast('Не получилось: '+e.message, 'err'); }
    }
  });
}

function revokeSub(u){
  confirmModal({
    title:'Отозвать подписку?',
    text:`Short-UUID <b>${esc(u.shortUuid)}</b> будет заменён — старая ссылка перестанет работать на всех устройствах.`,
    okText:'Отозвать',
    onOk:async ()=>{
      try {
        const r = await API.call('/api/clients/'+u.id+'/revoke', { method:'POST' });
        toast('Подписка отозвана. Новая ссылка: '+(r.subscription_url || 'обновите карточку'));
        await refreshDB();
      } catch(e){ toast('Не получилось: '+e.message, 'err'); }
    }
  });
}

function toggleClient(u){
  const on = u.status === 'DISABLED';
  confirmModal({
    title: on ? 'Включить клиента?' : 'Отключить клиента?',
    danger: !on,
    text: on
      ? `Доступ <b>@${esc(u.username)}</b> восстановится на всех нодах.`
      : `Подписка <b>@${esc(u.username)}</b> перестанет работать до ручного включения.`,
    okText: on ? 'Включить' : 'Отключить',
    onOk:async ()=>{
      try {
        await API.call('/api/clients/'+u.id, { method:'PATCH', body:{ status: on ? 'active' : 'disabled' } });
        toast(on ? 'Клиент включён' : 'Клиент отключён');
        await refreshDB();
      } catch(e){ toast('Не получилось: '+e.message, 'err'); }
    }
  });
}

function deleteClient(u, closeParent){
  confirmModal({
    title:`Удалить @${esc(u.username)}?`, danger:true,
    text:'Подписка и доступы удалятся. История платежей остаётся в архиве. Действие необратимо.',
    okText:'Удалить навсегда',
    onOk:async ()=>{
      try {
        await API.call('/api/clients/'+u.id, { method:'DELETE' });
        if (closeParent) closeParent();
        toast('Клиент удалён', 'info');
        await refreshDB();
      } catch(e){ toast('Не удалился: '+e.message, 'err'); }
    }
  });
}

function unbindDevice(u, hwid){
  confirmModal({
    title:'Отвязать устройство?',
    text:`HWID <b>${esc(hwid)}</b> будет удалён и слот освободится. Устройство сможет привязаться снова при следующем обновлении. Чтобы закрыть доступ по старой ссылке, отзовите подписку.`,
    okText:'Отвязать',
    onOk:async ()=>{
      try {
        await API.call('/api/clients/'+u.id+'/devices/'+encodeURIComponent(hwid), { method:'DELETE' });
        toast('Устройство отвязано');
        await refreshDB();
      } catch(e){ toast('Не получилось: '+e.message, 'err'); }
    }
  });
}


/* Скрытие колонок таблицы клиентов.

   Прячем по заголовку, а не по индексу: колонки со временем добавляют и
   переставляют, и привязка к номеру тихо начнёт прятать не то. */
function applyColumns(){
  const hidden = new Set(JSON.parse(localStorage.getItem('sn_cols') || '[]'));
  const table = document.querySelector('#uRows')?.closest('table');
  if (!table) return;

  const heads = [...table.querySelectorAll('thead th')];
  heads.forEach((th, i) => {
    const name = th.textContent.trim();
    const hide = hidden.has(name);
    th.style.display = hide ? 'none' : '';
    table.querySelectorAll('tbody tr').forEach(tr => {
      const td = tr.children[i];
      if (td) td.style.display = hide ? 'none' : '';
    });
  });
}

function clientLtv(u){return u.ltvByCurrency?.length?u.ltvByCurrency.map(w=>fmtMoney(minorToUnits(w.amount_minor,w.currency),w.currency)).join(" · "):fmtMoney(u.ltv,u.ltvCurrency||DB.currency);}

function clientCodeModal(u){
  openModal({title:'Код доступа · @'+esc(u.username),icon:'key',size:'sm',body:`<p class="hint" data-code-state>Загружаем сведения о коде…</p><div class="copy-block client-code-secret" data-code-box hidden><span class="txt mono" data-code-value></span></div><div class="client-actions client-code-controls"><button class="btn primary" data-code-show disabled>${I('eye',14)} Показать код</button><button class="btn" data-code-copy hidden>${I('copy',14)} Скопировать</button><button class="btn" data-code-reset disabled>${I('rotate',14)} Сбросить код</button></div><p class="hint" role="status" data-code-feedback></p><p class="hint">Сброс отменит старый код и завершит сеансы сайта. Подписка и покупки сохранятся. Просмотр и сброс записываются в журнал.</p>`,footer:'<button class="btn" data-close>Закрыть</button>',onMount:async layer=>{
    const status=layer.querySelector('[data-code-state]'),show=layer.querySelector('[data-code-show]'),reset=layer.querySelector('[data-code-reset]'),copy=layer.querySelector('[data-code-copy]'),feedback=layer.querySelector('[data-code-feedback]'),value=layer.querySelector('[data-code-value]'),box=layer.querySelector('[data-code-box]');let code='',visible=false;
    const display=next=>{code=next;visible=true;box.hidden=false;value.textContent=code;copy.hidden=false;show.disabled=false;show.textContent='Скрыть код';};
    try{const r=await API.call('/api/clients/'+u.id+'/cabinet-code');status.textContent=!r.issued?'Код ещё не выпущен.':r.viewable?'Код выпущен'+(r.updated_at?' · '+fmtDT(r.updated_at):''):'Старый код хранится только как хеш. Просмотр станет доступен после входа клиента по коду, либо после сброса.';show.disabled=!r.viewable;reset.disabled=false;reset.textContent=r.issued?'Сбросить код':'Выпустить код';}catch(e){status.textContent=e.message;}
    show.onclick=async()=>{if(visible){value.textContent='•••• •••• •••• •••• ••••';visible=false;show.textContent='Показать код';return;}show.disabled=true;try{const r=await API.call('/api/clients/'+u.id+'/cabinet-code',{method:'POST',body:{}});if(r.code)display(r.code);else feedback.textContent='Код пока недоступен для просмотра.';}catch(e){feedback.textContent=e.message;}finally{show.disabled=false;}};
    copy.onclick=async()=>{feedback.textContent='';try{await navigator.clipboard.writeText(code);copy.innerHTML=I('check',14)+'Код скопирован';feedback.textContent='Код скопирован в буфер обмена.';}catch{feedback.textContent='Не удалось скопировать. Выделите код и скопируйте вручную.';value.textContent=code;visible=true;show.textContent='Скрыть код';}};
    reset.onclick=()=>confirmModal({title:'Сбросить код · @'+esc(u.username),text:'Старый код перестанет работать, все сеансы клиента на сайте завершатся. Подписка и покупки сохранятся.',okText:'Сбросить код',onOk:async()=>{reset.disabled=true;try{const r=await API.call('/api/clients/'+u.id+'/cabinet-code/reset',{method:'POST',body:{confirm:true}});display(r.code);status.textContent='Новый код выпущен';copy.innerHTML=I('copy',14)+'Скопировать';feedback.textContent='Старый код отменён. Новый код готов для входа в этот аккаунт.';}catch(e){feedback.textContent=e.message;}finally{reset.disabled=false;}}});
  }});
}
