/* ═══════════ PAGES: Продажи — платежи, тарифы, промокоды, партнёры, рассылки ═══════════ */
'use strict';

/* ── ПЛАТЕЖИ ── */
/* Все значения payment_status из схемы. «canceled» здесь не было, и
   один отменённый платёж ронял разбор всей страницы: строка
   `const [t,c] = PAY_STATUS[p.status]` разворачивала undefined.
   Запасной вариант оставлен на случай, если в перечисление добавят
   что-то ещё, а сюда забудут заглянуть. */
const PAY_STATUS = {
  success:  ['Успех',   'ok'],
  failed:   ['Ошибка',  'err'],
  refunded: ['Возврат', 'neutral'],
  canceled: ['Отменён', 'neutral'],
  pending:  ['Ожидает', 'warn'],
};
const payStatus = (s) => PAY_STATUS[s] || [s || '—', 'neutral'];
registerPage({
  id:'payments', title:'Платежи', group:'Продажи', icon:'card', badge:()=>DB.payments.length,
  render(){
    return `
    <div class="page-head">
      <div><h1>Платежи</h1><div class="desc">Платежи клиентов, статусы счетов и возвраты. Каждая сумма показана в валюте платежа.</div></div>
      <div class="actions">
        <button class="btn" id="expBtn">${I('download',14)} Экспорт CSV</button>
      </div>
    </div>
    <div class="grid g4" id="payMetrics" style="margin-bottom:16px"><div class="sub-note">Загружаем показатели…</div></div>
    <div class="toolbar">
      <div class="search-inp">${I('search',14)}<input class="inp" placeholder="Поиск: клиент, TX ID, ID платежа…" id="paySearch"></div>
      <select class="inp" style="width:170px" id="payStatusF">
        <option value="">Все статусы</option><option value="success">Успешные</option><option value="failed">Ошибки</option><option value="refunded">Возвраты</option><option value="canceled">Отменённые</option><option value="pending">Ожидают оплаты</option>
      </select>
      <select class="inp" style="width:180px" id="payMethodF"><option value="">Все методы</option>${DB.payProviders.map(p=>`<option value="${esc(p.id)}">${esc(p.title)}</option>`).join('')}</select>
      <div class="spacer" style="flex:1"></div>
      <button class="btn icon-only" title="Обновить" onclick="refreshDB()">${I('refresh',14)}</button>
    </div>
    <div class="tbl-wrap"><table class="tbl">
      <thead><tr><th>Клиент</th><th>Покупка</th><th>Сумма</th><th>Метод</th><th>Статус</th><th>Время</th><th style="width:36px"></th></tr></thead>
      <tbody id="payRows"></tbody>
    </table>
    <div class="tbl-foot"><span id="payCount"></span><div class="pages" id="payPages"></div></div>
    </div>`;
  },
  bind(root){
    let offset=0, requestId=0, timer;
    const pageSize=50;
    const draw = async ()=>{
      const request=++requestId;
      const params=new URLSearchParams({paginated:'true',limit:String(pageSize),offset:String(offset)});
      for(const [id,key] of [['paySearch','search'],['payStatusF','status'],['payMethodF','provider']]){
        const value=root.querySelector('#'+id).value.trim(); if(value)params.set(key,value);
      }
      let data;
      try { data=await API.call('/api/payments?'+params); }
      catch(err){if(request===requestId&&root.isConnected){root.querySelector('#payRows').innerHTML='<tr><td colspan="7">'+esc(err.message)+'</td></tr>';root.querySelector('#payCount').textContent='Не удалось загрузить платежи';root.querySelector('#payPages').innerHTML='';}return;}
      if(request!==requestId||!root.isConnected)return;
      const rows=data.items.map(mapPayment);
      rows.forEach(p=>{const i=DB.payments.findIndex(x=>x.id===p.id);if(i<0)DB.payments.push(p);else DB.payments[i]=p;});
      root.querySelector('#payCount').textContent = data.total ? `${offset+1}–${offset+rows.length} из ${fmtN(data.total)} платежей` : 'Платежей не найдено';
      root.querySelector('#payPages').innerHTML=`<button class="btn sm" data-prev ${offset===0?'disabled':''}>Назад</button><button class="btn sm" data-next ${offset+pageSize>=data.total?'disabled':''}>Далее</button>`;
      root.querySelector('[data-prev]').onclick=()=>{offset=Math.max(0,offset-pageSize);draw();};
      root.querySelector('[data-next]').onclick=()=>{offset+=pageSize;draw();};
      const m=data.summary.find(x=>x.currency===DB.currency)||{};
      const failed=data.summary.reduce((sum,x)=>sum+x.failed_count,0);
      root.querySelector('#payMetrics').innerHTML=[
        ['За 24 часа',fmtMoney(minorToUnits(m.day_amount||0,DB.currency)),`${m.day_count||0} платежей · ${DB.currency}`],
        ['За 30 дней',fmtMoney(minorToUnits(m.month_amount||0,DB.currency)),`${m.month_count||0} платежей · ${DB.currency}`],
        ['Средний чек',fmtMoney(minorToUnits((m.month_amount||0)/(m.month_count||1),DB.currency)),`30 дней · ${DB.currency}`],
        ['Ошибок за 24 часа',fmtN(failed),'Все валюты'],
      ].map(([t,v,d])=>`<div class="card" style="padding:13px 16px"><div class="label">${t}</div><div class="metric num" style="font-size:20px">${v}</div><div class="sub-note">${esc(d)}</div></div>`).join('');
      root.querySelector('#payRows').innerHTML = rows.map(p=>{
        const [t,c] = payStatus(p.status);
        return `<tr data-pay="${p.id}" style="cursor:pointer">
          <td><div class="cell-main"><div class="avatar-sm">${esc(p.user[0].toUpperCase())}</div><div><b>@${esc(p.user)}</b><span class="sub mono">${esc(p.txid)}</span></div></div></td>
          <td>${esc(p.item)}</td>
          <td class="num" style="font-weight:600;color:${p.status==='success'?'var(--ok)':'var(--text)'}">${fmtMoney(p.amount, p.currency)}</td>
          <td><span class="chip">${esc(p.method)}</span></td>
          <td><span class="bdg ${c}"><span class="dot"></span>${t}</span>${p.err?`<span class="sub">${esc(p.err)}</span>`:''}</td>
          <td class="num" style="color:var(--text-2);font-size:12px">${fmtDT(p.at)}</td>
          <td><button class="btn ghost icon-only" data-menu>${I('more',14)}</button></td>
        </tr>`;
      }).join('') || `<tr><td colspan="7"><div class="empty">${I('search',32)}<b>Ничего не найдено</b><span>Попробуйте изменить фильтры</span></div></td></tr>`;
      root.querySelectorAll('[data-pay]').forEach(tr=>{
        const p = DB.payments.find(x=>x.id===tr.dataset.pay);
        tr.querySelector('[data-menu]').addEventListener('click', e=>{
          e.stopPropagation();
          menu(e.currentTarget, [
            {label:'Детали платежа', icon:'eye', onClick:()=>payDetails(p)},
            {label:'Открыть клиента', icon:'user', onClick:()=>openUserView({id:p.clientId})},
            {label:'Скопировать TX ID', icon:'copy', onClick:()=>copyText(p.txid)},
            '-',
            {label:'Оформить возврат', icon:'rotate', danger:true, onClick:()=>refundModal(p)},
          ]);
        });
        tr.addEventListener('click', ()=>payDetails(p));
      });
    };
    root.querySelector('#paySearch').addEventListener('input', ()=>{clearTimeout(timer);offset=0;timer=setTimeout(draw,180);});
    for(const id of ['payStatusF','payMethodF'])root.querySelector('#'+id).addEventListener('change',()=>{offset=0;draw();});
    root.querySelector('#expBtn').addEventListener('click', async e=>{
      const btn=e.currentTarget;btn.disabled=true;
      try {
        const all=[];
        for(let offset=0;;offset+=500){const page=await API.call('/api/payments?limit=500&offset='+offset);all.push(...page);if(page.length<500)break;}
        const cell=x=>'"'+String(x??'').replace(/^[=+@-]/,"'$&").replaceAll('"','""')+'"';
        const csv=[['ID','Клиент','Сумма (минорные единицы)','Валюта','Провайдер','Статус','Дата'],...all.map(p=>[p.id,p.username,p.amount_minor,p.currency,p.provider,p.status,p.paid_at||p.created_at])].map(row=>row.map(cell).join(';')).join('\r\n');
        const url=URL.createObjectURL(new Blob(['\ufeff'+csv],{type:'text/csv;charset=utf-8'}));const a=document.createElement('a');a.href=url;a.download='payments-'+new Date().toISOString().slice(0,10)+'.csv';a.click();setTimeout(()=>URL.revokeObjectURL(url),1000);toast('Экспортировано '+all.length+' платежей');
      }catch(err){toast('Экспорт не получился: '+err.message,'err');}finally{btn.disabled=false;}
    });
    draw();
  }
});
function payDetails(p){
  const [t,c] = payStatus(p.status);
  openModal({
    title:'Платёж '+esc(p.txid), sub:fmtDT(p.at), icon:'card',
    body:`
      <div class="info-rows">
        <div class="info-row"><span class="k">Клиент</span><span class="v"><span class="link" data-payment-client>@${esc(p.user)}</span></span></div>
        <div class="info-row"><span class="k">Покупка</span><span class="v">${esc(p.item)}</span></div>
        <div class="info-row"><span class="k">Сумма</span><span class="v num" style="font-weight:700">${fmtMoney(p.amount, p.currency)}</span></div>
        <div class="info-row"><span class="k">Метод</span><span class="v"><span class="chip">${esc(p.method)}</span></span></div>
        <div class="info-row"><span class="k">Статус</span><span class="v"><span class="bdg ${c}"><span class="dot"></span>${t}</span>${p.err?` <span style="color:var(--text-3);font-size:11.5px">${esc(p.err)}</span>`:''}</span></div>
        <div class="info-row"><span class="k">TX ID</span><span class="v mono">${esc(p.txid)} <button class="btn ghost icon-only" style="width:24px;height:24px;padding:4px" data-copy-tx>${I('copy',12)}</button></span></div>
        <div class="info-row"><span class="k">Webhook</span><span class="v"><span class="sub-note">Статус доставки webhook не записывается. Проверяйте статус платежа и журнал провайдера.</span></span></div>
      </div>`,
    footer:`<button class="btn danger sm" data-refund ${p.status!=='success'?'disabled':''}>${I('rotate',12)} Возврат</button><div class="spacer"></div><button class="btn" data-close>Закрыть</button>`,
    onMount(layer, close){ layer.querySelector('[data-copy-tx]').onclick=()=>copyText(p.txid);layer.querySelector('[data-payment-client]').onclick=()=>{const u=p.clientId?{id:p.clientId}:userByName(p.user);if(u){close();openUserView(u);}else toast('Найдите клиента через раздел «Клиенты»','err');};layer.querySelector('[data-refund]').addEventListener('click', ()=>{ close(); refundModal(p); }); }
  });
}
function refundModal(p){
  if(p.status!=='success'){toast('Возврат можно записать только для успешной оплаты','err');return;}
  openModal({
    title:'Возврат платежа', sub:`${esc(p.txid)} · ${fmtMoney(p.amount, p.currency)}`, icon:'rotate', iconTone:'danger', size:'sm',
    body:`
      <div class="field"><label>Сумма возврата</label>
        <div class="inp-group"><input class="inp num" value="${Math.max(0,p.amount-(p.refunded||0)).toFixed(2)}"><span class="suffix">${esc(p.currency || DB.currency)}</span></div>
        <div class="hint">Уже возвращено: ${fmtMoney(p.refunded||0,p.currency)}. Доступный остаток: ${fmtMoney(Math.max(0,p.amount-(p.refunded||0)),p.currency)}. Доступ остаётся прежним, если не выбран отзыв.</div></div>
      <div class="field"><label>Причина</label>
        <select class="inp"><option>По запросу клиента</option><option>Ошибочный платёж</option><option>Чарджбэк</option><option>Другое</option></select></div>
      <label class="check"><input type="checkbox" checked><span>${p.kind==='addon'?'Убрать лимиты из этой докупки':'Завершить текущую подписку клиента'}</span></label>
      <div class="hint" style="margin-top:10px;color:var(--warn-ink);font-size:11.5px">${I('alert',12)} Деньги возвращаются в личном кабинете провайдера (${esc(p.method)}) — панель этого не делает за вас. Здесь фиксируется факт возврата и отзывается выданное.</div>`,
    footer:`<div class="spacer"></div><button class="btn" data-close>Отмена</button><button class="btn danger" data-ok>Записать возврат</button>`,
    onMount(layer, close){
      // Раньше кнопка рисовала «Возврат оформлен», не отправляя ничего:
      // платёж оставался успешным, выручка — завышенной.
      layer.querySelector('[data-ok]').addEventListener('click', async (e)=>{
        const sum = parseFloat(layer.querySelector('.inp.num').value.replace(',', '.'));
        if (!(sum > 0)) { toast('Сумма возврата должна быть больше нуля','err'); return; }
        const btn = e.currentTarget; btn.disabled = true;
        try {
          await API.call('/api/payments/'+p.id+'/refund', { method:'POST', body:{
            amount_minor: unitsToMinor(sum, p.currency),
            reason: layer.querySelector('select').value,
            revoke: layer.querySelector('.check input').checked,
          }});
          close();
          toast('Возврат записан');
          await refreshDB();
        } catch(err){ btn.disabled = false; toast('Не записался: '+err.message,'err'); }
      });
    }
  });
}

