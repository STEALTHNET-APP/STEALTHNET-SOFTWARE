/* ═══════════ PAGES: Инструменты, Инфра-биллинг, Настройки ═══════════ */
'use strict';

/* Inspectors search the complete database, not the first loaded page. */
function inspectorPage(kind) {
  const hw=kind==='hwid', title=hw?'Инспектор HWID':'Инспектор SRH';
  return {
    id:hw?'hwid-inspector':'srh-inspector', title, group:'Инструменты', icon:hw?'fingerprint':'history',
    render(){return `<div class="page-head"><div><h1>${title}</h1><div class="desc">${hw?'Устройства клиентов, поиск по HWID и модели, обнаружение общего устройства у разных аккаунтов.':'История запросов подписки: клиент, приложение, IP и результат выдачи.'}</div></div></div>
      ${hw?'<div class="grid g4" data-inspector-summary></div>':''}
      <div class="toolbar"><div class="search-inp">${I('search',14)}<input class="inp" data-inspector-search placeholder="${hw?'HWID, модель, @username…':'Клиент, short-ID, IP, User-Agent…'}"></div>
      ${hw?'<select class="inp" data-inspector-platform style="width:170px"><option value="">Все платформы</option><option>iOS</option><option>Android</option><option>Windows</option><option>macOS</option><option>Linux</option></select>':''}
      <button class="btn" data-inspector-refresh>${I('refresh',14)} Обновить</button></div>
      <div class="tbl-wrap"><table class="tbl"><thead><tr>${(hw?['HWID','Клиент','Устройство','Приложение','Первый вход','Последняя активность','']:['Время','Клиент','Short ID','User-Agent','IP','Ответ']).map(x=>`<th>${x}</th>`).join('')}</tr></thead><tbody data-inspector-rows></tbody></table></div>
      <div class="toolbar" style="margin-top:12px"><span class="sub-note" data-inspector-count role="status">Загрузка…</span><div class="spacer"></div><button class="btn sm" data-inspector-prev>Назад</button><button class="btn sm" data-inspector-next>Далее</button></div>
      <div class="sub-note">${hw?'Отвязка освобождает слот; при следующем обновлении устройство может зарегистрироваться снова. Для отзыва старой ссылки используйте карточку клиента.':'Частые обновления с одного IP могут быть нормой. Проверяйте историю вместе с устройствами и активностью клиента.'}</div>`;},
    bind(root){
      let offset=0,total=0,seq=0,timer;const limit=50;
      const rows=root.querySelector('[data-inspector-rows]'), count=root.querySelector('[data-inspector-count]');
      const draw=async()=>{
        const run=++seq;count.textContent='Загрузка…';const params=new URLSearchParams({paginated:'true',limit:String(limit),offset:String(offset)});
        const q=root.querySelector('[data-inspector-search]').value.trim().replace(/^@/,'');if(q)params.set('q',q);
        const platform=root.querySelector('[data-inspector-platform]')?.value;if(platform)params.set('platform',platform);
        try{
          const result=await API.call((hw?'/api/devices':'/api/srh')+'?'+params);if(run!==seq||!root.isConnected)return;
          total=result.total;rows.innerHTML=result.items.map(r=>hw?`<tr><td class="mono">${esc(r.hwid)}${r.shared_by>1?` <span class="bdg warn">${r.shared_by} клиента</span>`:''}</td><td><button class="link" data-client="${r.client_id}">@${esc(r.username)}</button></td><td>${esc(r.model||'—')} <span class="sub">${esc(r.platform||'—')}</span></td><td>${esc(r.app_version||'—')}</td><td>${fmtDT(r.first_seen_at)}</td><td>${fmtDT(r.last_seen_at)}</td><td><button class="btn ghost icon-only" data-unbind-client="${r.client_id}" data-hwid="${esc(r.hwid)}" title="Отвязать устройство">${I('trash',13)}</button></td></tr>`:`<tr><td>${fmtDT(r.requested_at)}</td><td><button class="link" data-client="${r.client_id}">@${esc(r.username)}</button></td><td class="mono">${esc(r.short_id)}</td><td>${esc(r.user_agent||'—')}</td><td class="mono">${esc(r.ip||'—')}</td><td><span class="bdg info">${esc(r.response_code||'—')}</span></td></tr>`).join('')||`<tr><td colspan="${hw?7:6}" class="empty">Записей по этому запросу нет</td></tr>`;
          count.textContent=total?`${offset+1}–${Math.min(offset+limit,total)} из ${fmtN(total)}`:'Записей нет';
          root.querySelector('[data-inspector-prev]').disabled=offset===0;root.querySelector('[data-inspector-next]').disabled=offset+limit>=total;
          if(hw){const s=result.summary;root.querySelector('[data-inspector-summary]').innerHTML=[['Всего устройств',fmtN(s.devices)],['Клиентов с устройствами',fmtN(s.clients)],['Среднее на клиента',s.clients?(s.devices/s.clients).toFixed(2):'0'],['Общие HWID',fmtN(s.shared)]].map(([label,value])=>`<div class="card"><div class="label">${label}</div><div class="metric num">${value}</div></div>`).join('');}
          rows.querySelectorAll('[data-client]').forEach(b=>b.onclick=()=>openUserView({id:b.dataset.client}));
          rows.querySelectorAll('[data-unbind-client]').forEach(b=>b.onclick=()=>unbindDevice({id:b.dataset.unbindClient},b.dataset.hwid));enhancePanelUI(root);
        }catch(e){if(run===seq){rows.innerHTML=`<tr><td colspan="${hw?7:6}" role="alert">Не удалось загрузить: ${esc(e.message)}</td></tr>`;count.textContent='Повторите загрузку';}}
      };
      root.querySelector('[data-inspector-search]').oninput=()=>{clearTimeout(timer);offset=0;timer=setTimeout(draw,200);};
      const platform=root.querySelector('[data-inspector-platform]');if(platform)platform.onchange=()=>{offset=0;draw();};
      root.querySelector('[data-inspector-refresh]').onclick=()=>draw();root.querySelector('[data-inspector-prev]').onclick=()=>{offset=Math.max(0,offset-limit);draw();};root.querySelector('[data-inspector-next]').onclick=()=>{if(offset+limit<total){offset+=limit;draw();}};return draw();
    }
  };
}
registerPage(inspectorPage('hwid'));
registerPage(inspectorPage('srh'));

/* ── КТО СЕЙЧАС В СЕТИ ── */
registerPage({
  id:'sessions', title:'Кто сейчас в сети', group:'Инструменты', icon:'pulse',
  render(){
    return `
    <div class="page-head">
      <div><h1>Кто сейчас в сети</h1><div class="desc">Клиенты с трафиком за последние минуты и загрузка нод.</div></div>
      <div class="actions"><button class="btn" id="seRefresh">${I('refresh',14)} Обновить</button></div>
    </div>
    <div class="grid g2" style="align-items:start">
      <div class="card">
        <div class="section-h" style="margin:0 0 12px"><h2>${I('server',14)} Ноды</h2></div>
        <div id="seNodes"></div>
      </div>
      <div class="card">
        <div class="section-h" style="margin:0 0 12px"><h2>${I('users2',14)} Клиенты</h2></div>
        <div id="seClients"></div><div class="tbl-foot"><span id="seCount" class="sub-note"></span><button class="btn sm" id="seMore" hidden>Показать ещё</button></div>
      </div>
    </div>
    <div class="card" style="margin-top:var(--panel-gap,14px)">
      <div class="section-h" style="margin:0 0 10px"><h2>${I('info',14)} Почему не список соединений</h2></div>
      <p class="sub-note">Xray не отдаёт наружу перечень активных соединений: он ведёт только счётчики трафика по клиентам и инбаундам. «В сети» здесь означает «был трафик за последнее окно опроса» — это честное приближение, и по нему видно то же самое: кто пользуется и какая нода нагружена. Разрывать отдельное соединение движок тоже не умеет — чтобы отключить клиента, его выключают в карточке.</p>
    </div>`;
  },
  async bind(root){
    let activityOffset=0,activityRows=[];
    const draw = async () => {
      let nodes;
      try { nodes = await API.call('/api/sessions'); }
      catch(e){ toast('Не загрузилось: '+e.message, 'err'); return; }

      const total = nodes.reduce((a,n)=>a + (n.online_count||0), 0);
      root.querySelector('#seNodes').innerHTML = nodes.map(n=>`
        <div class="info-row">
          <span class="v" style="width:38px">${ccChip(n.country_code)}</span>
          <span class="v mono" style="flex:1;font-size:12.5px">${esc(n.node)}</span>
          <div class="progress" style="width:110px"><i style="width:${total?Math.round((n.online_count||0)/total*100):0}%"></i></div>
          <span class="num" style="width:52px;text-align:right">${fmtN(n.online_count||0)}</span>
        </div>`).join('') || '<div class="sub-note">Нод нет.</div>';

      try {
        const data=await API.call('/api/sessions/activity?'+new URLSearchParams({offset:activityOffset,limit:50}));
        activityRows=activityOffset?[...activityRows,...data.items]:data.items;
        root.querySelector('#seClients').innerHTML=activityRows.map(activityRecord).join('')||'<p class="sub-note">За последние 5 минут агент не передал трафик клиентов.</p>';
        root.querySelector('#seCount').textContent=`Показано ${activityRows.length} из ${data.total} пар клиент / нода`;
        root.querySelector('#seMore').hidden=activityRows.length>=data.total;
        root.querySelectorAll('[data-open-client]').forEach(b=>b.onclick=()=>openUserView({id:b.dataset.openClient}));
      }catch(e){root.querySelector('#seClients').innerHTML=`<p role="alert">Активность не загрузилась: ${esc(e.message)}</p>`;root.querySelector('#seMore').hidden=true;}
    };
    root.querySelector('#seRefresh').addEventListener('click', ()=>{activityOffset=0;draw();});
    root.querySelector('#seMore').onclick=()=>{activityOffset+=50;draw();};
    await draw();
  }
});