/* ── ТАРИФЫ ── */
registerPage({
  id:'tariffs', title:'Тарифы', group:'Продажи', icon:'layers',
  render(){
    return `
    <div class="page-head">
      <div><h1>Тарифы</h1><div class="desc">Витрина бота и сайта. Периоды и цены настраиваются в карточке тарифа. Тариф связывает цену, лимиты и сквады доступа.</div></div>
      <div class="actions"><button class="btn primary" id="newTariff">${I('plus',14)} Новый тариф</button></div>
    </div>
    <div class="grid g2" id="tariffCards">
      ${DB.tariffs.map(t=>`
        <div class="card" style="${t.active?'':'opacity:.6'}">
          <div class="tariff-layout">
            <span class="grip">${I('grip',14)}</span>
            <div class="avatar-sm" style="background:var(--accent-dim);border-color:color-mix(in srgb, var(--accent) 30%, transparent);color:var(--accent)">${I(t.emoji,15)}</div>
            <div style="flex:1;min-width:0">
              <b style="font-size:14.5px">${esc(t.title||t.name)}</b> <span class="chip mono">${esc(t.name)}</span> ${t.hidden?'<span class="bdg neutral">скрыт</span>':''} ${!t.active?'<span class="bdg neutral">выключен</span>':''}
              <div class="sub-note" style="margin-top:2px">${tariffDescription(t.desc)}</div>
              <div class="tariff-meta">
                <span class="chip">${I('smartphone',10)} ${t.devices} устр.</span>
                <span class="chip">${t.trafficGb?fmtB(t.trafficGb)+'/мес':'безлимит'}</span>
                ${t.squads.map(s=>`<span class="chip accent">${esc(s)}</span>`).join('')}
              </div>
              <button class="btn ghost sm" data-taddons="${t.id}">${I('plus',12)} Докупки: ${(t.addons||[]).length?[[(t.addons||[]).filter(a=>a.kind==='devices').length,'устройства'],[(t.addons||[]).filter(a=>a.kind==='traffic').length,'трафик']].filter(([n])=>n).map(([n,label])=>label+' — '+n).join(', '):'не настроены'}</button>
              <div class="tariff-prices">
                ${periodRows(t).filter(r=>r.d!=='').map(r=>`<span class="chip" style="font-size:11px;padding:4px 10px">${r.d} дн · <b style="color:var(--text)">${r.amount===''?(r.stars!==''?`${r.stars} ★`:'цена не задана'):r.amount>0?fmtMoney(r.amount):'бесплатно'}</b>${r.amount!==''&&r.stars!==''?` <span style="color:var(--text-3)">· ${r.stars} ★</span>`:''}</span>`).join('')}
              </div>
              ${t.active && !t.hidden && !ценаЕсть(t) ? `
                <div class="sub-note" style="color:var(--warn-ink);margin-top:8px">
                  ${I('alert',11)} Нет цены в ${esc(DB.currency)}${чужиеВалюты(t).length
                    ? ` — заданы только в ${чужиеВалюты(t).map(esc).join(', ')}` : ''}.
                  В боте и кабинете тариф не купить: смена валюты цены не пересчитывает,
                  их нужно задать заново.
                </div>` : ''}
            </div>
            <div class="tariff-actions">
              <label class="switch"><input type="checkbox" aria-label="Тариф ${esc(t.name)} активен" ${t.active?'checked':''} data-toggle="${t.id}"><span class="tr"></span></label>
              <span class="sub-note num">${t.buyers30d} покупок / 30д</span>
              <button class="btn ghost icon-only" aria-label="Действия тарифа ${esc(t.name)}" data-tmenu="${t.id}">${I('more',14)}</button>
            </div>
          </div>
        </div>`).join('')||`<div class="card empty" style="grid-column:1/-1">${I('layers',32)}<b>Создайте первый тариф</b><span>Укажите периоды, цены, лимиты и доступные локации. После этого тариф можно предложить клиентам.</span><button class="btn primary" data-first-tariff>Создать тариф</button></div>`}
    </div>`;
  },
  bind(root){
    root.querySelector('#newTariff').addEventListener('click', ()=>tariffModal());
    root.querySelector('[data-first-tariff]')?.addEventListener('click',()=>tariffModal());
    root.querySelectorAll('[data-toggle]').forEach(sw=>sw.addEventListener('change', e=>{
      const t = DB.tariffs.find(x=>x.id===e.target.dataset.toggle);
      const active = e.target.checked; e.target.disabled = true;
      API.call('/api/tariffs/'+t.id, {method:'PATCH', body:{is_active:active}})
        .then(()=>{t.active=active; toast(`Тариф ${t.name} ${active?'включён':'выключен'}`);})
        .catch(err=>{e.target.checked=t.active; toast('Не сохранено: '+err.message,'err');})
        .finally(()=>{e.target.disabled=false;});
    }));
    root.querySelectorAll('[data-taddons]').forEach(b=>b.onclick=()=>tariffModal(DB.tariffs.find(t=>t.id===b.dataset.taddons),'addons'));
    root.querySelectorAll('[data-tmenu]').forEach(b=>b.addEventListener('click', e=>{
      const t = DB.tariffs.find(x=>x.id===e.currentTarget.dataset.tmenu);
      menu(e.currentTarget, [
        {label:'Редактировать', icon:'edit', onClick:()=>tariffModal(t)},
        {label:'Дублировать', icon:'copy', onClick:async ()=>{
          try {
            await API.call('/api/tariffs', { method:'POST', body:{
              code: t.name+'_COPY', title: t.name+' (копия)', description: t.desc,
              device_limit: t.devices, reset_strategy:t.reset, is_trial:t.trial, badge:t.badge,
              traffic_limit_bytes: t.trafficGb == null ? null : Math.round(t.trafficGb * 1024 ** 3),
              // Копия создаётся скрытой: две одинаковые витрины в боте
              // сбивают покупателя с толку.
              is_visible: false, is_active: true,
              prices: (t.prices||[]).map(p=>({ period_days: p.d, currency: p.cur,
                                               amount_minor: unitsToMinor(p.p, p.cur) })),
              squad_names: t.squads,
              addons: (t.addons||[]).map(({id,...a})=>a),
            }});
            toast('Копия создана — она скрыта из витрины');
            await refreshDB();
          } catch(e){ toast('Не скопировался: '+e.message, 'err'); }
        }},
        {label:'Покупатели тарифа',icon:'users2',onClick:()=>{usersTariffFilter=t.name;navigate('users');}},
        '-',
        {label:'Удалить',icon:'trash',danger:true,onClick:()=>deleteTariff(t)},
      ]);
    }));
  }
});
/* Валюты, доступные в редакторе цен. Список тот же, что понимают
   платёжные модули; у каждой валюты может быть свой провайдер. */


/* Единицы Telegram Stars. Это не вторая валюта системы: звёзды существуют
   только внутри Telegram и рубли принять не могут, поэтому цена в них —
   отдельное поле рядом с ценой в валюте сервиса. */
const STARS = 'XTR';

function tariffModal(t, section){
  const isNew = !t;
  t = t || {name:'', emoji:'zap', devices:1, trafficGb:100,
            prices:[{d:30, p:4.99, cur:DB.currency}], desc:'', squads:[], hidden:false, active:true};
  openDrawer({
    title:isNew?'Новый тариф':'Тариф · '+t.name, sub:'Витрина, лимиты и доступ', icon:'layers', size:'lg', className:'sectioned-dialog '+'tariff-dialog',
    body:`
      <div class="two-col">
        <div class="field"><label>Код <span class="req">*</span></label><input class="inp" id="tCode" value="${esc(t.name)}" placeholder="PRO">
          <div class="hint">Служебный код тарифа. Менять у работающего тарифа не стоит.</div></div>
        <div class="field"><label>Название для клиента</label><input class="inp" id="tTitle" value="${esc(t.title||t.name)}" placeholder="Семейный"></div>
        <div class="field"><label>Тег в боте</label><input class="inp" id="tBadge" value="${esc(t.badge||'')}" placeholder="⭐ Популярный"></div>
      </div>
      <div class="field"><label>Описание (HTML разрешён)</label><textarea class="inp" id="tDesc">${esc(t.desc)}</textarea>
        <div class="hint">Показывается в карточке тарифа в боте и в кабинете. Поддерживаются &lt;b&gt;, &lt;i&gt;, эмодзи.
          Устройства и трафик писать не нужно — они подставляются из лимитов ниже.</div></div>
      <details class="form-section"><summary>Перевод для сайта и Mini App · EN</summary><div class="two-col"><div class="field"><label>Название · EN</label><input class="inp" id="tTitleEn" value="${esc(t.locales?.en?.title||'')}" maxlength="200"></div><div class="field"><label>Тег · EN</label><input class="inp" id="tBadgeEn" value="${esc(t.locales?.en?.badge||'')}" maxlength="200"></div></div><div class="field"><label>Описание · EN</label><textarea class="inp" id="tDescEn" maxlength="8000">${esc(t.locales?.en?.description||'')}</textarea></div><p class="hint">Цена, срок и лимиты едины для обоих языков. Название тарифа без перевода остаётся общим; описание на английском задаётся отдельно.</p></details>
      <div class="form-section"><h4>${I('sliders',13)} Лимиты</h4>
        <div class="two-col">
          <div class="field"><label>Устройства (HWID)</label>
            <div class="slider-row"><input type="range" min="1" max="${Math.max(100,t.devices||1)}" value="${t.devices}" id="devR"><span class="num" style="width:20px" id="devV">${t.devices}</span></div></div>
          <div class="field"><label>Трафик за период сброса</label>
            <div class="inp-group"><input class="inp num" id="tTraffic" value="${t.trafficGb??''}" placeholder="∞ (пусто = безлимит)"><span class="suffix">GiB</span></div></div>
        </div>
        <div class="field"><label>Стратегия сброса трафика</label>
          <div class="seg" id="tReset">
            <button data-v="no_reset" class="${t.reset==='no_reset'?'on':''}">Без сброса</button>
            <button data-v="day" class="${t.reset==='day'?'on':''}">Ежедневно</button>
            <button data-v="week" class="${t.reset==='week'?'on':''}">Еженедельно</button>
            <button data-v="month" class="${!t.reset||t.reset==='month'?'on':''}">Ежемесячно</button>
          </div></div>
      </div>
      <div class="form-section"><h4>${I('dollar',13)} Цены по периодам</h4>
        ${чужиеВалюты(t).length?`<div class="hint">Цены в ${чужиеВалюты(t).map(esc).join(", ")} больше не используются. При сохранении останутся цены в ${esc(DB.currency)} и Telegram Stars.</div>`:""}
        <div id="priceRows">${periodRows(t).map(row=>priceRowHtml(row)).join('')}</div>
        <button class="btn sm" id="addPrice">${I('plus',12)} Добавить период</button>
        <div class="hint" style="margin-top:8px">Цена — в валюте системы (<b>${esc(DB.currency)}</b>, меняется в «Платёжки и валюты»).
        Звёзды Telegram живут в собственных единицах и рубли принять не могут, поэтому цена в них задаётся отдельно — оставьте пустой, если оплата звёздами не нужна.</div>
      </div>
      <div class="form-section tariff-addons"><h4 tabindex="-1">${I('plus',13)} Докупки для этого тарифа</h4>
        <div class="addon-scope"><strong>Только для тарифа «<span data-addon-plan>${esc(t.title||t.name||'Новый тариф')}</span>»</strong><p>Пакеты увидят клиенты с действующей подпиской на этот тариф. Для остальных тарифов пакеты и цены настраиваются отдельно.</p></div>
        <p class="hint">На сайте и в Mini App кнопки появятся на главной, в карточках «Устройства» и «Трафик». В боте — «Докупки трафика и устройств» в меню и команда /addons. Покупки и кнопка меню должны быть включены в настройках.</p>
        <p class="hint">Клиент покупает пакеты отдельно от подписки. Устройства — до конца оплаченного срока, трафик — до следующего сброса или окончания подписки. Докупка не продлевает срок.</p>
        ${['traffic','devices'].map(kind=>`<section class="addon-admin-group"><h5>${kind==='traffic'?'Дополнительный трафик':'Дополнительные устройства'}</h5><p class="hint">${kind==='traffic'?'<span data-addon-traffic-hint>Показывается только клиентам с ограниченным трафиком.</span>':'Добавляет места для устройств к текущему лимиту.'}</p><div data-addon-rows="${kind}">${(t.addons||[]).filter(a=>a.kind===kind).map(a=>addonPriceRow(a)).join('')}</div><button class="btn sm" data-add-addon="${kind}">${I('plus',12)} Добавить пакет</button></section>`).join('')}
        <p class="hint" data-addon-status></p><p class="hint">Удаление пакета скрывает его из продажи; оплаченные докупки сохраняются.</p>
      </div>
      <div class="form-section"><h4>${I('squads',13)} Доступ · внутренние сквады</h4>
        <div class="chips-select">${DB.squadsInt.map(s=>`<span class="chip-opt ${t.squads.includes(s.name)?'on':''}" data-squad="${esc(s.name)}">${esc(s.name)}</span>`).join('')}</div>
        <div class="hint" style="margin-top:8px">Определяет, какие инбаунды и ноды получит подписка этого тарифа.</div>
      </div>
      <div class="form-section"><h4>${I('eye',13)} Видимость</h4>
        <div class="switch-row"><label class="switch"><input type="checkbox" id="tTrial" ${t.trial?'checked':''}><span class="tr"></span></label><div class="sw-txt"><b>Пробный тариф</b><span>Предлагается клиенту один раз. Для бесплатного доступа укажите нулевую цену.</span></div></div>
        <div class="switch-row"><label class="switch"><input type="checkbox" id="tVisible" ${!t.hidden?'checked':''}><span class="tr"></span></label>
          <div class="sw-txt"><b>Показывать в боте и на сайте</b><span>Скрытый тариф доступен только по прямой ссылке/промокоду</span></div></div>
        <div class="switch-row"><label class="switch"><input type="checkbox" id="tActive" ${t.active!==false?'checked':''}><span class="tr"></span></label>
          <div class="sw-txt"><b>Тариф активен</b><span>Выключенный нельзя купить, у действующих подписок он продолжает работать</span></div></div>
      </div>`,
    footer:`${isNew?'':`<button class="btn danger sm" data-delete-tariff>${I('trash',12)} Удалить</button>`}<div class="spacer"></div><button class="btn" data-close>Отмена</button><button class="btn primary" data-save>${isNew?'Создать тариф':'Сохранить'}</button>`,
    onMount(layer, close){
      layer.querySelector('[data-delete-tariff]')?.addEventListener('click',()=>deleteTariff(t,close));
      const r = layer.querySelector('#devR'), v = layer.querySelector('#devV');
      r.addEventListener('input', ()=>v.textContent = r.value);
      layer.querySelectorAll('.chip-opt').forEach(c=>c.addEventListener('click', ()=>c.classList.toggle('on')));
      layer.querySelectorAll('.seg button').forEach(b=>b.addEventListener('click', ()=>{ b.parentElement.querySelectorAll('button').forEach(x=>x.classList.remove('on')); b.classList.add('on'); }));
      layer.querySelector('#addPrice').addEventListener('click', ()=>{
        layer.querySelector('#priceRows').insertAdjacentHTML('beforeend',
          priceRowHtml({ d:'', amount:'', stars:'' }));
      });
      // Удаление строки цены — раньше крестик ничего не делал.
      layer.querySelector('#priceRows').addEventListener('click', e=>{
        const rm = e.target.closest('[data-rm]');
        if (rm) rm.closest('.price-row').remove();
      });

      function updateAddonScope(){
        layer.querySelector('[data-addon-plan]').textContent=layer.querySelector('#tTitle').value.trim()||layer.querySelector('#tCode').value.trim()||'Новый тариф';
        const count=kind=>layer.querySelectorAll('[data-addon-rows="'+kind+'"] .addon-price-row').length;
        const devices=count('devices'),traffic=count('traffic');
        layer.querySelector('[data-addon-status]').textContent=devices||traffic?`После сохранения пакеты этого тарифа: устройства — ${devices}, трафик — ${traffic}.`:'Пакеты не добавлены: докупки для этого тарифа выключены.';
        layer.querySelector('[data-addon-traffic-hint]').textContent=layer.querySelector('#tTraffic').value.trim()?'Показывается клиентам этого тарифа с ограниченным трафиком.':'У тарифа безлимитный трафик. Пакеты ГБ доступны только подпискам с ограничением трафика.';
      }
      for(const id of ['tTitle','tCode','tTraffic'])layer.querySelector('#'+id).addEventListener('input',updateAddonScope);
      updateAddonScope();
      if(section==='addons')requestAnimationFrame(()=>{const heading=layer.querySelector('.tariff-addons h4');heading.scrollIntoView({block:'start'});heading.focus({preventScroll:true});});
      layer.querySelectorAll('[data-add-addon]').forEach(b=>b.onclick=()=>{const box=layer.querySelector('[data-addon-rows="'+b.dataset.addAddon+'"]');box.insertAdjacentHTML('beforeend',addonPriceRow({kind:b.dataset.addAddon}));box.lastElementChild.querySelector('input').focus();updateAddonScope();});
      layer.querySelector('.tariff-addons').addEventListener('click',e=>{const b=e.target.closest('[data-remove-addon]');if(b){b.closest('.addon-price-row').remove();updateAddonScope();}});
      const btn = layer.querySelector('[data-save]');
      btn.addEventListener('click', async ()=>{
        // Одна строка формы = один период. В базу она превращается в цену
        // в валюте системы и, если заполнена, отдельную цену в звёздах.
        let prices, addons;
        try { addons=readAddonPrices(layer); prices=tariffPriceValues([...layer.querySelectorAll('.price-row')].map(row=>({
          days:row.querySelector('[data-days]').value, amount:row.querySelector('[data-amount]').value, stars:row.querySelector('[data-stars]').value
        })),DB.currency); } catch(e){toast(e.message,'err');return;}

        const gb = layer.querySelector('#tTraffic').value.trim();
        if(gb!==''&&(!Number.isFinite(Number(gb))||Number(gb)<=0)){toast('Укажите положительный лимит или оставьте поле пустым для безлимита','err');return;}
        const body = {
          code: layer.querySelector('#tCode').value.trim(),
          title: layer.querySelector('#tTitle').value.trim() || layer.querySelector('#tCode').value.trim(),
          description: layer.querySelector('#tDesc').value,
          locales:{en:{title:layer.querySelector('#tTitleEn').value.trim(),badge:layer.querySelector('#tBadgeEn').value.trim(),description:layer.querySelector('#tDescEn').value.trim()}},
          badge: layer.querySelector('#tBadge').value.trim() || null,
          device_limit: parseInt(layer.querySelector('#devR').value, 10),
          // Пусто = безлимит; поле обязано уйти как null, а не пропасть,
          // иначе прежний лимит останется.
          traffic_limit_bytes: gb === '' ? null : Math.round(parseFloat(gb) * 1024 ** 3),
          reset_strategy: layer.querySelector('#tReset .on').dataset.v,
          is_trial:layer.querySelector('#tTrial').checked,
          is_visible: layer.querySelector('#tVisible').checked,
          is_active: layer.querySelector('#tActive').checked,
          prices,
          addons,
          squad_names: [...layer.querySelectorAll('.chip-opt.on')].map(c=>c.dataset.squad),
        };

        if (!body.code) { toast('Нужен код тарифа', 'err'); return; }
        if (!prices.length) { toast('Добавьте хотя бы один период с ценой', 'err'); return; }

        btn.disabled = true;
        try {
          if (isNew) await API.call('/api/tariffs', { method:'POST', body });
          else await API.call('/api/tariffs/'+t.id, { method:'PATCH', body });
          try { await refreshDB(); }
          catch { close(); toast('Тариф сохранён, но список не обновился. Обновите страницу.', 'err'); return; }
          close();
          toast(isNew?'Тариф создан':'Тариф сохранён');
        } catch(e) {
          toast('Не сохранилось: '+e.message, 'err');
          btn.disabled = false;
        }
      });
    }
  });
}