/* ── ТОРРЕНТ-РЕПОРТЫ ── */
registerPage({
  id:'torrent-reports', title:'Торрент-репорты', group:'Инструменты', icon:'magnet',
  render(){
    return `
    <div class="page-head">
      <div><h1>Торрент-репорты</h1><div class="desc">Кто пытался качать торренты через ваши ноды. Сама раздача при этом блокируется правилом маршрутизации.</div></div>
      <div class="actions">
        <select class="inp" id="trDays" style="width:140px">
          <option value="1">за сутки</option><option value="7" selected>за 7 дней</option><option value="30">за 30 дней</option>
        </select>
      </div>
    </div>
    <div class="card" style="padding:0"><div class="tbl-wrap"><table class="tbl readonly">
      <thead><tr><th>Дата</th><th>Клиент</th><th>Нода</th><th>Срабатываний</th><th>Последняя цель</th><th style="width:36px"></th></tr></thead>
      <tbody id="trRows"></tbody></table></div></div>
    <div class="sub-note" style="margin-top:10px">${I('info',12)} Считаются срабатывания правила блокировки: агент читает журнал движка и присылает счётчики, сами строки журнала в панель не уезжают.</div>`;
  },
  async bind(root){
    const draw = async () => {
      let list;
      try { list = await API.call('/api/reports/torrents?days=' + root.querySelector('#trDays').value); }
      catch(e){ toast('Не загрузилось: '+e.message, 'err'); return; }

      root.querySelector('#trRows').innerHTML = list.map(r=>`
        <tr>
          <td>${fmtDate(r.day)}</td>
          <td><span class="link" data-tuser="${r.client_id}">@${esc(r.username)}</span></td>
          <td class="mono" style="font-size:12px">${esc(r.node || '—')}</td>
          <td class="num"><span class="bdg ${r.hits > 50 ? 'err' : 'warn'}">${fmtN(r.hits)}</span></td>
          <td class="mono" style="font-size:11.5px;color:var(--text-3)">${esc(r.last_target || '—')}</td>
          <td><button class="btn ghost icon-only" data-tmenu="${r.client_id}">${I('more',13)}</button></td>
        </tr>`).join('') || `<tr><td colspan="6" style="padding:24px">
          <div class="empty">${I('magnet',30)}<b>Срабатываний нет</b>
            <span>Либо никто не пробовал, либо агент ещё не прислал отчёт — он читает журнал раз в несколько секунд.</span></div></td></tr>`;

      root.querySelectorAll('[data-tuser]').forEach(b=>b.addEventListener('click', ()=>{
        openUserView({id:b.dataset.tuser});
      }));
      root.querySelectorAll('[data-tmenu]').forEach(b=>b.addEventListener('click', e=>{
        const row = list.find(x=>String(x.client_id)===b.dataset.tmenu);
        const u = {id:b.dataset.tmenu,username:row?.username,status:'ACTIVE'};
        menu(e.currentTarget, [
          {label:'Карточка клиента', icon:'user', onClick:()=>openUserView(u)},
          {label:'Отключить клиента', icon:'ban', danger:true, onClick:()=>toggleClient(u)},
        ]);
      }));
    };
    root.querySelector('#trDays').addEventListener('change', draw);
    await draw();
  }
});

/* ── HTTP-СТАТИСТИКА ── */
registerPage({
  id:'http-stats', title:'HTTP-статистика', group:'Инструменты', icon:'globe',
  render(){
    return `
    <div class="page-head">
      <div><h1>HTTP-статистика</h1><div class="desc">Домены, куда ходят клиенты через ваши ноды.</div></div>
      <div class="actions">
        <select class="inp" id="hsDays" style="width:140px">
          <option value="1">за сутки</option><option value="7" selected>за 7 дней</option><option value="30">за 30 дней</option>
        </select>
      </div>
    </div>
    <div class="card" style="margin-bottom:12px">
      <div class="switch-row"><label class="switch"><input type="checkbox" id="hsToggle"><span class="tr"></span></label>
        <div class="sw-txt"><b>Собирать домены</b>
          <span>Выключено по умолчанию. Это журнал посещённых сайтов ваших клиентов — самое ценное, что может утечь из VPN-панели.</span></div></div>
      <div class="field" style="margin-top:12px;max-width:260px"><label>Хранить</label>
        <div class="inp-group"><input class="inp num" id="hsKeep" type="number" min="1" max="365"><span class="suffix">дней</span></div>
        <div class="hint">Старше — удаляется автоматически. Чем дольше журнал лежит, тем хуже последствия утечки.</div></div>
      <button class="btn sm" style="margin-top:10px" id="hsSave">${I('check',12)} Сохранить</button>
    </div>
    <div class="card" style="padding:0"><div id="hsBody" class="tbl-wrap sub-note" tabindex="0" aria-label="HTTP-статистика" style="padding:20px">Загружаем…</div></div>`;
  },
  async bind(root){
    const draw = async () => {
      let d, keep = 30;
      try {
        const st = await API.call('/api/settings');
        keep = parseInt(st['reports.retention_days'], 10) || 30;
        d = await API.call('/api/reports/domains?days=' + root.querySelector('#hsDays').value);
      } catch(e){ root.querySelector('#hsBody').textContent = 'Не загрузилось: '+e.message; return; }

      root.querySelector('#hsToggle').checked = d.enabled;
      root.querySelector('#hsKeep').value = keep;

      if (!d.enabled) {
        root.querySelector('#hsBody').innerHTML = `<div class="empty" style="padding:32px">${I('globe',30)}
          <b>Сбор выключен</b><span>Включите тумблер выше, если это осознанное решение для вашего сервиса.</span></div>`;
        return;
      }
      const max = Math.max(1, ...d.items.map(x=>x.hits));
      root.querySelector('#hsBody').innerHTML = d.items.length ? `
        <table class="tbl"><thead><tr><th>Домен</th><th style="width:45%"></th><th style="text-align:right">Обращений</th></tr></thead>
        <tbody>${d.items.map(x=>`
          <tr><td class="mono" style="font-size:12.5px">${esc(x.domain)}</td>
            <td><div class="progress"><i style="width:${x.hits/max*100}%"></i></div></td>
            <td class="num" style="text-align:right">${fmtN(x.hits)}</td></tr>`).join('')}</tbody></table>`
        : `<div class="empty" style="padding:32px">${I('globe',30)}<b>Пока пусто</b>
             <span>Сбор включён, но агент ещё не прислал данные — или клиенты не ходили никуда за выбранный период.</span></div>`;
    };

    root.querySelector('#hsDays').addEventListener('change', draw);
    root.querySelector('#hsToggle').addEventListener('change', e=>{
      const on = e.target.checked;
      e.target.checked = !on;
      if (!on) return applyHttp(false, draw);
      confirmModal({
        title:'Включить сбор доменов?',
        text:'Панель начнёт хранить, на какие сайты ходят ваши клиенты. Это персональные данные: убедитесь, что так можно по вашей политике и по закону страны, где вы работаете.',
        okText:'Включить', danger:true,
        onOk:()=>applyHttp(true, draw),
      });
    });
    root.querySelector('#hsSave').addEventListener('click', async ()=>{
      const days = parseInt(root.querySelector('#hsKeep').value, 10);
      if (!(days >= 1 && days <= 365)) { toast('Срок хранения — от 1 до 365 дней', 'err'); return; }
      try {
        await API.call('/api/settings', { method:'PATCH', body:{ 'reports.retention_days': days } });
        toast('Сохранено');
      } catch(e){ toast('Не сохранилось: '+e.message, 'err'); }
    });
    await draw();
  }
});