/* ── ПРОМОКОДЫ ── */
registerPage({
  id:'promos', title:'Промокоды', group:'Продажи', icon:'percent',
  render(){
    return `
    <div class="page-head">
      <div><h1>Промокоды</h1><div class="desc">Проценты, фиксированная скидка или дополнительные дни. Клиент применяет код в Telegram-боте до выбора оплаты.</div></div>
      <div class="actions"><button class="btn primary" id="newPromo">${I('plus',14)} Создать промокод</button></div>
    </div>
    <div class="tbl-wrap"><table class="tbl">
      <thead><tr><th>Код</th><th>Эффект</th><th>Применения</th><th>Действует до</th><th>Область</th><th>Статус</th><th style="width:36px"></th></tr></thead>
      <tbody>${DB.promos.map(p=>`
        <tr>
          <td><span class="chip accent" style="font-size:12px;padding:4px 12px;cursor:pointer" onclick="copyText('${p.code}','Код ${p.code} скопирован')">${p.code} ${I('copy',10)}</span></td>
          <td>${p.type==='percent'?`<b>−${p.value}%</b> на оплату`:p.type==='fixed'?`<b>−${fmtMoney(minorToUnits(p.value,p.currency||DB.currency),p.currency||DB.currency)}</b> на оплату`:`<b>+${p.value} дней</b> подписки`}</td>
          <td class="num">${fmtN(p.uses)}${p.limit?` <span style="color:var(--text-3)">/ ${fmtN(p.limit)}</span>`:''}
            ${p.limit?`<div class="progress" style="margin-top:5px;max-width:90px"><i style="width:${Math.min(100,p.uses/p.limit*100)}%"></i></div>`:''}</td>
          <td>${p.until?fmtDate(p.until):'бессрочно'}</td>
          <td><span class="chip">${p.appliesTo}</span></td>
          <td>${p.active?'<span class="bdg ok"><span class="dot"></span>Активен</span>':'<span class="bdg neutral">Выключен</span>'}</td>
          <td><button class="btn ghost icon-only" data-pmenu="${p.id}">${I('more',14)}</button></td>
        </tr>`).join('')||'<tr><td colspan="8"><div class="empty"><b>Промокодов пока нет</b><span>Создайте скидку или дополнительный срок доступа для клиентов.</span><button class="btn primary" data-first-promo>Создать промокод</button></div></td></tr>'}</tbody>
    </table></div>`;
  },
  bind(root){
    root.querySelector('#newPromo').addEventListener('click', ()=>promoModal());
    root.querySelector('[data-first-promo]')?.addEventListener('click',()=>promoModal());
    root.querySelectorAll('[data-pmenu]').forEach(b=>b.addEventListener('click', e=>{
      const p = DB.promos.find(x=>x.id===e.currentTarget.dataset.pmenu);
      menu(e.currentTarget, [
        {label:'Редактировать', icon:'edit', onClick:()=>promoModal(p)},
        {label:p.active?'Выключить':'Включить', icon:'power', onClick:async ()=>{
          try {
            await API.call('/api/promos/'+p.id, { method:'PATCH', body:{ is_active: !p.active } });
            toast(`Промокод ${p.code} ${p.active?'выключен':'включён'}`);
            await refreshDB();
          } catch(e){ toast('Не получилось: '+e.message, 'err'); }
        }},
        '-',
        {label:'Удалить', icon:'trash', danger:true, onClick:()=>confirmModal({
          title:`Удалить ${p.code}?`, text:'Статистика применений сохранится в платежах.', okText:'Удалить',
          onOk:async ()=>{
            try {
              await API.call('/api/promos/'+p.id, { method:'DELETE' });
              toast('Промокод удалён');
              await refreshDB();
            } catch(e){ toast('Не удалился: '+e.message, 'err'); }
          }})},
      ]);
    }));
  }
});
function promoModal(p){
  const isNew = !p;
  openModal({
    title:isNew?'Новый промокод':'Промокод · '+p.code, icon:'percent',
    body:`
      <div class="field"><label>Код <span class="req">*</span></label>
        <div class="inp-row"><input class="inp mono" id="prCode" style="text-transform:uppercase" value="${p?esc(p.code):''}" ${p?'disabled':''} placeholder="SUMMER25"><button class="btn" id="prGen" ${p?'disabled':''}>Сгенерировать</button></div>
        ${p?'<div class="hint">Код существующего промокода менять нельзя: по нему находятся уже сделанные покупки.</div>':''}</div>
      <div class="field"><label>Тип</label>
        <div class="seg" id="prKind" ${p?'style="opacity:.6;pointer-events:none"':''}>
          <button data-v="percent" class="${!p||p.type==='percent'?'on':''}">Скидка %</button>
          <button data-v="days" class="${p&&p.type==='days'?'on':''}">Бонусные дни</button>
          <button data-v="fixed" class="${p&&p.type==='fixed'?'on':''}">Фикс. сумма</button></div></div>
      <div class="two-col">
        <div class="field"><label>Значение</label><div class="inp-group"><input class="inp num" id="prValue" ${p?'disabled':''} value="${p?(p.type==='fixed'?minorToUnits(p.value,p.currency||DB.currency):p.value):15}"><span class="suffix" id="prUnit">%</span></div></div>
        <div class="field"><label>Лимит применений</label><input class="inp num" id="prLimit" value="${p?.limit??''}" placeholder="∞"></div>
      </div>
      <div class="two-col">
        <div class="field"><label>Действует до</label><input class="inp" type="date" id="prUntil" value="${dateInputValue(p?.until)}"></div>
        <div class="field"><label>Статус</label>
          <label class="switch" style="margin-top:8px"><input type="checkbox" id="prActive" ${!p||p.active?'checked':''}><span class="tr"></span></label>
          <div class="hint">Выключенный промокод не применяется, но остаётся в истории.</div></div>
      </div>
      <label class="check"><input type="checkbox" id="prFirst" ${p?.firstOnly?'checked':''} ${p?'disabled':''}><span>Только для первой покупки</span></label>`,
    footer:`<div class="spacer"></div><button class="btn" data-close>Отмена</button><button class="btn primary" data-save>${isNew?'Создать':'Сохранить'}</button>`,
    onMount(layer, close){
      const unit = () => {
        const kind = layer.querySelector('#prKind .on').dataset.v;
        layer.querySelector('#prUnit').textContent =
          kind === 'percent' ? '%' : kind === 'days' ? 'дней' : (p?.currency||DB.currency);
      };
      layer.querySelectorAll('#prKind button').forEach(b=>b.addEventListener('click', ()=>{
        b.parentElement.querySelectorAll('button').forEach(x=>x.classList.remove('on'));
        b.classList.add('on'); unit();
      }));
      unit();

      const gen = layer.querySelector('#prGen');
      if (gen) gen.addEventListener('click', ()=>{
        const abc = 'ABCDEFGHJKLMNPQRSTUVWXYZ23456789';
        layer.querySelector('#prCode').value =
          Array.from({length:8}, ()=>abc[Math.floor(Math.random()*abc.length)]).join('');
      });

      const btn = layer.querySelector('[data-save]');
      btn.addEventListener('click', async ()=>{
        const limit = layer.querySelector('#prLimit').value.trim();
        const until = layer.querySelector('#prUntil').value;
        btn.disabled = true;
        try {
          if (isNew) {
            const code = layer.querySelector('#prCode').value.trim().toUpperCase();
            if (!code) { toast('Нужен код', 'err'); btn.disabled = false; return; }
            const kind = layer.querySelector('#prKind .on').dataset.v;
            const value = Number(layer.querySelector('#prValue').value);
            if(!Number.isFinite(value)||(kind!=='fixed'&&!Number.isInteger(value))){toast('Проверьте значение скидки','err');btn.disabled=false;return;}
            if (!(value > 0)) { toast('Значение должно быть больше нуля', 'err'); btn.disabled = false; return; }
            await API.call('/api/promos', { method:'POST', body:{
              code, kind,
              // Фиксированная скидка — сумма в минимальных единицах валюты системы.
              value: kind === 'fixed' ? unitsToMinor(value, DB.currency) : value,
              currency: kind === 'fixed' ? DB.currency : null,
              max_uses: limit === '' ? null : Number(limit),
              valid_until: until ? new Date(until + 'T23:59:59').toISOString() : null,
              first_purchase_only: layer.querySelector('#prFirst').checked,
              is_active:layer.querySelector('#prActive').checked,
            }});
          } else {
            // У существующего меняем только то, что безопасно менять:
            // тип и значение задним числом исказили бы уже сделанные покупки.
            await API.call('/api/promos/'+p.id, { method:'PATCH', body:{
              is_active: layer.querySelector('#prActive').checked,
              max_uses: limit === '' ? null : Number(limit),
              valid_until: until ? new Date(until + 'T23:59:59').toISOString() : null,
            }});
          }
          close();
          toast(isNew ? 'Промокод создан' : 'Сохранено');
          await refreshDB();
        } catch(e){ toast('Не сохранилось: '+e.message, 'err'); btn.disabled = false; }
      });
    }
  });
}

/* ── ПАРТНЁРЫ ── */
registerPage({
  id:'partners', title:'Партнёрка', group:'Продажи', icon:'users2',
  render(){
    return `
    <div class="page-head">
      <div><h1>Партнёрская программа</h1><div class="desc">Партнёры получают процент с оплат приглашённых клиентов. Балансы и выплаты учитываются отдельно в каждой валюте.</div></div>
      <div class="actions"><button class="btn primary" id="newPartner">${I('plus',14)} Добавить партнёра</button></div>
    </div>
    <div class="grid g3" style="margin-bottom:16px">
      ${(()=>{
        const p = DB.partners;
        const owed = p.filter(x=>(x.wallets||[]).some(w=>w.balance_minor>0));
        return [
          ['Приведено клиентов', fmtN(p.reduce((a,x)=>a + (x.referred||0), 0)), 'за всё время'],
          ['Выплачено', partnerTotals(p, 'paid_minor'), `${p.length} партнёров`],
          ['К выплате сейчас', partnerTotals(p, 'balance_minor', true),
           owed.length ? `${owed.length} ждут выплаты` : 'долгов нет'],
        ];
      })().map(([t,v,d])=>`
        <div class="card" style="padding:13px 16px"><div class="label">${t}</div><div class="metric num" style="font-size:20px">${v}</div><div class="sub-note">${d}</div></div>`).join('')}
    </div>
    <div class="tbl-wrap"><table class="tbl">
      <thead><tr><th>Партнёр</th><th>Ставка</th><th>Приведено</th><th>Заработано</th><th>Выплачено</th><th>Баланс</th><th>Реф-ссылка</th><th style="width:36px"></th></tr></thead>
      <tbody>${DB.partners.map(p=>`
        <tr>
          <td><div class="cell-main"><div class="avatar-sm">${esc(p.user[0].toUpperCase())}</div><b>${esc(p.user)}</b></div></td>
          <td><span class="chip accent">${p.share}%</span></td>
          <td class="num">${fmtN(p.referred)}</td>
          <td class="num">${partnerAmounts(p,'earned_minor')}</td>
          <td class="num" style="color:var(--text-2)">${partnerAmounts(p,'paid_minor')}</td>
          <td class="num" style="font-weight:600;color:${p.balance>0?'var(--accent)':'var(--text-2)'}">${partnerAmounts(p,'balance_minor')}</td>
          <td>${p.link?`<button class="btn sm" data-partner-link="${p.id}" title="${esc(p.link)}">${esc(p.slug)} ${I('copy',10)}</button>`:'<span class="hint">Подключите Telegram-бота</span>'}</td>
          <td><button class="btn ghost icon-only" data-pamenu="${p.id}">${I('more',14)}</button></td>
        </tr>`).join('')||'<tr><td colspan="8"><div class="empty"><b>Партнёров пока нет</b><span>Добавьте партнёра, чтобы отслеживать приглашённых клиентов и начислять комиссию.</span><button class="btn primary" data-first-partner>Добавить партнёра</button></div></td></tr>'}</tbody>
    </table></div>`;
  },
  bind(root){
    root.querySelector('[data-first-partner]')?.addEventListener('click',()=>root.querySelector('#newPartner').click());
    root.querySelector('#newPartner').addEventListener('click', ()=>openModal({
      title:'Новый партнёр', icon:'users2',
      body:`
        <div class="field"><label>Клиент</label><input class="inp" id="paUser" placeholder="username существующего клиента">
          <div class="hint">Комиссия начисляется на его же аккаунт. Пусто — партнёр без привязки к клиенту.</div></div>
        <div class="field"><label>Название <span class="req">*</span></label><input class="inp" id="paTitle" placeholder="Блог про VPN"></div>
        <div class="field"><label>Ставка</label><div class="slider-row"><input type="range" min="0" max="100" step="0.01" value="20" id="shR"><span class="num" style="width:38px" id="shV">20%</span></div>
          <div class="hint">Процент с каждой успешной оплаты приведённого клиента, пожизненно.</div></div>
        <div class="field"><label>Слаг ссылки</label><div class="inp-group"><span class="suffix" style="border-radius:9px 0 0 9px;border-right:0">?start=r_</span><input class="inp mono" id="paSlug" maxlength="62" style="border-radius:0 9px 9px 0" placeholder="myslug"></div>
          <div class="hint">Латиница, цифры, дефис. По нему считаются приведённые клиенты.</div></div>`,
      footer:`<div class="spacer"></div><button class="btn" data-close>Отмена</button><button class="btn primary" data-save>Создать</button>`,
      onMount(layer, close){
        const r=layer.querySelector('#shR'), v=layer.querySelector('#shV');
        r.addEventListener('input',()=>v.textContent=r.value+'%');
        const btn = layer.querySelector('[data-save]');
        btn.addEventListener('click', async ()=>{
          const slug = layer.querySelector('#paSlug').value.trim();
          if (!slug) { toast('Нужен слаг ссылки', 'err'); return; }
          btn.disabled = true;
          try {
            await API.call('/api/partners', { method:'POST', body:{
              username: layer.querySelector('#paUser').value.trim().replace(/^@/, '') || null,
              title: layer.querySelector('#paTitle').value.trim() || null,
              slug,
              share_percent: Number(layer.querySelector('#shR').value),
            }});
            close();
            toast('Партнёр добавлен');
            await refreshDB();
          } catch(e){ toast('Не создался: '+e.message, 'err'); btn.disabled = false; }
        });
      }
    }));
    root.querySelectorAll('[data-partner-link]').forEach(b=>b.onclick=()=>copyText(DB.partners.find(p=>p.id===b.dataset.partnerLink).link,'Ссылка скопирована'));
    root.querySelectorAll('[data-pamenu]').forEach(b=>b.addEventListener('click', e=>{
      const p = DB.partners.find(x=>x.id===e.currentTarget.dataset.pamenu);
      menu(e.currentTarget, [
        {label:'Отметить выплату', icon:'dollar', onClick:()=>partnerPayoutModal(p)},
        {label:'Приведённые клиенты', icon:'users2', onClick:()=>{usersPartnerFilter=p.slug;navigate('users');}},
        {label:'Изменить ставку', icon:'percent', onClick:()=>partnerRateModal(p)},
        '-',
        {label:p.active===false?'Включить партнёрку':'Отключить партнёрку', icon:'ban', danger:p.active!==false,
          onClick:async ()=>{
            try {
              await API.call('/api/partners/'+p.id, { method:'PATCH', body:{ is_active: p.active === false } });
              toast(p.active === false ? 'Партнёрка включена' : 'Партнёрка отключена');
              await refreshDB();
            } catch(e){ toast('Не получилось: '+e.message, 'err'); }
          }},
      ]);
    }));
  }
});