async function applyHttp(on, onDone){
  try {
    await API.call('/api/settings', { method:'PATCH', body:{ 'reports.http_enabled': on } });
    toast(on ? 'Сбор включён — ноды подхватят в течение 15 секунд' : 'Сбор выключен');
    await onDone();
  } catch(e){ toast('Не сохранилось: '+e.message, 'err'); }
}

/* ── ИНФРА-БИЛЛИНГ ── */
registerPage({
  id:'infra-billing', title:'Инфра-биллинг', group:'CRM', icon:'dollar',
  render(){
    return `
    <div class="page-head">
      <div><h1>Инфра-биллинг</h1><div class="desc">Во что обходится парк серверов и сколько остаётся после расходов.</div></div>
      <div class="actions">
        <button class="btn" id="newProv">${I('plus',14)} Провайдер</button>
        <button class="btn primary" id="newRec">${I('plus',14)} Записать оплату</button>
      </div>
    </div>
    <div class="grid g4" id="ibSummary" style="margin-bottom:16px"></div>
    <div class="tabs" id="ibTabs">
      <button class="on" data-t="nodes">Ноды и списания</button>
      <button data-t="prov">Провайдеры</button>
      <button data-t="hist">История оплат</button>
    </div>
    <div class="tab-pane on" data-p="nodes">
      <div class="tbl-wrap"><table class="tbl readonly">
        <thead><tr><th>Нода</th><th>Провайдер</th><th>Стоимость</th><th>День списания</th><th>Следующее списание</th><th style="width:36px"></th></tr></thead>
        <tbody id="ibNodes"></tbody>
      </table></div>
      <div class="sub-note" style="margin-top:10px">${I('info',12)} Стоимость и день списания задаются в настройках ноды — здесь их видно все сразу.</div>
    </div>
    <div class="tab-pane" data-p="prov"><div class="grid g3" id="ibProv"></div></div>
    <div class="tab-pane" data-p="hist">
      <div class="tbl-wrap"><table class="tbl readonly">
        <thead><tr><th>Дата</th><th>Провайдер</th><th>Нода</th><th>Сумма</th><th style="width:36px"></th></tr></thead>
        <tbody id="ibHist"></tbody>
      </table></div>
    </div>`;
  },
  async bind(root){
    const tabs = root.querySelector('#ibTabs');
    tabs.querySelectorAll('button').forEach(b=>b.addEventListener('click', ()=>{
      tabs.querySelectorAll('button').forEach(x=>x.classList.remove('on')); b.classList.add('on');
      root.querySelectorAll('.tab-pane').forEach(p=>p.classList.toggle('on', p.dataset.p===b.dataset.t));
    }));

    // Сколько дней до списания. Если день уже прошёл — считаем до
    // следующего месяца, иначе «через −5 дней» выглядит как ошибка.
    const daysUntil = (day) => {
      if (!day) return null;
      const now = new Date(), cur = now.getDate();
      const inMonth = new Date(now.getFullYear(), now.getMonth() + 1, 0).getDate();
      const d = Math.min(day, inMonth);
      return d >= cur ? d - cur : inMonth - cur + d;
    };

    const draw = async () => {
      let sum, provs, hist;
      try {
        [sum, provs, hist] = await Promise.all([
          API.call('/api/infra/summary'),
          API.call('/api/infra/providers'),
          API.call('/api/infra/payments?limit=100'),
        ]);
      } catch(e){ toast('Не загрузилось: '+e.message, 'err'); return; }

      const money = (minor) => fmtMoney(minorToUnits(minor, sum.currency), sum.currency);

      root.querySelector('#ibSummary').innerHTML = [
        ['Инфраструктура / месяц', money(sum.monthly_cost_minor), `${sum.node_count} нод`],
        ['Выручка за 30 дней', money(sum.revenue_minor), 'успешные платежи'],
        ['Остаётся', money(sum.profit_minor),
          // Маржа без выручки не определена: показывать «100%» при нулевом
          // доходе — прямой обман.
          sum.margin_percent == null ? 'выручки пока нет' : `маржа ${sum.margin_percent}%`],
        ['Средняя нода', money(sum.avg_node_minor), 'в месяц'],
      ].map(([t,v,d])=>`
        <div class="card" style="padding:13px 16px"><div class="label">${t}</div>
          <div class="metric num" style="font-size:20px">${v}</div>
          <div class="sub-note">${esc(d)}</div></div>`).join('');

      root.querySelector('#ibNodes').innerHTML = DB.nodes.map(n=>{
        const d = daysUntil(n.billDay);
        return `<tr>
          <td><div class="cell-main">${ccChip(n.cc)}<b class="mono">${esc(n.name)}</b></div></td>
          <td>${n.infraProvider ? providerBadge(n.infraProvider, n.infraProviderLogo) : '<span class="sub-note">не указан</span>'}</td>
          <td class="num">${n.costMinor ? money(n.costMinor)+'/мес' : '<span class="sub-note">—</span>'}</td>
          <td class="num">${n.billDay ? n.billDay+'-е число' : '<span class="sub-note">—</span>'}</td>
          <td>${d == null ? '<span class="sub-note">—</span>'
                : `<span class="bdg ${d<=5?'warn':'neutral'}">${d<=5?I('alert',11)+' ':''}через ${d} дн</span>`}</td>
          <td><button class="btn ghost icon-only" data-ibnode="${n.id}">${I('more',14)}</button></td>
        </tr>`;
      }).join('') || `<tr><td colspan="6" class="sub-note" style="padding:20px">Нод пока нет.</td></tr>`;

      root.querySelector('#ibProv').innerHTML = provs.map(p=>`
        <div class="card">
          <div style="display:flex;align-items:center;gap:10px">
            ${providerLogo(p.name, p.logo_url, 34)}
            <div style="flex:1;min-width:0"><b style="font-size:13.5px">${esc(p.name)}</b>
              <span class="sub mono">${esc(p.url || '—')}</span></div>
            <button class="btn ghost icon-only" data-ibprov="${p.id}">${I('more',14)}</button>
          </div>
          <div style="display:flex;gap:16px;margin-top:12px">
            <div><div class="label" style="font-size:9.5px">Нод</div>
              <div class="num" style="font-size:16px;font-weight:600">${p.node_count}</div></div>
            <div><div class="label" style="font-size:9.5px">В месяц</div>
              <div class="num" style="font-size:16px;font-weight:600">${money(p.monthly_minor)}</div></div>
          </div>
          ${p.note ? `<div class="sub-note" style="margin-top:10px">${esc(p.note)}</div>` : ''}
        </div>`).join('') || `<div class="sub-note">Провайдеров пока нет.</div>`;

      root.querySelector('#ibHist').innerHTML = hist.map(r=>`
        <tr>
          <td>${fmtDate(r.paid_on)}</td>
          <td>${esc(r.provider_name || '—')}</td>
          <td class="mono" style="font-size:12px">${esc(r.node_name || '—')}</td>
          <td class="num">${fmtMoney(minorToUnits(r.amount_minor, r.currency), r.currency)}</td>
          <td><button class="btn ghost icon-only" data-ibdel="${r.id}">${I('trash',13)}</button></td>
        </tr>`).join('') || `<tr><td colspan="5" class="sub-note" style="padding:20px">Оплат ещё не записывали.</td></tr>`;

      root.querySelectorAll('[data-ibnode]').forEach(b=>b.addEventListener('click', e=>{
        const n = DB.nodes.find(x=>x.id===e.currentTarget.dataset.ibnode);
        menu(e.currentTarget, [
          {label:'Расходы ноды', icon:'dollar', onClick:()=>nodeCostModal(n, provs, draw)},
          {label:'Записать оплату', icon:'plus', onClick:()=>infraPaymentModal(provs, draw, n)},
          {label:'Настройки ноды', icon:'settings', onClick:()=>editNodeDrawer(n)},
        ]);
      }));

      root.querySelectorAll('[data-ibprov]').forEach(b=>b.addEventListener('click', e=>{
        const p = provs.find(x=>String(x.id)===e.currentTarget.dataset.ibprov);
        menu(e.currentTarget, [
          {label:'Редактировать', icon:'edit', onClick:()=>infraProviderModal(p, draw)},
          '-',
          {label:'Удалить', icon:'trash', danger:true, onClick:()=>confirmModal({
            title:`Удалить ${esc(p.name)}?`,
            text:'Ноды останутся, у них просто пропадёт привязка к провайдеру.',
            okText:'Удалить',
            onOk:async ()=>{
              try {
                await API.call('/api/infra/providers/'+p.id, { method:'DELETE' });
                toast('Провайдер удалён');
                await draw();
              } catch(e){ toast('Не удалился: '+e.message, 'err'); }
            }})},
        ]);
      }));

      root.querySelectorAll('[data-ibdel]').forEach(b=>b.addEventListener('click', ()=>confirmModal({
        title:'Удалить запись об оплате?', text:'Расход исчезнет из истории.', okText:'Удалить',
        onOk:async ()=>{
          try {
            await API.call('/api/infra/payments/'+b.dataset.ibdel, { method:'DELETE' });
            toast('Запись удалена');
            await draw();
          } catch(e){ toast('Не удалилась: '+e.message, 'err'); }
        }
      })));

      root.querySelector('#newProv').onclick = ()=>infraProviderModal(null, draw);
      root.querySelector('#newRec').onclick = ()=>infraPaymentModal(provs, draw);
    };

    await draw();
  }
});