/* ── РАССЫЛКИ ── */
registerPage({
  id:'broadcasts', title:'Рассылки', group:'Продажи', icon:'send',
  render(){
    const st = {sent:['Отправлена','ok'], scheduled:['Запланирована','info'], draft:['Черновик','neutral'],sending:['Отправляется','info'],canceled:['Отменена','neutral'],failed:['Ошибка отправки','err']};
    return `
    <div class="page-head">
      <div><h1>Рассылки</h1><div class="desc">Сообщения клиентам в Telegram-бот. Сегменты, черновики, отложенная отправка и результаты доставки.</div></div>
      <div class="actions"><button class="btn" onclick="refreshDB()">${I('refresh',14)} Обновить</button><button class="btn primary" id="newBc">${I('plus',14)} Новая рассылка</button></div>
    </div>
    <div class="sub-note" id="broadcastWorker" role="status"></div>
    <div class="tbl-wrap"><table class="tbl">
      <thead><tr><th>Рассылка</th><th>Аудитория</th><th>Доставлено</th><th>Ошибки доставки</th><th>Статус</th><th>Время</th><th style="width:36px"></th></tr></thead>
      <tbody>${DB.broadcasts.map(b=>{
        const [t,c] = b.status==='sent'&&b.failed ? ['Завершена с ошибками','warn'] : st[b.status] || [b.status,'neutral'];
        return `<tr data-brow="${b.id}">
          <td><b>${esc(b.name)}</b><div class="sub-note" data-bc-error>${esc(b.lastError||'')}</div></td>
          <td><span class="chip">${esc(b.audience)}</span></td>
          <td class="num" data-bc-sent>${fmtN(b.sent||0)} / ${fmtN(b.total||0)}</td>
          <td class="num" data-bc-failed>${fmtN(b.failed||0)}</td>
          <td data-bc-status><span class="bdg ${c}"><span class="dot"></span>${t}</span></td>
          <td class="num" style="font-size:12px;color:var(--text-2)">${b.at?fmtDT(b.at):'—'}</td>
          <td><button class="btn ghost icon-only" data-bmenu="${b.id}">${I('more',14)}</button></td>
        </tr>`;}).join('')}</tbody>
    </table></div>`;
  },
  bind(root){
    const statusText={sent:['Отправлена','ok'],scheduled:['Запланирована','info'],draft:['Черновик','neutral'],sending:['Отправляется','info'],canceled:['Отменена','neutral'],failed:['Ошибка отправки','err']};
    const update=async()=>{if(!root.isConnected)return;try{
      const [worker,rows]=await Promise.all([API.call('/api/broadcasts/worker'),API.call('/api/broadcasts')]);if(!root.isConnected)return;
      const note=root.querySelector('#broadcastWorker');note.textContent=!worker.alive?'Служба рассылок не отвечает. Проверьте sn-worker.':!worker.bot_configured?'Токен Telegram-бота не настроен.':'Служба рассылок работает. Результаты обновляются автоматически.';
      for(const r of rows){const b=DB.broadcasts.find(b=>b.id===String(r.id));if(b)Object.assign(b,{status:r.status,sent:r.sent_count,failed:r.failed_count,total:r.total_count,lastError:r.last_error,retryAt:r.retry_at});
        const tr=root.querySelector('[data-brow="'+r.id+'"]');if(!tr)continue;
        tr.querySelector('[data-bc-error]').textContent=r.last_error||'';
        tr.querySelector('[data-bc-sent]').textContent=fmtN(r.sent_count)+' / '+fmtN(r.total_count);
        tr.querySelector('[data-bc-failed]').textContent=fmtN(r.failed_count);
        const [t,c]=r.status==='sent'&&r.failed_count ? ['Завершена с ошибками','warn'] : statusText[r.status]||[r.status,'neutral'];tr.querySelector('[data-bc-status]').innerHTML='<span class="bdg '+c+'">'+esc(t)+'</span>';
      }translateNode(root);
    }catch(e){if(root.isConnected)root.querySelector('#broadcastWorker').textContent=e.message;}
      if(root.isConnected)setTimeout(update,10000);
    };update();

    root.querySelector('#newBc').addEventListener('click', ()=>broadcastModal());
    // Строка открывает рассылку: она выглядела нажимаемой, но отзывалась
    // только через меню из трёх точек.
    root.querySelectorAll('[data-brow]').forEach(tr=>tr.addEventListener('click', e=>{
      if (e.target.closest('input, button, label, .chip')) return;
      const bc = DB.broadcasts.find(x=>x.id===tr.dataset.brow);
      if (bc) broadcastModal(bc);
    }));
    root.querySelectorAll('[data-bmenu]').forEach(b=>b.addEventListener('click', e=>{
      const bc = DB.broadcasts.find(x=>x.id===e.currentTarget.dataset.bmenu);
      menu(e.currentTarget, [
        {label:'Открыть', icon:'eye', onClick:()=>broadcastModal(bc)},
        {label:'Дублировать', icon:'copy', onClick:async ()=>{
          try {
            await API.call('/api/broadcasts', { method:'POST', body:{
              title: bc.name + ' (копия)', body: bc.body || '', segment: bc.segment || 'all',button_text:bc.buttonText||null,button_url:bc.buttonUrl||null } });
            toast('Копия создана в черновиках');
            await refreshDB();
          } catch(e){ toast('Не скопировалась: '+e.message, 'err'); }
        }},
        ...(bc.status==='sending'||bc.status==='scheduled'
          ? [{label:'Отменить отправку', icon:'ban', danger:true, onClick:async ()=>{
              try {
                await API.call('/api/broadcasts/'+bc.id+'/cancel', { method:'POST' });
                toast('Рассылка отменена');
                await refreshDB();
              } catch(e){ toast('Не отменилась: '+e.message, 'err'); }
            }}]
          : []),
      ]);
    }));
  }
});
function broadcastModal(bc){
  const isNew = !bc, editable = isNew || bc.status==='draft';
  openDrawer({
    title:isNew?'Новая рассылка':'Рассылка · '+esc(bc.name), sub:'Telegram-бот · HTML-разметка', icon:'send', size:'lg',
    body:`
      ${!isNew?'<div id="broadcastDelivery" class="sub-note" role="status"></div>':''}
      <div class="field"><label>Название (внутреннее)</label><input class="inp" id="bcName" value="${bc?esc(bc.name):''}" placeholder="Август: скидка на годовые"></div>
      <div class="field"><label>Сегмент аудитории</label>
        <select class="inp" id="bcSeg">
          <option value="all">Все клиенты</option>
          <option value="active">Активные подписки</option>
          <option value="expired">Истёкшие</option>
          <option value="limited">На лимите трафика</option>
          <option value="trial">На пробном тарифе</option>
        </select>
        <div class="hint">В счёт идут только клиенты с привязанным Telegram: остальным сообщение доставить нечем.</div></div>
      <div class="field"><label>Сообщение</label>
        <textarea class="inp" rows="7" id="bcBody" placeholder="<b>Скидка 25%</b> на годовые тарифы до конца недели!&#10;&#10;Успей продлить → /renew">${bc?esc(bc.body||''):''}</textarea>
        <div class="hint">HTML: &lt;b&gt;, &lt;i&gt;, &lt;a href&gt;, &lt;code&gt;. Плейсхолдеры: {name}, {tariff}, {expire_date}.</div></div>
      <div class="field"><label>Кнопка (опционально)</label>
        <div class="inp-row"><input class="inp" id="bcBtnText" value="${esc(bc?.buttonText||'')}" placeholder="Текст: 🔥 Продлить со скидкой"><input class="inp mono" id="bcBtnUrl" value="${esc(bc?.buttonUrl||'')}" placeholder="https://…"></div></div>
      <div class="hint">${I('info',12)} Рассылка всегда сохраняется черновиком: отправка — отдельное действие, чтобы случайное нажатие не ушло на всю базу.</div>`,
    footer:`<div class="spacer"></div><button class="btn" data-close>Закрыть</button>
            ${!isNew?'<button class="btn" data-test>Отправить тест</button>':''}${editable?'<button class="btn primary" data-save>Сохранить черновик</button>':''}${!isNew&&editable?'<button class="btn" data-send>Отправить…</button>':''}`,
    onMount(layer, close){
      layer.querySelector('#bcSeg').value=bc?.segment||'all';
      if(!isNew){API.call('/api/broadcasts/'+bc.id).then(r=>{if(!layer.isConnected)return;const box=layer.querySelector('#broadcastDelivery');box.innerHTML=(r.last_error?'<p class="sub-note">'+esc(r.last_error)+'</p>':'')+(r.retry_at?'<p>Повтор: '+esc(fmtDT(r.retry_at))+'</p>':'')+r.errors.map(x=>'<p>'+esc(x.username)+': '+esc(x.error)+'</p>').join('');translateNode(box);}).catch(e=>{if(layer.isConnected)layer.querySelector('#broadcastDelivery').textContent=e.message;});
        layer.querySelector('[data-test]').onclick=()=>openModal({title:'Тестовая отправка',size:'sm',body:'<form id="broadcastTestForm"><label for="broadcastTestId">Telegram ID получателя</label><input class="inp" id="broadcastTestId" inputmode="numeric" pattern="[0-9]+" required><p class="sub-note">Отправляется сохранённый текст с кнопкой. Получатель должен предварительно запустить вашего бота.</p></form>',footer:'<button class="btn" data-close>Отмена</button><button class="btn primary" type="submit" form="broadcastTestForm">Отправить тест</button>',onMount(m,done){m.querySelector('form').onsubmit=async e=>{e.preventDefault();const b=m.querySelector('[type=submit]');b.disabled=true;try{await API.call('/api/broadcasts/'+bc.id+'/send',{method:'POST',body:{test_to:Number(m.querySelector('input').value)}});done();toast('Тест доставлен');}catch(e){toast(e.message,'err');b.disabled=false;}};}});
      }

      if(!editable) layer.querySelectorAll('.m-body input,.m-body select,.m-body textarea').forEach(el=>el.disabled=true);
      const save = layer.querySelector('[data-save]');
      if (save) save.addEventListener('click', async ()=>{
        const title = layer.querySelector('#bcName').value.trim();
        const body = layer.querySelector('#bcBody').value.trim();
        if (!title || !body) { toast('Нужны название и текст', 'err'); return; }
        save.disabled = true;
        try {
          const r = await API.call('/api/broadcasts'+(isNew?'':'/'+bc.id), { method:isNew?'POST':'PATCH', body:{
            title, body,
            segment: layer.querySelector('#bcSeg').value,
            button_text: layer.querySelector('#bcBtnText').value.trim() || null,
            button_url: layer.querySelector('#bcBtnUrl').value.trim() || null,
          }});
          close();
          toast('Черновик сохранён');
          await refreshDB();
        } catch(e){ toast('Не сохранилось: '+e.message, 'err'); save.disabled = false; }
      });

      const send = layer.querySelector('[data-send]');
      if (send) send.addEventListener('click', ()=>confirmModal({
        title:'Запустить рассылку?', danger:false,
        text:`Будет отправлена сохранённая версия. Изменения в полях сначала сохраните.<br>Время отправки (пусто — сразу):<input class="inp" type="datetime-local" id="bcSchedule"><br>Сообщение уйдёт сегменту <b>${esc(bc.audience || '')}</b>: получателей <b>${fmtN(bc.total || 0)}</b>. Отменить можно, пока очередь не разошлась.`,
        okText:'Запустить',
        onOk:async ()=>{
          try {
            const at=document.getElementById('bcSchedule')?.value;
            await API.call('/api/broadcasts/'+bc.id+'/send', { method:'POST',body:{scheduled_at:at?new Date(at).toISOString():null} });
            close();
            toast('Рассылка запущена');
            await refreshDB();
          } catch(e){ toast('Не запустилась: '+e.message, 'err'); }
        }
      }));
    }
  });
}