/* Провайдер инфраструктуры — хостер, у которого арендованы серверы. */
function infraProviderModal(p, onSaved){
  const isNew = !p;
  openModal({
    title: isNew ? 'Новый провайдер' : 'Провайдер · '+esc(p.name), icon:'server', size:'sm',
    body:`
      <div class="field"><label>Название <span class="req">*</span></label>
        <input class="inp" id="ipName" value="${isNew?'':esc(p.name)}" placeholder="Hetzner"></div>
      <div class="field"><label>Сайт / панель</label>
        <input class="inp mono" id="ipUrl" value="${isNew?'':esc(p.url||'')}" placeholder="https://console.hetzner.cloud"></div>
      <div class="field"><label>Логотип</label>
        <div class="inp-group">
          <input class="inp mono" id="ipLogo" value="${isNew?'':esc(p.logo_url||'')}" placeholder="https://…/logo.png">
          <span class="suffix" id="ipLogoPrev" style="padding:4px 10px">${providerLogo(isNew?'':p.name, isNew?'':p.logo_url, 22)}</span>
        </div>
        <div class="hint">Необязательно. Без него рисуется монограмма — цвет выводится из названия, так что хостер узнаётся по значку.</div></div>
      <div class="field"><label>Заметка</label>
        <input class="inp" id="ipNote" value="${isNew?'':esc(p.note||'')}" placeholder="оплата картой, счёт 5-го числа"></div>`,
    footer:`<div class="spacer"></div><button class="btn" data-close>Отмена</button>
            <button class="btn primary" data-save>${isNew?'Создать':'Сохранить'}</button>`,
    onMount(layer, close){
      // Значок обновляем на лету: видно, что ссылка рабочая, до сохранения.
      const redraw = () => {
        layer.querySelector('#ipLogoPrev').innerHTML = providerLogo(
          layer.querySelector('#ipName').value.trim(),
          layer.querySelector('#ipLogo').value.trim(), 22);
      };
      layer.querySelector('#ipName').addEventListener('input', redraw);
      layer.querySelector('#ipLogo').addEventListener('input', redraw);

      layer.querySelector('[data-save]').addEventListener('click', async ()=>{
        const name = layer.querySelector('#ipName').value.trim();
        if (!name) { toast('Нужно название', 'err'); return; }
        const body = { name, url: layer.querySelector('#ipUrl').value.trim() || null,
                       note: layer.querySelector('#ipNote').value.trim() || null,
                       logo_url: layer.querySelector('#ipLogo').value.trim() || null };
        try {
          if (isNew) await API.call('/api/infra/providers', { method:'POST', body });
          else await API.call('/api/infra/providers/'+p.id, { method:'PATCH', body });
          close();
          toast(isNew ? 'Провайдер создан' : 'Сохранено');
          await onSaved();
        } catch(e){ toast('Не сохранилось: '+e.message, 'err'); }
      });
    }
  });
}

/* Расходы конкретной ноды: у кого арендована, почём и когда списывают. */
function nodeCostModal(n, provs, onSaved){
  openModal({
    title:'Расходы · '+esc(n.name), icon:'dollar', size:'sm',
    body:`
      <div class="field"><label>Провайдер</label>
        <select class="inp" id="ncProv">
          <option value="">— не указан —</option>
          ${provs.map(p=>`<option value="${p.id}" ${String(p.id)===String(n.infraProviderId)?'selected':''}>${esc(p.name)}</option>`).join('')}
        </select></div>
      <div class="field"><label>Стоимость в месяц</label>
        <div class="inp-group"><input class="inp num" id="ncCost" type="number" step="0.01"
          value="${n.costMinor ? minorToUnits(n.costMinor, DB.currency) : ''}" placeholder="0">
          <span class="suffix">${esc(DB.currency)}</span></div></div>
      <div class="field"><label>День списания</label>
        <input class="inp num" id="ncDay" type="number" min="1" max="31" value="${n.billDay || ''}" placeholder="5">
        <div class="hint">Число месяца, когда хостер снимает деньги — панель предупредит за несколько дней.</div></div>`,
    footer:`<div class="spacer"></div><button class="btn" data-close>Отмена</button>
            <button class="btn primary" data-save>Сохранить</button>`,
    onMount(layer, close){
      layer.querySelector('[data-save]').addEventListener('click', async ()=>{
        const cost = layer.querySelector('#ncCost').value.trim();
        const day = layer.querySelector('#ncDay').value.trim();
        if (day && !(day >= 1 && day <= 31)) { toast('День списания — от 1 до 31', 'err'); return; }
        try {
          await API.call('/api/nodes/'+n.id, { method:'PATCH', body:{
            infra_provider_id: layer.querySelector('#ncProv').value ? +layer.querySelector('#ncProv').value : null,
            monthly_cost_minor: cost === '' ? 0 : unitsToMinor(parseFloat(cost), DB.currency),
            bill_day: day === '' ? null : parseInt(day, 10),
          }});
          close();
          toast('Расходы ноды сохранены');
          await refreshDB();
          await onSaved();
        } catch(e){ toast('Не сохранилось: '+e.message, 'err'); }
      });
    }
  });
}

/* Факт оплаты хостеру. Плановая стоимость и фактические оплаты — разные
   вещи: план показывает, во что обходится парк сейчас, факт — что уже
   ушло со счёта. */
function infraPaymentModal(provs, onSaved, node){
  openModal({
    title:'Записать оплату', icon:'dollar', size:'sm',
    body:`
      <div class="field"><label>Провайдер</label>
        <select class="inp" id="ipvProv"><option value="">— не указан —</option>
          ${provs.map(p=>`<option value="${p.id}">${esc(p.name)}</option>`).join('')}</select></div>
      <div class="field"><label>Нода</label>
        <select class="inp" id="ipvNode"><option value="">— вся инфраструктура —</option>
          ${DB.nodes.map(n=>`<option value="${n.id}" ${node&&node.id===n.id?'selected':''}>${esc(n.name)}</option>`).join('')}</select>
        <div class="hint">Одно из двух полей обязательно — иначе расход не к чему отнести.</div></div>
      <div class="field"><label>Сумма <span class="req">*</span></label>
        <div class="inp-group"><input class="inp num" id="ipvSum" type="number" step="0.01" placeholder="0">
          <span class="suffix">${esc(DB.currency)}</span></div></div>
      <div class="field"><label>Дата оплаты</label>
        <input class="inp" type="date" id="ipvDate" value="${new Date().toISOString().slice(0,10)}"></div>`,
    footer:`<div class="spacer"></div><button class="btn" data-close>Отмена</button>
            <button class="btn primary" data-save>Записать</button>`,
    onMount(layer, close){
      layer.querySelector('[data-save]').addEventListener('click', async ()=>{
        const sum = parseFloat(layer.querySelector('#ipvSum').value);
        if (!(sum > 0)) { toast('Сумма должна быть больше нуля', 'err'); return; }
        try {
          await API.call('/api/infra/payments', { method:'POST', body:{
            provider_id: layer.querySelector('#ipvProv').value ? +layer.querySelector('#ipvProv').value : null,
            node_id: layer.querySelector('#ipvNode').value ? +layer.querySelector('#ipvNode').value : null,
            amount_minor: unitsToMinor(sum, DB.currency),
            paid_on: layer.querySelector('#ipvDate').value || null,
          }});
          close();
          toast('Оплата записана');
          await onSaved();
        } catch(e){ toast('Не записалась: '+e.message, 'err'); }
      });
    }
  });
}