/* ── ПЛАТЁЖКИ И ВАЛЮТЫ ── */
registerPage({
  id:'providers', title:'Платёжки и валюты', group:'Продажи', icon:'dollar',
  render(){
    return `
    <div class="page-head">
      <div><h1>Платёжки и валюты</h1><div class="desc">Какими способами клиент может заплатить и в какой валюте.</div></div>
    </div>
    <div class="card" style="margin-bottom:12px">
      <div class="section-h" style="margin:0 0 12px"><h2>${I('globe',14)} Валюта системы</h2></div>
      <p class="sub-note" style="margin-bottom:12px">В ней задаются цены тарифов и считаются отчёты. Валюта одна на весь сервис: несколько сразу означали бы курс, пересчёт и расхождения в отчётности.</p>
      <div class="inp-group" style="max-width:280px">
        <select class="inp" id="sysCur">${['USD','EUR','RUB','UAH','KZT','TRY','GBP'].map(c=>`<option ${c===DB.currency?'selected':''}>${c}</option>`).join('')}</select>
        <button class="btn sm" id="sysCurSave">Сохранить</button>
      </div>
      <div class="hint" style="margin-top:8px">Смена валюты не пересчитывает уже проставленные цены — их нужно перепроверить в тарифах.</div>
    </div>
    <div id="provWarn"></div>
    <div class="card"><div class="section-h" style="margin:0 0 12px"><h2>${I('card',14)} Модули оплаты</h2></div>
      <div id="provList" class="sub-note">Загружаем…</div>
    </div>
    <div class="card" style="margin-top:var(--panel-gap,14px)">
      <div class="section-h" style="margin:0 0 12px"><h2>${I('dollar',14)} Чем можно заплатить</h2></div>
      <p class="sub-note" style="margin-bottom:12px">Цена бесполезна, если её валюту не принимает ни один включённый модуль: клиент увидит сумму, но не увидит кнопку оплаты.</p>
      <div id="curList"></div>
    </div>
    <div class="card" style="margin-top:var(--panel-gap,14px)">
      <div class="section-h" style="margin:0 0 12px"><h2>${I('info',14)} Как добавить свою платёжку</h2></div>
      <p class="sub-note" style="margin-bottom:12px">Выбирайте самый первый подходящий способ — он дешевле в поддержке.</p>

      <div class="info-row" style="align-items:flex-start;gap:12px">
        <span class="bdg accent" style="margin-top:2px">1</span>
        <div style="flex:1">
          <b style="font-size:12.5px">Настроить прямо здесь</b>
          <div class="sub-note" style="margin-top:4px">Подходит, если у провайдера обычный REST API: принимает POST с суммой, возвращает ссылку на оплату и шлёт вебхук со статусом. Откройте «Своя платёжка» → «Ключи и настройки». Кода писать не нужно.</div>
        </div>
      </div>

      <div class="info-row" style="align-items:flex-start;gap:12px">
        <span class="bdg accent" style="margin-top:2px">2</span>
        <div style="flex:1">
          <b style="font-size:12.5px">Написать свой модуль</b>
          <div class="sub-note" style="margin-top:4px">Нужен при нестандартной подписи, протоколе в несколько шагов или автосписании. Один файл в <code>crates/payments/src/providers/</code> плюс строка в реестре; поля настроек модуль описывает сам, форму в панели править не нужно. Пошагово — в <code>crates/payments/README.md</code>.
          <br><b>Новый модуль требует пересборки проекта:</b> это скомпилированный код, загрузить его в работающую панель нельзя.</div>
        </div>
      </div>

      <div class="info-row" style="align-items:flex-start;gap:12px">
        <span class="bdg accent" style="margin-top:2px">3</span>
        <div style="flex:1">
          <b style="font-size:12.5px">Принимать переводы вручную</b>
          <div class="sub-note" style="margin-top:4px">Модуль «Вручную»: клиент переводит как договорились, вы подтверждаете платёж в разделе «Платежи». Настраивать нечего.</div>
        </div>
      </div>

      <div class="hint" style="margin-top:12px">${I('lock',12)}
        Ключи хранятся в базе панели и обратно в браузер не отдаются — показывается только признак «задан».
        Пустое поле при сохранении означает «оставить прежний ключ».</div>
    </div>`;
  },
  async bind(root){
    const draw = async () => {
      let list;
      try { list = await API.call('/api/pay/providers'); }
      catch(e){ root.querySelector('#provList').innerHTML = 'Не удалось загрузить: '+esc(e.message); return; }

      root.querySelector('#provList').innerHTML = list.map(p=>{
        // Валюты модуля: отмеченные реально предлагаются клиенту.
        const on = p.enabled_currencies || p.currencies;
        return `
        <div class="info-row" style="align-items:flex-start;gap:12px;padding:12px 0">
          <label class="switch" style="margin-top:2px"><input type="checkbox" data-pid="${esc(p.id)}" aria-label="Включить ${esc(p.title)}"
            ${p.is_enabled?'checked':''} ${p.is_configured?'':'disabled'}><span class="tr"></span></label>
          <div style="flex:1;min-width:0">
            <div style="display:flex;align-items:center;gap:8px;flex-wrap:wrap">
              <b style="font-size:13px">${esc(p.title)}</b>
              <span class="mono sub-note">${esc(p.id)}</span>
              ${p.is_configured
                ? '<span class="bdg ok"><span class="dot"></span>ключи найдены</span>'
                : '<span class="bdg warn">ключи не заданы</span>'}
            </div>
            <div class="chips-select" style="margin-top:8px">
              ${p.currencies.map(c=>`<button type="button" class="chip-opt ${on.includes(c)?'on':''}" aria-pressed="${on.includes(c)}"
                 data-cur-of="${esc(p.id)}" data-cur="${esc(c)}">${esc(c)}</button>`).join('')}
            </div>
            ${p.fields && p.fields.length
              ? `<button class="btn sm" style="margin-top:8px" data-setup="${esc(p.id)}">${I('key',12)} Ключи и настройки</button>`
              : '<div class="hint" style="margin-top:6px">Настраивать нечего — модуль работает как есть.</div>'}
          </div>
        </div>`;
      }).join('') || '<div class="sub-note">Модулей нет.</div>';

      const live = (cur) => list.filter(p=>p.is_enabled && p.is_configured
        && (p.enabled_currencies || p.currencies).includes(cur));

      // Звёзды показываем отдельной строкой и только если цены в них есть:
      // это не вторая валюта системы, а отдельный способ оплаты.
      const starsUsed = DB.tariffs.some(t=>(t.prices||[]).some(pr=>pr.cur===STARS)||(t.addons||[]).some(a=>a.stars_minor>0));
      const rows = [{ cur: DB.currency, label: 'Валюта системы' }];
      if (starsUsed) rows.push({ cur: STARS, label: 'Telegram Stars · отдельная цена' });

      const orphan = rows.filter(r=>!live(r.cur).length).map(r=>r.cur);
      root.querySelector('#provWarn').innerHTML = orphan.length ? `
        <div class="notice" style="background:color-mix(in srgb, var(--warn) 10%, transparent);border:1px solid color-mix(in srgb, var(--warn) 30%, transparent);color:var(--warn-ink);border-radius:10px;padding:11px 14px;font-size:12.5px;margin-bottom:12px">
          ${I('alert',13)} Оплатить нечем: <b>${orphan.map(esc).join(', ')}</b> не принимает ни один включённый модуль.
          Клиент увидит цену, но не увидит кнопку оплаты.
        </div>` : '';

      root.querySelector('#curList').innerHTML = rows.map(r=>{
        const ps = live(r.cur);
        return `<div class="info-row">
          <span class="k mono" style="width:70px">${esc(r.cur)}</span>
          <span class="v" style="flex-direction:column;align-items:flex-start">
            <b style="font-size:12.5px">${ps.length ? ps.map(p=>esc(p.title)).join(', ')
              : '<span class="bdg warn">нет модуля</span>'}</b>
            <span class="sub-note">${esc(r.label)}</span>
          </span></div>`;
      }).join('');

      // Форма ключей модуля
      root.querySelectorAll('[data-setup]').forEach(b=>b.addEventListener('click', ()=>
        providerSetupModal(list.find(x=>x.id===b.dataset.setup), draw)));

      // Тумблер модуля
      root.querySelectorAll('[data-pid]').forEach(cb=>cb.addEventListener('change', async e=>{
        try {
          await API.call('/api/pay/providers/'+cb.dataset.pid,
            { method:'PATCH', body:{ is_enabled: e.target.checked } });
          toast('Модуль '+(e.target.checked?'включён':'выключен'));
        } catch(err){ toast('Не сохранилось: '+err.message, 'err'); }
        await draw();
      }));

      // Валюты модуля: клик по чипу переключает и сразу сохраняет.
      root.querySelectorAll('[data-cur-of]').forEach(chip=>chip.addEventListener('click', async ()=>{
        const pid = chip.dataset.curOf;
        const p = list.find(x=>x.id===pid);
        const cur = new Set(p.enabled_currencies || p.currencies);
        cur.has(chip.dataset.cur) ? cur.delete(chip.dataset.cur) : cur.add(chip.dataset.cur);
        try {
          await API.call('/api/pay/providers/'+pid,
            { method:'PATCH', body:{ enabled_currencies: [...cur] } });
          // Пустой список сервер трактует как «все» — сообщаем честно.
          toast(cur.size ? 'Валюты модуля обновлены' : 'Сняты все валюты — модуль снова принимает все свои');
        } catch(err){ toast('Не сохранилось: '+err.message, 'err'); }
        await draw();
      }));
    };
    root.querySelector('#sysCurSave').addEventListener('click', async ()=>{
      const cur = root.querySelector('#sysCur').value;
      // Цены уже проставлены в прежней валюте — пересчитать их мы не можем
      // и не должны: курс знает только владелец сервиса.
      confirmModal({
        title:'Сменить валюту системы?',
        text:`Новые цены будут задаваться в <b>${esc(cur)}</b>. Уже проставленные цены тарифов останутся прежними числами — их нужно перепроверить вручную.`,
        okText:'Сменить',
        onOk:async ()=>{
          try {
            await API.call('/api/settings', { method:'PATCH', body:{ 'billing.currency': cur } });
            toast('Валюта системы: '+cur);
            await refreshDB();
          } catch(e){ toast('Не сохранилось: '+e.message, 'err'); }
        }
      });
    });

    await draw();
  }
});


/* Форма ключей платёжного модуля.

   Поля рисуются по схеме, которую отдаёт сам модуль: ядро не знает, что у
   одной платёжки токен, а у другой — пара «магазин + подпись».
   Секреты в браузер не приходят: показывается только признак «задан», а
   пустое поле означает «оставить как было». */
function providerSetupModal(p, onSaved){
  const guides = {
    platega:{url:'https://docs.platega.io/',text:'Укажите Merchant ID и X-Secret из кабинета Platega. В кабинете задайте адрес уведомлений ниже. Клиент выберет способ оплаты на форме Platega. Модуль принимает RUB.'},
    rollypay:{url:'https://docs.rollypay.io/',text:'Укажите рабочий API key и signing_secret вашей кассы. В кассе включите HTTP-уведомления на адрес ниже. Модуль принимает RUB и EUR; для EUR используется intl_card, минимум 1 EUR.'},
    paritypay:{url:'http://docs.paritypay.net/',text:'Укажите ID кассы и оба секретных ключа: №1 для API, №2 для уведомлений. Задайте URL уведомлений здесь или в кассе ParityPay. Подключение использует API v2 и принимает RUB.'}
  };
  const guide=guides[p.id];
  const callbackUrl=location.origin+'/api/pay/webhook/'+p.id;
  const f = (x) => `
    <div class="field">
      <label for="provider-${esc(x.key)}">${esc(x.label)}${x.required?' <span class="req">*</span>':''}</label>
      <input id="provider-${esc(x.key)}" class="inp mono" data-k="${esc(x.key)}" autocomplete="off" type="${x.secret?'password':'text'}"
             value="${x.secret?'':esc(x.value||'')}"
             placeholder="${x.secret && x.filled ? '•••••• (задан — оставьте пустым, чтобы не менять)' : esc(x.default||'')}">
      <div class="hint">${esc(x.hint)}</div>
    </div>`;

  openModal({
    title:'Настройка · '+esc(p.title), sub:p.id, icon:'key', size:'md',
    body:`
      ${p.id === 'http' ? `
      <div class="hint" style="margin-bottom:14px">${I('info',12)}
        Универсальный модуль для платёжек с обычным REST API — подойдёт, если провайдер
        принимает POST с суммой, возвращает ссылку на оплату и шлёт вебхук со статусом.
        Для нестандартных схем подписи и автосписания нужен свой модуль.</div>` : ''}
      ${guide?`<div class="notice provider-guide"><p>${esc(guide.text)}</p><a class="btn sm" href="${esc(guide.url)}" target="_blank" rel="noopener noreferrer">${I('external',13)} Документация ${esc(p.title)}</a></div>`:''}
      ${p.fields.map(f).join('')}
      <div class="field"><label>Заметка</label>
        <input class="inp" data-note value="${esc(p.note||'')}" placeholder="комиссия, кому писать при сбоях">
      </div>
      <div class="notice" style="background:color-mix(in srgb, var(--accent) 8%, transparent);border:1px solid color-mix(in srgb, var(--accent) 25%, transparent);border-radius:10px;padding:11px 14px;font-size:12.5px;margin-top:6px">
        ${['stars','tg_stars'].includes(p.id)?`${I('info',13)} Telegram Stars подтверждает оплату через работающий бот. Отдельный HTTP webhook для Stars не требуется.`:`<div class="provider-callback"><b>Адрес HTTP-уведомлений</b><code>${esc(callbackUrl)}</code><button class="btn sm" data-copy-callback>${I('copy',13)} Скопировать адрес</button></div>`}
      </div>`,
    footer:`<div class="spacer"></div><button class="btn" data-close>Отмена</button>
            <button class="btn primary" data-save>Сохранить</button>`,
    onMount(layer, close){
      layer.querySelector('[data-copy-callback]')?.addEventListener('click',()=>copyText(callbackUrl));
      const btn = layer.querySelector('[data-save]');
      btn.addEventListener('click', async ()=>{
        const settings = {};
        for (const el of layer.querySelectorAll('[data-k]')) {
          const v = el.value.trim();
          const spec = p.fields.find(x=>x.key===el.dataset.k);
          // Пустой секрет не отправляем: сервер воспримет его как «не менять».
          if (!v && spec.secret) continue;
          settings[el.dataset.k] = v;
        }
        // Обязательное поле, которое пусто и ни разу не заполнялось, — ошибка
        // здесь понятнее, чем «платёжка недоступна» при первой оплате.
        for (const x of p.fields) {
          if (!x.required) continue;
          const filledNow = settings[x.key] !== undefined && settings[x.key] !== '';
          if (!filledNow && !x.filled) { toast(`Не заполнено: ${x.label}`, 'err'); return; }
        }

        btn.disabled = true;
        try {
          await API.call('/api/pay/providers/'+p.id, { method:'PATCH',
            body:{ settings, note: layer.querySelector('[data-note]').value.trim() } });
          close();
          toast('Настройки модуля сохранены');
          await onSaved();
        } catch(e){ toast('Не сохранилось: '+e.message, 'err'); btn.disabled = false; }
      });
    }
  });
}