/* ── НАСТРОЙКИ ── */
registerPage({
  id:'settings', title:'Настройки', group:'Система', icon:'settings',
  render(){
    return `
    <div class="page-head">
      <div><h1>Настройки</h1><div class="desc">Безопасность, интеграции, API-доступ и брендинг платформы.</div></div>
    </div>
    <div class="grid g2" style="align-items:start">
      <div>
        <div class="card" style="margin-bottom:20px"><h2>Клиентский кабинет и Mini App</h2><p class="sub-note">Сайт, брендинг, вход по коду и установка клиентского сервера.</p><a class="btn primary" href="#/cabinet">Настроить кабинет</a></div>
        <div class="card" id="subSvcCard">
          <div class="section-h" style="margin:0 0 12px"><h2>${I('globe',14)} Сервис подписки</h2></div>
          <p class="sub-note" style="margin-bottom:14px">Страница подписки работает рядом с панелью или на отдельном сервере. На отдельном сервере она получает данные через API панели: для выдачи конфигураций связь с панелью должна работать.</p>
          <div class="field"><label>Публичный адрес сабки</label>
            <input class="inp mono" id="subUrl" placeholder="https://sub.вашдомен">
            <div class="hint">Из него собираются ссылки и QR-коды для клиентов. Должен быть виден снаружи — не localhost.</div></div>
          <div class="field"><label>Публичный адрес панели</label>
            <div class="inp-group"><input class="inp mono" id="panelUrl" placeholder="https://panel.вашдомен">
              <button class="btn sm" id="subUrlSave">Сохранить</button></div>
            <div class="hint">По нему к панели обращаются сабка и агенты нод. Подставляется в установочные команды.</div></div>
          <div class="info-row"><span class="k">API-ключ сабки</span>
            <span class="v mono" id="subTok">—</span>
            <button class="btn sm" style="margin-left:auto" id="subTokBtn">${I('key',12)} Выпустить</button></div>
          <div class="info-row"><span class="k">Последний запрос</span><span class="v" id="subTokUsed">—</span>
            <button class="btn sm" style="margin-left:auto" id="subInstBtn">${I('download',12)} Установка</button></div>
        </div>

        <div class="card" style="margin-top:var(--panel-gap,14px)">
          <div class="section-h" style="margin:0 0 12px"><h2>${I('shieldCheck',14)} Безопасность</h2></div>
          <div class="switch-row"><label class="switch"><input type="checkbox" id="sec2fa"><span class="tr"></span></label>
            <div class="sw-txt"><b>Двухфакторная аутентификация</b>
              <span id="sec2faNote">Код из приложения-аутентификатора при каждом входе</span></div></div>
          <div class="info-row"><span class="k">Passkeys</span>
            <span class="v" id="secPk">—</span>
            <button class="btn sm" style="margin-left:auto" id="pkBtn">${I('fingerprint',12)} Подробнее</button></div>
          <div class="info-row"><span class="k">Пароль</span><span class="v">вход по паролю</span>
            <button class="btn sm" style="margin-left:auto" id="secPwd">Сменить</button></div>
          <div class="info-row"><span class="k">Активные сессии</span><span class="v" id="secSess">—</span>
            <button class="btn sm" style="margin-left:auto" id="secKill">Завершить остальные</button></div>
        </div>
        <div class="card" style="margin-top:var(--panel-gap,14px)">
          <div class="section-h" style="margin:0 0 12px"><h2>${I('key',14)} API-токены</h2>
            <div class="spacer"></div><button class="btn primary sm" id="newToken">${I('plus',12)} Создать</button></div>
          <div id="tokList" class="sub-note">Загружаем…</div>
        </div>
      </div>
      <div>
        <div class="card">
          <div class="section-h" style="margin:0 0 12px"><h2>${I('send',14)} Интеграции</h2></div>
          <div id="integrList" class="sub-note">Загружаем…</div>
        </div>
        <div class="card" style="margin-top:var(--panel-gap,14px)">
          <div class="section-h" style="margin:0 0 12px"><h2>${I('globe',14)} Домены и брендинг</h2></div>
          <div class="field"><label>Название бренда</label>
            <input class="inp" data-brand="brand.name" value="${esc(DB.brandName || '')}">
            <div class="hint">Попадает в заголовок подписки и в страницу для клиента.</div></div>
          <div class="field"><label>Акцентный цвет</label>
            <input class="inp mono" data-brand="brand.accent" value="${esc(DB.brandAccent || '')}" placeholder="var(--accent)"></div>
          <div class="hint">${I('info',12)} Домены панели и подписки задаются в разделе «Сервис подписки» выше — там же, где ключ и установка.</div>
          <button class="btn primary sm" style="margin-top:12px" id="brandSave">${I('check',12)} Сохранить</button>
        </div>
      </div>
    </div>`;
  },
  async bind(root){
    bindSubService(root);

    // ── API-токены ──
    const loadTokens = async () => {
      let list = [];
      try { list = await API.call('/api/tokens'); }
      catch(e){ root.querySelector('#tokList').textContent = 'Не загрузились: '+e.message; return; }

      root.querySelector('#tokList').innerHTML = list.map(t=>`
        <div class="info-row">
          <span class="v" style="flex-direction:column;align-items:flex-start;flex:1">
            <b style="font-size:12.5px">${esc(t.name)}</b>
            <span class="mono" style="font-size:11px;color:var(--text-3)">${esc(t.prefix)}… · ${(t.scopes || []).includes('write') ? 'чтение и изменение' : 'только чтение'} · создан ${fmtDate(t.created_at)}</span></span>
          <span style="font-size:11px;color:var(--text-3)">${t.last_used_at ? 'исп. '+fmtAgo(t.last_used_at)+' назад' : 'не использовался'}</span>
          <button class="btn ghost icon-only" data-trev="${t.id}" title="Отозвать">${I('trash',13)}</button>
        </div>`).join('') || '<div class="sub-note">Токенов нет. Они нужны для сторонних интеграций с API панели.</div>';

      root.querySelectorAll('[data-trev]').forEach(b=>b.addEventListener('click', ()=>confirmModal({
        title:'Отозвать токен?', text:'Интеграция немедленно потеряет доступ к API.', okText:'Отозвать',
        onOk:async ()=>{
          try {
            await API.call('/api/tokens/'+b.dataset.trev, { method:'DELETE' });
            toast('Токен отозван');
            await loadTokens();
          } catch(e){ toast('Не получилось: '+e.message, 'err'); }
        }
      })));
    };

    root.querySelector('#newToken').addEventListener('click', ()=>openModal({
      title:'Новый API-токен', icon:'key', size:'sm',
      body:`
        <div class="field"><label>Название <span class="req">*</span></label>
          <input class="inp" id="tkName" placeholder="my-integration">
          <div class="hint">По нему потом понятно, что отзывать.</div></div>
        <div class="field"><label for="tkScopes">Права доступа</label>
          <select class="inp" id="tkScopes"><option value="read">Только чтение</option><option value="write">Чтение и изменение</option></select>
          <div class="hint">Токен ограничен правами создавшего его администратора. Настройки входа и выпуск новых токенов ему недоступны.</div></div>`,
      footer:`<div class="spacer"></div><button class="btn" data-close>Отмена</button>
              <button class="btn primary" data-mk>Создать</button>`,
      onMount(layer, close){
        layer.querySelector('[data-mk]').addEventListener('click', async ()=>{
          const name = layer.querySelector('#tkName').value.trim();
          if (!name) { toast('Нужно название', 'err'); return; }
          try {
            const scopes = layer.querySelector('#tkScopes').value === 'write' ? ['read', 'write'] : ['read'];
            const r = await API.call('/api/tokens', { method:'POST', body:{ name, scopes } });
            close();
            tokenModal('Токен создан', r.token);
            await loadTokens();
          } catch(e){ toast('Не создался: '+e.message, 'err'); }
        });
      }
    }));

    const brand = root.querySelector('#brandSave');
    if (brand) brand.addEventListener('click', async ()=>{
      const body = {};
      root.querySelectorAll('[data-brand]').forEach(el=>{ body[el.dataset.brand] = el.value.trim(); });
      if (!Object.keys(body).length) { toast('Нечего сохранять', 'info'); return; }
      try {
        await API.call('/api/settings', { method:'PATCH', body });
        toast('Настройки сохранены');
        await refreshDB();
      } catch(e){ toast('Не сохранилось: '+e.message, 'err'); }
    });

    // ── Интеграции: настоящее состояние, а не значок «работает» ──
    const loadIntegrations = async () => {
      const box = root.querySelector('#integrList');
      if (!box) return;
      let provs = [];
      try { provs = await API.call('/api/pay/providers'); } catch(e) {box.innerHTML=`<p role="alert">Интеграции не загрузились: ${esc(e.message)}</p>`;return;}

      const row = (icon, name, note, ok, action) => `
        <div class="integration-row">
          <span class="integration-icon">${I(icon,16)}</span>
          <span class="v" style="flex:1;flex-direction:column;align-items:flex-start">
            <b style="font-size:12.5px">${esc(name)}</b>
            <span style="font-size:11px;color:var(--text-3)">${esc(note)}</span></span>
          ${ok ? '<span class="bdg ok"><span class="dot"></span>включено</span>'
               : '<span class="bdg neutral">не настроено</span>'}
          ${action || ''}
        </div>`;

      box.innerHTML = provs.map(p=>row(
        p.id === 'stars' ? 'star' : p.id === 'cryptobot' ? 'dollar' : 'card',
        p.title,
        (p.enabled_currencies || p.currencies).join(', ') || 'валюты не заданы',
        p.is_enabled && p.is_configured,
        `<button class="btn sm" onclick="navigate('providers')">Настроить</button>`
      )).join('') + `
        <div class="hint" style="margin-top:10px">${I('info',12)}
          Настройки Telegram находятся в разделе «Телеграм-бот». Статус платёжного модуля здесь показывает наличие настроек и включение; успешную оплату проверяют по платежам.</div>`;
    };

    await Promise.all([bindAdminSecurity(root), loadTokens(), loadIntegrations()]);
  }
});