/* Строки редактора цен: одна строка = один период.

   В базе у периода может быть две записи — в валюте системы и в звёздах, —
   но для человека это одна строка «30 дней = 5.99 USD / 500 ★». Показывать
   их отдельными строками значит предлагать завести валюту, которой в
   системе нет. */
/* Цены не в валюте сервиса.

   Цены лежат отдельной строкой на каждую валюту, а смена валюты в
   настройках ничего не пересчитывает. Карточка при этом рисует суммы со
   значком новой валюты — выглядит так, будто всё перевелось, а бот и
   кабинет показывают только строки в валюте сервиса, то есть у клиента
   тариф остаётся без цены. Молчать об этом нельзя. */
function чужиеВалюты(t){
  return [...new Set((t.prices || [])
    .filter(p => p.cur && p.cur !== STARS && p.cur !== DB.currency)
    .map(p => p.cur))];
}

function tariffDescription(value){
  return esc(value||'').replace(/&lt;(\/?)(b|i|u|s|code|pre)&gt;/g,'<$1$2>');
}

/* Покупка возможна в валюте сервиса или в Telegram Stars. */
function ценаЕсть(t){
  return (t.prices || []).some(p => p.cur === DB.currency || p.cur === STARS || !p.cur);
}

function periodRows(t){
  const byDays = new Map();
  for (const p of (t.prices || [])) {
    const row = byDays.get(p.d) || { d: p.d, amount: '', stars: '' };
    if (p.cur === STARS) row.stars = p.p;
    else if (!p.cur || p.cur === DB.currency) row.amount = p.p;
    byDays.set(p.d, row);
  }
  const rows = [...byDays.values()].sort((a, b) => a.d - b.d);
  return rows.length ? rows : [{ d: '', amount: '', stars: '' }];
}

function priceRowHtml(r){
  return `
    <div class="price-row">
      <div class="inp-group" style="flex:1"><input class="inp num" aria-label="Длительность периода в днях" data-days value="${r.d}" placeholder="30"><span class="suffix">дней</span></div>
      <div class="inp-group" style="flex:1"><input class="inp num" aria-label="Цена в валюте системы" data-amount value="${r.amount}" placeholder="4.99"><span class="suffix">${esc(DB.currency)}</span></div>
      <div class="inp-group" style="flex:1"><input class="inp num" aria-label="Цена в Telegram Stars" data-stars value="${r.stars}" placeholder="—"><span class="suffix">★</span></div>
      <button class="btn icon-only" data-rm title="Убрать период">${I('x',13)}</button>
    </div>`;
}


/* Смена ставки партнёра. Задним числом уже начисленные комиссии не
   пересчитываются: они записаны по ставке, действовавшей на момент оплаты. */
function partnerRateModal(p){
  openModal({
    title:'Ставка · '+esc(p.user), icon:'percent', size:'sm',
    body:`
      <div class="field"><label>Процент с оплат</label>
        <div class="slider-row"><input type="range" min="0" max="100" step="0.01" value="${p.share}" id="prR">
          <span class="num" style="width:38px" id="prV">${p.share}%</span></div>
        <div class="hint">Новая ставка применится к будущим оплатам. Уже начисленные комиссии не пересчитываются.</div></div>`,
    footer:`<div class="spacer"></div><button class="btn" data-close>Отмена</button>
            <button class="btn primary" data-save>Сохранить</button>`,
    onMount(layer, close){
      const r = layer.querySelector('#prR'), v = layer.querySelector('#prV');
      r.addEventListener('input', ()=>v.textContent = r.value + '%');
      layer.querySelector('[data-save]').addEventListener('click', async ()=>{
        try {
          await API.call('/api/partners/'+p.id, { method:'PATCH',
            body:{ share_percent: Number(r.value) } });
          close();
          toast('Ставка обновлена');
          await refreshDB();
        } catch(e){ toast('Не сохранилось: '+e.message, 'err'); }
      });
    }
  });
}


function partnerAmounts(p, key){
  return (p.wallets||[]).map(w=>fmtMoney(minorToUnits(w[key],w.currency),w.currency)).join('<br>') || '—';
}
function partnerTotals(partners, key, positive=false){
  const amounts=new Map();
  for(const p of partners) for(const w of (p.wallets||[])) amounts.set(w.currency,(amounts.get(w.currency)||0)+(positive?Math.max(0,w[key]):w[key]));
  return [...amounts].map(([cur,n])=>fmtMoney(minorToUnits(n,cur),cur)).join('<br>') || '—';
}
function partnerPayoutModal(p){
  const wallets=(p.wallets||[]).filter(w=>w.balance_minor>0);
  if(!wallets.length){toast('Нет доступного баланса для выплаты','info');return;}
  openModal({title:'Выплата партнёру',sub:esc(p.user),icon:'dollar',size:'sm',
    body:`<div class="notice">Сначала переведите деньги партнёру своим способом, затем запишите выплату здесь. Панель ведёт учёт и не отправляет перевод.</div>
      <div class="field"><label for="paCurrency">Валюта</label><select class="inp" id="paCurrency">${wallets.map(w=>`<option value="${esc(w.currency)}">${esc(w.currency)} · доступно ${fmtMoney(minorToUnits(w.balance_minor,w.currency),w.currency)}</option>`).join('')}</select></div>
      <div class="field"><label for="paAmount">Сумма выплаты</label><input class="inp" type="number" id="paAmount"></div>
      <div class="field"><label for="paNote">Комментарий или номер перевода</label><input class="inp" id="paNote" maxlength="1000"></div>
      <div class="hint">Возврат клиенту уменьшает партнёрский баланс. Уже выплаченная комиссия учитывается как долг в счёт будущих начислений.</div>`,
    footer:'<button class="btn" data-close>Отмена</button><div class="spacer"></div><button class="btn primary" data-save>Записать выплату</button>',
    onMount(layer,close){
      const cur=layer.querySelector('#paCurrency'), amount=layer.querySelector('#paAmount'), btn=layer.querySelector('[data-save]');
      const update=()=>{const w=wallets.find(w=>w.currency===cur.value);amount.step=ZERO_DECIMAL.has(cur.value)?'1':'0.01';amount.min=amount.step;amount.max=minorToUnits(w.balance_minor,w.currency);amount.value=amount.max;};
      cur.onchange=update;update();
      btn.onclick=async()=>{
        const n=Number(amount.value), w=wallets.find(w=>w.currency===cur.value), minor=unitsToMinor(n,cur.value);
        if(!Number.isFinite(n)||minor<=0||minor>w.balance_minor||Math.abs(minorToUnits(minor,cur.value)-n)>0.000001){toast('Проверьте сумму и доступный баланс','err');return;}
        btn.disabled=true;
        try{await API.call('/api/partners/'+p.id+'/payout',{method:'POST',body:{amount_minor:minor,currency:cur.value,note:layer.querySelector('#paNote').value.trim()||null}});close();toast('Выплата записана');await refreshDB();}
        catch(e){toast(e.message,'err');btn.disabled=false;}
      };
    }
  });
}

function tariffPriceValues(rows,currency){
  const seen=new Set(),prices=[];
  for(const row of rows){
    const d=String(row.days).trim(),a=String(row.amount).trim(),s=String(row.stars).trim();
    if(!d&&!a&&!s)continue;
    const days=Number(d);
    if(!d||!Number.isSafeInteger(days)||days<=0)throw Error('Укажите целое положительное число дней');
    if(seen.has(days))throw Error(`Период ${days} дней указан дважды`);seen.add(days);
    if(!a&&!s)throw Error(`Укажите цену для периода ${days} дней`);
    for(const [raw,cur] of [[a,currency],[s,STARS]]){
      if(raw==='')continue;const n=Number(raw),minor=unitsToMinor(n,cur);
      if(!Number.isFinite(n)||n<0||!Number.isSafeInteger(minor)||Math.abs(minorToUnits(minor,cur)-n)>0.000001)throw Error(`Проверьте цену в ${cur}: недопустимая сумма или дробная часть`);
      prices.push({period_days:days,currency:cur,amount_minor:minor});
    }
  }
  return prices;
}
function deleteTariff(t,closeParent){
  confirmModal({title:'Удалить тариф '+t.name+'?',text:'Тариф с действующими подписками будет выключен и скрыт, чтобы сохранить историю и доступ клиентов. Тариф без подписок будет удалён.',okText:'Подтвердить',onOk:async()=>{
    try{const result=await API.call('/api/tariffs/'+t.id,{method:'DELETE'});closeParent?.();toast(result.deactivated?'Тариф выключен; действующие подписки сохранены':'Тариф удалён');await refreshDB();}catch(e){toast(e.message,'err');}
  }});
}

function addonPriceRow(a){
  const traffic=a.kind==='traffic',same=!a.currency||a.currency===DB.currency;
  return `<div class="addon-price-row" data-kind="${a.kind}" ${a.id?'data-id="'+a.id+'"':''}>
    <label>Объём · ${traffic?'ГБ':'устройства'}<input class="inp num" data-addon-quantity type="number" min="1" max="${traffic?100000:100}" step="1" value="${a.quantity??''}" placeholder="${traffic?'50':'1'}"></label>
    <label>Цена · ${esc(DB.currency)}<input class="inp num" data-addon-amount inputmode="decimal" value="${same&&a.amount_minor?minorToUnits(a.amount_minor,DB.currency):''}" placeholder="Укажите цену"></label>
    <label>Telegram Stars <span class="hint">необязательно</span><input class="inp num" data-addon-stars type="number" min="1" step="1" value="${a.stars_minor??''}" placeholder="Выключено"></label>
    <button class="btn icon-only danger" data-remove-addon title="Удалить пакет" aria-label="Удалить пакет">${I('trash',14)}</button>
  </div>`;
}
function readAddonPrices(layer){
  return [...layer.querySelectorAll('.addon-price-row')].map(row=>{
    const quantity=Number(row.querySelector('[data-addon-quantity]').value), raw=row.querySelector('[data-addon-amount]').value.trim().replace(',','.');
    const stars=row.querySelector('[data-addon-stars]').value.trim();
    if(!Number.isInteger(quantity)||quantity<1||quantity>(row.dataset.kind==='traffic'?100000:100)||!raw||!Number.isFinite(Number(raw))||Number(raw)<=0||(stars&&(!Number.isInteger(Number(stars))||Number(stars)<1)))throw Error('В каждом пакете укажите целый объём и положительную цену.');
    const amount=unitsToMinor(Number(raw),DB.currency);if(amount<1)throw Error('Цена пакета слишком мала.');
    return {id:row.dataset.id?Number(row.dataset.id):null,kind:row.dataset.kind,quantity,currency:DB.currency,amount_minor:amount,stars_minor:stars?Number(stars):null,is_active:true};
  });
}