/* ── Сервис подписки: адрес, ключ и установка на отдельный сервер ── */

async function bindSubService(root){
  const card = root.querySelector('#subSvcCard');
  if (!card) return;

  const urlInp   = card.querySelector('#subUrl');
  const panelInp = card.querySelector('#panelUrl');
  const tokEl  = card.querySelector('#subTok');
  const usedEl = card.querySelector('#subTokUsed');
  const tokBtn = card.querySelector('#subTokBtn');

  let state;
  const load = async () => {
    state = await API.call('/api/sub-service');
    urlInp.value = state.sub_public_url || '';
    panelInp.value = state.panel_public_url || '';
    if (state.token) {
      tokEl.textContent = state.token.prefix + '…';
      tokBtn.innerHTML = I('refresh',12) + ' Сменить';
      // Пустое «последний запрос» — сабка не дошла до панели ни разу.
      // Это первое, что нужно увидеть, когда «подписка не работает».
      usedEl.textContent = state.token.last_used_at
        ? fmtDT(state.token.last_used_at)
        : 'ни разу — сабка ещё не обращалась';
    } else if (state.env_token_configured) {
      tokEl.textContent = 'задан через SUB_SERVICE_TOKEN';
      usedEl.textContent = 'переменная окружения';
    } else {
      tokEl.textContent = 'не выпущен';
      usedEl.textContent = 'сабка работает из общей базы';
    }
  };

  try { await load(); }
  catch(e){ toast('Не удалось прочитать настройки сабки: '+e.message, 'err'); return; }

  // Оба адреса попадают в ссылки клиентам и в установочные команды.
  // localhost здесь — нерабочие ссылки и нода, которая «не подключается»,
  // поэтому проверяем до сохранения, а не после жалоб.
  const checkUrl = (url, what) => {
    if (!url) return true;
    if (!/^https?:\/\//.test(url)) { toast(what+': адрес должен начинаться с http:// или https://', 'err'); return false; }
    if (/localhost|127\.0\.0\.1/.test(url)) { toast(what+': localhost виден только этому серверу — нужен внешний адрес', 'err'); return false; }
    return true;
  };

  card.querySelector('#subUrlSave').addEventListener('click', async ()=>{
    const sub = urlInp.value.trim(), panel = panelInp.value.trim();
    if (!checkUrl(sub, 'Адрес сабки') || !checkUrl(panel, 'Адрес панели')) return;
    try {
      await API.call('/api/sub-service', { method:'PATCH', body:{ sub_public_url: sub, panel_public_url: panel } });
      await load();
      toast('Адреса сохранены');
    } catch(e){ toast('Не сохранилось: '+e.message, 'err'); }
  });

  tokBtn.addEventListener('click', ()=>{
    const issue = async () => {
      try {
        const res = await API.call('/api/sub-service/token', { method:'POST' });
        await load();
        subInstallModal(res.install, res.token);
      } catch(e){ toast('Не выпустился: '+e.message, 'err'); }
    };
    // Смена ключа мгновенно ломает работающую сабку — спрашиваем.
    if (state.token) {
      confirmModal({
        title:'Сменить ключ сабки?',
        text:'Прежний ключ перестанет действовать сразу. Сервис подписок с ним потеряет доступ к панели, пока вы не пропишете новый.',
        okText:'Сменить', onOk:issue,
      });
    } else issue();
  });

  card.querySelector('#subInstBtn').addEventListener('click', ()=>{
    subInstallModal(state.install, null);
  });
}

/* Установочные материалы сабки. Ключ виден только сразу после выпуска —
   в базе лежит лишь его хэш, поэтому в остальных случаях в конфигах
   стоит плейсхолдер. */
function subInstallModal(inst, token){
  // Панель могла не отдать установочные материалы (старая версия API,
  // недоступный сервис). Раньше это падало на чтении полей объекта, и
  // окно не открывалось вовсе — без единого слова о причине.
  if (!inst || typeof inst !== 'object') {
    toast('Панель не вернула установочные материалы — обновите сервер', 'err');
    return;
  }
  // Где стоит сабка — определяет, по какому адресу она стучится в панель.
  // На одном сервере это имя контейнера, на отдельном — публичный домен;
  // перепутать их значит получить сабку, которая молча не отвечает.
  let where = token ? 'separate' : 'same_host';

  const pane = () => {
    const p = inst[where];
    return `
      ${where === 'same_host' ? `
      <div class="hint" style="margin-bottom:12px">${I('info',12)}
        Рядом с панелью сабка читает базу напрямую: это быстрее похода в API,
        и служебный ключ ей вообще не нужен.</div>` : ''}

      <p class="sub-note">${where==='same_host'?'Сервис уже входит в установку панели. Проверьте его состояние и DNS домена подписки; повторно запускать install-sub.sh не нужно.':'На сервере подписки нужны Debian/Ubuntu, root, systemd и доступ к панели по HTTPS. PostgreSQL устанавливать не нужно.'}</p>
      <div class="tabs" id="subInstTabs">
        ${where === 'separate' ? '<button class="on" data-t="one_liner">Установка</button>' : ''}
        <button class="${where==='same_host'?'on':''}" data-t="systemd">${where==='same_host'?'Проверка сервиса':'Вручную'}</button>
        <button data-t="caddy">Домен и HTTPS</button><button data-t="compose">Docker</button>
      </div>

      <div class="tab-pane" data-p="compose">
        ${p.compose?`<div class="code wrap" style="max-height:32vh"><pre>${esc(p.compose)}</pre></div><button class="btn primary sm" style="margin-top:10px" data-copy="${esc(p.compose)}">${I('copy',12)} Копировать</button>`:'<p class="notice">Docker-образ для отдельного сервера пока не настроен. Используйте вкладку «Установка». Для Docker владелец панели должен задать доступный SUB_IMAGE с бинарником sn-sub.</p>'}
      </div>

      ${where === 'separate' ? `
      <div class="tab-pane on" data-p="one_liner">
        <p class="sub-note" style="margin-bottom:10px">Поставит бинарь и systemd-сервис, заодно проверит, что панель принимает ключ:</p>
        <div class="code wrap" style="max-height:32vh"><pre>${esc(p.one_liner)}</pre></div>
        <button class="btn primary sm" style="margin-top:10px" data-copy="${esc(p.one_liner)}"
                data-copy-msg="Команда скопирована">${I('copy',12)} Копировать команду</button>
      </div>` : ''}

      <div class="tab-pane ${where==='same_host'?'on':''}" data-p="systemd">
        <p class="sub-note" style="margin-bottom:10px">${where==='same_host'?'Выполните на сервере панели. Ответ ready подтверждает доступ сервиса к базе.':'Сначала установите бинарник sn-sub в /usr/local/bin/sn-sub. Сохраните параметры ниже в /etc/sn-sub/env с правами 600, юнит — в /etc/systemd/system/sn-sub.service. Затем systemctl daemon-reload и systemctl enable --now sn-sub.'}</p>
        ${p.env?`<div class="code wrap" style="max-height:24vh"><pre>${esc(p.env)}</pre></div><button class="btn sm" style="margin:10px 0" data-copy="${esc(p.env)}">${I('copy',12)} Копировать параметры</button>`:''}
        <div class="code" style="max-height:32vh"><pre>${esc(p.systemd)}</pre></div>
        <button class="btn primary sm" style="margin-top:10px" data-copy="${esc(p.systemd)}"
                data-copy-msg="${where==='same_host'?'Команды скопированы':'Юнит скопирован'}">${I('copy',12)} ${where==='same_host'?'Копировать команды':'Копировать юнит'}</button>
      </div>

      <div class="tab-pane" data-p="caddy">
        <p class="sub-note" style="margin-bottom:10px">Направьте DNS A/AAAA домена подписки на ${where==='same_host'?'сервер панели':'сервер подписки'}, откройте TCP 80 и 443. Добавьте этот блок в существующий Caddyfile и проверьте конфигурацию. Caddy получит сертификат для HTTPS. Порт 8081 снаружи открывать не нужно.</p>
        <div class="code" style="max-height:32vh"><pre>${esc(p.caddy)}</pre></div>
        <button class="btn primary sm" style="margin-top:10px" data-copy="${esc(p.caddy)}"
                data-copy-msg="Блок Caddy скопирован">${I('copy',12)} Копировать блок Caddy</button>
      </div>`;
  };

  openModal({
    title:'Установка сервиса подписки',
    sub: (inst.sub_public_url || '—') + ' → ' + (inst.panel_url || '—'),
    icon:'globe', size:'lg',
    body:`
      ${token ? `
      <div class="notice" style="background:color-mix(in srgb, var(--warn) 10%, transparent);border:1px solid color-mix(in srgb, var(--warn) 30%, transparent);color:var(--warn-ink);border-radius:10px;padding:11px 14px;font-size:12.5px;margin-bottom:14px">
        ${I('alert',13)} Ключ показывается один раз — в базе только его хэш. Скопируйте сейчас.
      </div>
      <div class="copy-block" style="margin-bottom:16px"><span class="txt mono">${esc(token)}</span>
        <button data-copy-raw="${esc(token)}">${I('copy',14)}</button></div>` : `
      <div class="hint" style="margin-bottom:14px">${I('info',12)} Для отдельного сервера подставьте сохранённый ключ вместо <code>ВАШ_ТОКЕН</code>. На одном сервере в режиме базы ключ не нужен.</div>`}

      <div class="tabs" id="whereTabs" style="margin-bottom:14px">
        <button class="${where==='separate'?'on':''}" data-w="separate">На отдельном сервере</button>
        <button class="${where==='same_host'?'on':''}" data-w="same_host">На этом же сервере</button>
      </div>
      <div id="subInstBody">${pane()}</div>
      <p class="hint" style="margin-top:18px">После настройки откройте реальную ссылку клиента в браузере и VPN-приложении. <a href="/subscription-installation.html" target="_blank" rel="noopener">Полная инструкция: DNS, HTTPS, обновление и диагностика</a></p>`,
    footer:`<div class="spacer"></div><button class="btn primary" data-close>Готово</button>`,
    onMount(layer){
      const wire = () => {
        const tabs = layer.querySelector('#subInstTabs');
        tabs.querySelectorAll('button').forEach(b=>b.addEventListener('click', ()=>{
          tabs.querySelectorAll('button').forEach(x=>x.classList.remove('on')); b.classList.add('on');
          layer.querySelectorAll('#subInstBody .tab-pane')
               .forEach(p=>p.classList.toggle('on', p.dataset.p===b.dataset.t));
        }));
        // Копированием занимается общий делегат в core.js: он читает
        // data-copy как сам текст. Свой обработчик здесь был лишним и,
        // срабатывая первым, проигрывал делегату — в буфер попадало
        // название вкладки вместо команды.
      };

      const wt = layer.querySelector('#whereTabs');
      wt.querySelectorAll('button').forEach(b=>b.addEventListener('click', ()=>{
        wt.querySelectorAll('button').forEach(x=>x.classList.remove('on')); b.classList.add('on');
        where = b.dataset.w;
        layer.querySelector('#subInstBody').innerHTML = pane();
        wire();
      }));

      layer.querySelectorAll('[data-copy-raw]').forEach(b=>b.addEventListener('click', ()=>
        copyText(b.dataset.copyRaw, 'Ключ скопирован')));
      wire();
    }
  });
}


/* Подключение приложения-аутентификатора.

   Секрет в базу пишется только после того, как человек подтвердил его
   кодом: включить двухфакторную, не убедившись, что приложение
   настроено, — верный способ запереть владельца снаружи. */
function totpSetupModal(onDone){
  openModal({
    title:'Двухфакторная аутентификация', icon:'lock', size:'sm',
    body:`<div id="tfBody" class="sub-note">Готовим секрет…</div>`,
    footer:`<div class="spacer"></div><button class="btn" data-close>Отмена</button>
            <button class="btn primary" data-save disabled>Включить</button>`,
    async onMount(layer, close){
      let data;
      try { data = await API.call('/api/admin/totp/setup', { method:'POST' }); }
      catch(e){ layer.querySelector('#tfBody').textContent = 'Не получилось: '+e.message; return; }

      layer.querySelector('#tfBody').innerHTML = `
        <p class="sub-note" style="margin-bottom:12px">Откройте приложение-аутентификатор и добавьте ключ вручную:</p>
        <div class="copy-block" style="margin-bottom:12px"><span class="txt mono">${esc(data.secret)}</span>
          <button data-copy>${I('copy',14)}</button></div>
        <div class="field"><label>Код из приложения <span class="req">*</span></label>
          <input class="inp mono" id="tfCode" inputmode="numeric" maxlength="6" placeholder="123456"></div>
        <div class="hint">${I('info',12)} Если код не подходит — проверьте время на телефоне: коды привязаны к часам.</div>`;

      layer.querySelector('[data-copy]').addEventListener('click', ()=>copyText(data.secret, 'Секрет скопирован'));
      const btn = layer.querySelector('[data-save]');
      btn.disabled = false;
      btn.addEventListener('click', async ()=>{
        const code = layer.querySelector('#tfCode').value.trim();
        if (code.length !== 6) { toast('Код из шести цифр', 'err'); return; }
        try {
          await API.call('/api/admin/totp', { method:'POST', body:{ secret: data.secret, code } });
          close();
          toast('Двухфакторная включена');
          await onDone();
        } catch(e){ toast(e.message, 'err'); }
      });
    }
  });
}

function totpDisableModal(onDone){
  openModal({
    title:'Выключить двухфакторную?', icon:'alert', size:'sm',
    body:`
      <p class="sub-note" style="margin-bottom:12px">Вход снова будет защищён только паролем.</p>
      <div class="field"><label>Код из приложения <span class="req">*</span></label>
        <input class="inp mono" id="tfOff" inputmode="numeric" maxlength="6" placeholder="123456">
        <div class="hint">Спрашиваем его затем, что перехваченная сессия иначе снимала бы защиту одним запросом.</div></div>`,
    footer:`<div class="spacer"></div><button class="btn" data-close>Отмена</button>
            <button class="btn danger" data-save>Выключить</button>`,
    onMount(layer, close){
      layer.querySelector('[data-save]').addEventListener('click', async ()=>{
        try {
          await API.call('/api/admin/totp', { method:'DELETE',
            body:{ code: layer.querySelector('#tfOff').value.trim() } });
          close();
          toast('Двухфакторная выключена', 'info');
          await onDone();
        } catch(e){ toast(e.message, 'err'); }
      });
    }
  });
}


/* Ключи входа без пароля.

   Смысл passkey в том, что закрытый ключ не покидает устройство и не
   может быть подобран или выужен: браузер подписывает вызов только для
   того домена, на котором ключ создан. */
function passkeysDrawer(onChange){
  openDrawer({
    title:'Passkeys', sub:'Вход по отпечатку, лицу или ключу устройства', icon:'fingerprint',
    body:`<div id="pkBody" class="sub-note">Загружаем…</div>`,
    footer:`<div class="spacer"></div><button class="btn" data-close>Закрыть</button>`,
    async onMount(layer){
      const draw = async () => {
        let d;
        try { d = await API.call('/api/admin/passkeys'); }
        catch(e){ layer.querySelector('#pkBody').textContent = 'Не загрузилось: '+e.message; return; }

        layer.querySelector('#pkBody').innerHTML = `
          ${!passkeysSupported() ? `
            <div class="notice" style="background:color-mix(in srgb, var(--warn) 10%, transparent);border:1px solid color-mix(in srgb, var(--warn) 30%, transparent);color:var(--warn-ink);border-radius:10px;padding:11px 14px;font-size:12.5px;margin-bottom:14px">
              ${I('alert',13)} Этот браузер не умеет passkeys. Список ниже покажем, но добавить ключ отсюда не выйдет.
            </div>` : ''}
          ${/* Ответ может прийти массивом или объектом со списком, а на
                свежей установке — вовсе пустым. Раньше здесь падало на
                d.items.map, и вместо списка ключей оставалась пустая
                панель без объяснения. */''}
          ${((Array.isArray(d) ? d : (d && d.items) || []).map(k=>`
            <div class="info-row">
              <span class="v" style="width:36px">${I('fingerprint',16)}</span>
              <span class="v" style="flex:1;flex-direction:column;align-items:flex-start">
                <b style="font-size:12.5px">${esc(k.name)}</b>
                <span style="font-size:11px;color:var(--text-3)">добавлен ${fmtDate(k.created_at)}</span></span>
              <button class="btn ghost icon-only" data-pkdel="${k.id}">${I('trash',13)}</button>
            </div>`).join('')) || '<div class="sub-note">Ключей пока нет.</div>'}
          <button class="btn" style="width:100%;justify-content:center;margin-top:14px" id="pkAdd"
            ${passkeysSupported() ? '' : 'disabled'}>${I('plus',13)} Добавить ключ</button>
          <div class="hint" style="margin-top:12px">${I('info',12)}
            Ключ привязан к домену панели: на другом адресе он не сработает — именно поэтому его нельзя выудить фишингом.
            Пароль при этом остаётся: passkey его не отменяет.</div>`;

        layer.querySelectorAll('[data-pkdel]').forEach(b=>b.addEventListener('click', ()=>confirmModal({
          title:'Удалить ключ?', text:'Войти этим устройством без пароля станет нельзя.', okText:'Удалить',
          onOk:async ()=>{
            try {
              await API.call('/api/admin/passkeys/'+b.dataset.pkdel, { method:'DELETE' });
              toast('Ключ удалён');
              await draw(); await onChange();
            } catch(e){ toast('Не удалился: '+e.message, 'err'); }
          }
        })));

        const add = layer.querySelector('#pkAdd');
        if (add) add.addEventListener('click', async ()=>{
          add.disabled = true;
          try {
            const start = await API.call('/api/admin/passkeys/register', { method:'POST' });
            const cred = await navigator.credentials.create(decodeWebauthn(start.options));
            const label = prompt('Название ключа', 'Этот компьютер') || 'Ключ';
            await API.call('/api/admin/passkeys/register/finish', { method:'POST',
              body:{ label, credential: encodeCredential(cred) } });
            toast('Ключ добавлен');
            await draw(); await onChange();
          } catch(e){
            // Отказ пользователя — не ошибка: он просто передумал.
            const msg = String(e.message || e);
            toast(/NotAllowed|abort/i.test(msg) ? 'Добавление отменено' : 'Не добавился: '+msg,
                  /NotAllowed|abort/i.test(msg) ? 'info' : 'err');
          } finally { add.disabled = false; }
        });
      };
      await draw();
    }
  });
}

async function bindAdminSecurity(root) {
    // ── Безопасность ──
    const loadSec = async () => {
      try {
        const [sess, pk] = await Promise.all([
          API.call('/api/admin/sessions'), API.call('/api/admin/passkeys')]);
        root.querySelector('#secSess').textContent =
          String(sess.length);
        const pkList = Array.isArray(pk) ? pk : (pk && pk.items) || [];
        root.querySelector('#secPk').textContent = !passkeysSupported()
          ? 'браузер не умеет'
          : pkList.length ? pkList.length + ' ключей' : 'ключей нет';
        const t = await API.call('/api/admin/totp');
        const cb = root.querySelector('#sec2fa');
        cb.checked = t.enabled;
        root.querySelector('#sec2faNote').textContent = t.enabled
          ? 'Включена — код запрашивается при каждом входе'
          : 'Код из приложения-аутентификатора при каждом входе';
      } catch(e){ root.querySelector('#sec2faNote').textContent = e.message; }
    };

    root.querySelector('#sec2fa').addEventListener('change', async e=>{
      const on = e.target.checked;
      // Возвращаем тумблер обратно: он отражает состояние, а меняется
      // оно только после подтверждения кодом.
      e.target.checked = !on;
      if (on) totpSetupModal(loadSec); else totpDisableModal(loadSec);
    });

    root.querySelector('#secPwd').addEventListener('click', ()=>openModal({
      title:'Смена пароля', icon:'lock', size:'sm',
      body:`
        <div class="field"><label>Текущий пароль <span class="req">*</span></label>
          <input class="inp" type="password" id="pwCur" autocomplete="current-password">
          <div class="hint">Спрашиваем его затем, что перехваченная сессия иначе позволила бы сменить пароль и запереть вас снаружи.</div></div>
        <div class="field"><label>Новый пароль <span class="req">*</span></label>
          <input class="inp" type="password" id="pwNew" autocomplete="new-password"></div>
        <div class="field"><label>Ещё раз</label>
          <input class="inp" type="password" id="pwNew2" autocomplete="new-password"></div>
        <div class="hint">${I('info',12)} Все остальные сессии будут закрыты: если пароль меняют из-за утечки, оставлять чужую сессию живой нельзя.</div>`,
      footer:`<div class="spacer"></div><button class="btn" data-close>Отмена</button>
              <button class="btn primary" data-save>Сменить</button>`,
      onMount(layer, close){
        layer.querySelector('[data-save]').addEventListener('click', async ()=>{
          const cur = layer.querySelector('#pwCur').value;
          const n1 = layer.querySelector('#pwNew').value;
          const n2 = layer.querySelector('#pwNew2').value;
          if (n1.length < 10) { toast('Новый пароль короче 10 символов', 'err'); return; }
          if (n1 !== n2) { toast('Пароли не совпадают', 'err'); return; }
          try {
            await API.call('/api/admin/password', { method:'POST', body:{ current: cur, new_password: n1 } });
            close();
            toast('Пароль изменён, остальные сессии закрыты');
            await loadSec();
          } catch(e){ toast(e.message, 'err'); }
        });
      }
    }));

    root.querySelector('#secKill').addEventListener('click', ()=>confirmModal({
      title:'Завершить остальные сессии?',
      text:'Все входы, кроме текущего, будут закрыты. Ваша сессия останется — иначе вы выкинули бы сами себя.',
      okText:'Завершить',
      onOk:async ()=>{
        try {
          const r = await API.call('/api/admin/sessions', { method:'DELETE' });
          toast(r.closed ? `Закрыто сессий: ${r.closed}` : 'Других сессий не было');
          await loadSec();
        } catch(e){ toast('Не получилось: '+e.message, 'err'); }
      }
    }));

    root.querySelector('#pkBtn').addEventListener('click', ()=>passkeysDrawer(loadSec));


    await loadSec();
}
