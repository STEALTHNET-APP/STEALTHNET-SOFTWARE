/* ═══════════ PAGES: Ноды, Метрики нод, Статистика, Плагины ═══════════ */
'use strict';

/* ── НОДЫ ── */
registerPage({
  id:'nodes', title:'Ноды', group:'Инфраструктура', icon:'server', badge:()=>DB.nodes.length,
  render(){
    // Сводка сверху: состояние парка видно, не читая список.
    const on = DB.nodes.filter(n=>n.status==='online' && n.engineOk !== false);
    // Выключенная администратором нода — не авария: смешивать её с теми,
    // что не выходят на связь, значит поднимать ложную тревогу.
    const muted = DB.nodes.filter(n=>n.status==='disabled');
    // К авариям относим и те ноды, где агент на связи, а движок лежит:
    // для клиента это тот же простой.
    const off = DB.nodes.filter(n=>(n.status!=='online' && n.status!=='disabled')
                                 || (n.status==='online' && n.engineOk === false));
    const sum = (f) => DB.nodes.reduce((a,n)=>a + (f(n) || 0), 0);

    const stat = (icon, label, value, tone) => `
      <div class="card" style="padding:13px 15px;display:flex;align-items:center;gap:11px">
        <span class="stat-ic ${tone||''}">${I(icon,15)}</span>
        <div style="min-width:0">
          <div class="label" style="font-size:9.5px">${label}</div>
          <div class="num" style="font-size:16.5px;font-weight:600;line-height:1.25">${value}</div>
        </div>
      </div>`;

    return `
    <div class="page-head">
      <div><h1>Ноды</h1><div class="desc">Серверы с агентом и Xray. Нода тянет профиль конфигурации и отчитывается трафиком, онлайном и нагрузкой.</div></div>
      <div class="actions">
        <button class="btn" id="engineVer">${I('refresh',14)} Версия Xray</button>
        <button class="btn" id="allStats">${I('chart',14)} Статистика по всем</button>
        <button class="btn primary" id="newNode">${I('plus',14)} Подключить ноду</button>
      </div>
    </div>
    ${engineBanner()}

    <div class="grid g4" style="gap:10px;margin-bottom:14px">
      ${stat('users2', 'Клиентов онлайн', fmtN(sum(n=>n.online)), 't-ok')}
      ${stat('server', 'Нод онлайн', on.length, 't-cyan')}
      ${stat('alert', off.length ? 'Не выходят на связь' : 'Аварий нет',
             off.length + (muted.length ? ` <span class="sub-note">+${muted.length} выкл.</span>` : ''),
             off.length ? 'err' : 't-ok')}
      ${stat('chart', 'Трафик сегодня', fmtBytes(sum(n=>n.todayBytes)), 't-violet')}
      ${stat('download', 'Скорость приёма', fmtBps(sum(n=>n.rxBps)), 't-info')}
      ${stat('upload', 'Скорость отдачи', fmtBps(sum(n=>n.txBps)), 't-pink')}
      ${stat('layers', 'Память занята', fmtBytes(sum(n=>n.memUsed)), 't-warn')}
      ${stat('clock', 'Аптайм (макс.)', fmtUptime(Math.max(0, ...DB.nodes.map(n=>n.uptimeSec||0))), 't-cyan')}
    </div>

    <div class="node-list">
      ${DB.nodes.map(n=>nodeRow(n)).join('') || `<div class="card empty" style="padding:40px">${I('server',32)}<b>Нод пока нет</b><span>Подключите первую — панель выдаст готовую команду установки</span></div>`}
    </div>`;
  },
  bind(root){
    root.querySelector('#newNode').addEventListener('click', ()=>createNodeStepper());
    root.querySelector('#allStats').addEventListener('click', ()=>allNodesStatsModal());
    root.querySelector('#engineVer').addEventListener('click', ()=>engineVersionModal());
    root.querySelectorAll('[data-nrow]').forEach(el=>el.addEventListener('click', e=>{
      if (e.target.closest('[data-nmenu]')) return;
      editNodeDrawer(DB.nodes.find(n=>n.id===el.dataset.nrow));
    }));
    root.querySelectorAll('[data-nmenu]').forEach(b=>b.addEventListener('click', e=>{
      e.stopPropagation();
      nodeMenu(e.currentTarget, DB.nodes.find(x=>x.id===e.currentTarget.dataset.nmenu));
    }));
  }
});

/* Искра трафика за сутки.

   Столбики по часам: провал или всплеск видно, не открывая графики.
   Рисуем через SVG, а не набором div — так она остаётся чёткой при
   любом масштабе страницы. */
function искра(ряд){
  if (!ряд || ряд.length < 2) return '';
  const макс = Math.max(...ряд, 1);
  const ш = 3, зазор = 1, h = 18;
  const столбики = ряд.slice(-24).map((v, i) => {
    // Минимум в пиксель: нулевой час должен быть виден как нулевой,
    // а не как отсутствующий.
    const высота = Math.max(1, Math.round((v / макс) * h));
    return `<rect x="${i * (ш + зазор)}" y="${h - высота}" width="${ш}" height="${высота}" rx="1"/>`;
  }).join('');
  const ширина = Math.min(24, ряд.length) * (ш + зазор);
  return `<svg class="nr-spark" viewBox="0 0 ${ширина} ${h}" width="${ширина}" height="${h}"
    aria-hidden="true"><g>${столбики}</g></svg>`;
}

/* Строка ноды.

   Плотная, в две линии, а не карточка: на парке в полсотни серверов
   карточки заставляют листать, а нужно одним взглядом найти ту, что
   перегружена или отвалилась. */
function nodeRow(n){
  // Три разных состояния, и путать их нельзя: «выключена админом» —
  // это норма, а «не выходит на связь» — авария, к которой надо идти.
  // «На связи» и «работает» — разные вещи. Агент может исправно
  // отвечать, пока движок на той же машине лежит: клиенты при этом не
  // подключаются ни к одной локации. Такая нода должна гореть красным,
  // иначе о простое узнают от клиентов.
  const engineDown = n.status === 'online' && n.engineOk === false;
  const state = engineDown ? 'off'
              : n.status === 'online' ? 'on'
              : n.status === 'disabled' ? 'muted'
              : 'off';
  const title = engineDown
    ? 'движок не работает: ' + (n.engineError || 'причина не сообщена')
    : { on:'агент на связи', off:'нет связи с агентом', muted:'выключена администратором' }[state];
  const icon  = { on:'pulse', off:'alert', muted:'ban' }[state];
  const cpu = n.cpu == null ? null : Math.round(n.cpu);
  const la = (v) => v == null ? '—' : v.toFixed(2);
  // 75% — уже повод посмотреть, 90% — повод действовать.
  const tone = cpu == null ? '' : cpu > 90 ? 'err' : cpu > 75 ? 'warn' : '';

  return `
  <div class="node-row is-${state}" data-nrow="${n.id}">
    <div class="nr-head">
      <span class="nr-state ${state}" title="${title}">${I(icon,13)}</span>
      <span class="chip nr-online">${I('users2',11)} ${fmtN(n.online)}</span>
      ${ccChip(n.cc)}
      <b class="mono nr-name">${esc(n.name)}</b>
      <span class="chip">${esc(n.profile)}</span>
      ${providerBadge(n.infraProvider, n.infraProviderLogo)}
      <span class="mono nr-addr">${esc(n.addr)}</span>
      ${engineDown ? `<span class="bdg err" title="${esc(n.engineError || '')}">${I('alert',11)} движок не запустился</span>` : ''}
      <div class="nr-traffic">
        <span class="num">${fmtBytes(n.todayBytes)}</span>
        <span class="sub-note">за сегодня</span>
      </div>
      ${искра(n.spark)}
      <div class="nr-meta">
        <span title="последняя связь">${I('clock',10)} ${fmtAgo(n.lastSeen)}</span>
        <span title="версия движка и агента">${esc(n.xray)} · ${esc(n.agent)}</span>
      </div>
      <button class="btn ghost icon-only" data-nmenu="${n.id}">${I('more',14)}</button>
    </div>
    <div class="nr-sub">
      <span class="nr-cpu">
        <span class="label">CPU</span>
        <div class="progress mini"><i style="width:${Math.min(100, cpu || 0)}%" class="${tone}"></i></div>
        <span class="num">${cpu == null ? '—' : cpu + '%'}</span>
      </span>
      ${/* Раньше здесь подряд шли шесть чисел без единой подписи: «54%
           0.54 2.85 0.36 3.16 GiB / 8.00 GiB» — что из этого средняя
           загрузка, а что память, приходилось вспоминать. Подписи
           короткие и приглушённые: читается как таблица, а не как ряд. */''}
      <span class="nr-stat" title="средняя загрузка за 1 / 5 / 15 минут">
        <span class="label">LA</span><span class="num">${la(n.la[0])} ${la(n.la[1])} ${la(n.la[2])}</span>
      </span>
      <span class="nr-cpu" title="занято ${fmtBytes(n.memUsed)} из ${fmtBytes(n.memTotal)}">
        <span class="label">RAM</span>
        <div class="progress mini"><i style="width:${Math.min(100, Math.round(n.ram || 0))}%"
          class="${n.ram > 90 ? 'err' : n.ram > 75 ? 'warn' : ''}"></i></div>
        <span class="num">${n.ram == null ? '—' : Math.round(n.ram) + '%'}</span>
      </span>
      <span class="nr-stat" title="скорость приёма">
        <span class="label">${I('download',10)}</span><span class="num nr-dl">${fmtBps(n.rxBps)}</span>
      </span>
      <span class="nr-stat" title="скорость отдачи">
        <span class="label">${I('upload',10)}</span><span class="num nr-ul">${fmtBps(n.txBps)}</span>
      </span>
      ${n.multiplier && n.multiplier !== 1
        ? `<span class="nr-stat" title="множитель трафика: списывается с клиента во столько раз больше">
             <span class="label">×</span><span class="num">${n.multiplier}</span></span>`
        : ''}
      <span style="flex:1"></span>
      ${n.inbounds.map(i=>`<span class="chip">${esc(i)}</span>`).join('')}
    </div>
  </div>`;
}

/* Меню ноды — тот же набор, что в «Действиях» редактора: два разных
   набора для одного объекта заставляют помнить, где что лежит. */
function nodeMenu(anchor, n){
  menu(anchor, [
    {label:'Настройки ноды', icon:'settings', onClick:()=>editNodeDrawer(n)},
    {label:'Статистика трафика', icon:'chart', onClick:()=>nodeStatsModal(n)},
    {label:'Проверить сеть · bgp.tools', icon:'globe', onClick:()=>openNodeOverview(n,'network')},
    {label:'Активные сессии', icon:'pulse', onClick:()=>nodeSessionsDrawer(n)},
    {label:'Связанные хосты', icon:'host', onClick:()=>nodeLinkedHostsDrawer(n)},
    {label:'Инбаунды и хосты', icon:'layers', onClick:()=>nodeInboundsDrawer(n)},
    {label:'Сменить профиль', icon:'json', onClick:()=>nodeProfileDrawer(n)},
    '-',
    {label:'Перезапустить Xray', icon:'refresh', onClick:()=>restartNodeEngine(n)},
    {label:'Перевыпустить секрет', icon:'key', onClick:()=>rotateNodeSecret(n)},
    {label:'Сбросить трафик ноды', icon:'refresh', onClick:()=>resetNodeTraffic(n)},
    {label:n.status==='disabled'?'Включить ноду':'Отключить ноду',
      icon:n.status==='disabled'?'power':'ban', onClick:()=>toggleNode(n)},
    '-',
    {label:'Удалить', icon:'trash', danger:true, onClick:()=>deleteNode(n)},
  ]);
}

/* Перевыпуск секрета: старый перестаёт действовать сразу, поэтому
   спрашиваем и сразу отдаём готовые команды установки. */
function rotateNodeSecret(n){
  confirmModal({
    title:'Перевыпустить секрет?',
    text:`Агент на <b>${esc(n.name)}</b> потеряет доступ, пока вы не пропишете новый секрет. Нода уйдёт в статус «подключается».`,
    okText:'Перевыпустить',
    onOk:async ()=>{
      try {
        const r = await API.call('/api/nodes/'+n.id+'/rotate-secret', { method:'POST' });
        nodeInstallModal(r.install);
        await refreshDB();
      } catch(e){ toast('Не получилось: '+e.message, 'err'); }
    }
  });
}

/* создание ноды: степпер */
function createNodeStepper(){
  let step = 1;
  const total = 3;
  const form = { name:'', country_code:'NL', address:'', api_port:2222,
                 profile_id: null,
                 inbound_tags: null, traffic_multiplier:1.0,
                 count_traffic:true, notify:true };

  const stepsHtml = () => `
    <div class="stepper">
      ${[['1','Сервер'],['2','Конфигурация'],['3','Параметры']].map(([n,t],i)=>`
        ${i?'<div class="step-line"></div>':''}
        <div class="step ${step==i+1?'on':step>i+1?'done':''}"><span class="s-num">${step>i+1?'✓':n}</span>${t}</div>`).join('')}
    </div>`;

  const profileInbounds = () => {
    const p = DB.profiles.find(x=>+x.id===form.profile_id);
    return p ? p.inbounds : [];
  };

  const bodies = () => ({
    1:`
      <div class="two-col">
        <div class="field"><label>Название <span class="req">*</span></label>
          <input class="inp mono" id="fName" value="${esc(form.name)}" placeholder="waw-edge-01">
          <div class="hint">Внутреннее имя, клиенты его не увидят.</div></div>
        <div class="field"><label>Страна <span class="req">*</span></label>
          ${countryField('fCC', form.country_code)}
          <div class="hint">Флаг локации у клиента. Ищите по названию или коду.</div></div>
        <div class="field"><label>Адрес сервера <span class="req">*</span></label>
          <input class="inp mono" id="fAddr" value="${esc(form.address)}" placeholder="185.10.20.30">
          <div class="hint">IP или домен, куда будут подключаться клиенты.</div></div>
        <p class="hint">Агент подключится к панели сам. Открывать для него входящий порт на сервере не нужно.</p>
      </div>`,
    2:`
      <div class="field"><label>Профиль конфигурации</label>
        <label class="check node-profile-choice">
          <input type="radio" name="cnProf" value="" ${form.profile_id===null?'checked':''}>
          <span><b>Без профиля</b><small>Подключить сервер сейчас, выбрать конфигурацию позже. Подходит для тестовой ноды.</small></span>
        </label>
        ${DB.profiles.length ? `<input class="inp" id="cnProfileSearch" aria-label="Поиск профиля" placeholder="Найти профиль…"><div class="node-setup-profiles">`+DB.profiles.map(p=>`
          <label class="check" data-cn-profile="${esc(p.name.toLowerCase())}" style="padding:11px 13px;border:1px solid ${+p.id===form.profile_id?'color-mix(in srgb, var(--accent) 40%, transparent)':'var(--border)'};border-radius:10px;margin-bottom:8px;background:${+p.id===form.profile_id?'var(--accent-dim)':'transparent'}">
            <input type="radio" name="cnProf" value="${p.id}" ${+p.id===form.profile_id?'checked':''} style="appearance:auto;accent-color:var(--accent)">
            <span style="flex:1"><b class="mono" style="font-size:12.5px;display:block">${esc(p.name)}</b>
            <span style="font-size:11px;color:var(--text-3)">${p.inbounds.join(' · ')||'нет инбаундов'}</span></span>
          </label>`).join('')+'</div>'
          : `<div class="empty">${I('json',32)}<b>Нет профилей</b><span>Можно подключить ноду без профиля и настроить VPN позже.</span></div>`}
      </div>
      <div class="field" ${form.profile_id===null?'hidden':''}><label>Инбаунды на этой ноде</label>
        <div class="chips-select" id="fInb">${profileInbounds().map(t=>`
          <button type="button" class="chip-opt ${form.inbound_tags===null||form.inbound_tags.includes(t)?'on':''}" aria-pressed="${form.inbound_tags===null||form.inbound_tags.includes(t)}" data-tag="${esc(t)}">${esc(t)}</button>`).join('')||'<span class="sub-note">выберите профиль</span>'}</div>
        <div class="hint">Можно поднять не все: например, без Hysteria2 на слабом сервере.
          Если не выбрать ни одного, клиенты эту ноду не увидят.</div></div>`,
    3:`
      <div class="two-col">
        <div class="field"><label>Множитель потребления</label>
          <div class="slider-row"><input type="range" min="0" max="30" value="${form.traffic_multiplier*10}" id="fMul" aria-label="Множитель потребления">
            <span class="num" style="width:34px" id="fMulV">×${form.traffic_multiplier.toFixed(1)}</span></div>
          <div class="hint">Дорогая локация? ×2 спишет с клиента вдвое больше трафика.</div></div>
        <div class="field"><label>&nbsp;</label>
          <div class="switch-row"><label class="switch"><input type="checkbox" id="fCount" aria-label="Учитывать трафик" ${form.count_traffic?'checked':''}><span class="tr"></span></label>
            <div class="sw-txt"><b>Учитывать трафик</b><span>Списывать с лимитов клиентов</span></div></div>
          <div class="switch-row"><label class="switch"><input type="checkbox" id="fNotify" aria-label="Уведомления о ноде" ${form.notify?'checked':''}><span class="tr"></span></label>
            <div class="sw-txt"><b>Уведомления</b><span>Алерты при offline</span></div></div>
        </div>
      </div>`,
  });

  const collect = (layer)=>{
    const g = id => layer.querySelector('#'+id);
    if(step===1){
      form.name = g('fName').value.trim();
      form.country_code = g('fCC').value.trim().toUpperCase();
      form.address = g('fAddr').value.trim();
    }
    if(step===2){
      const sel = layer.querySelector('input[name=cnProf]:checked');
      if(sel) form.profile_id = sel.value ? Number(sel.value) : null;
      form.inbound_tags = [...layer.querySelectorAll('#fInb .chip-opt.on')].map(c=>c.dataset.tag);
    }
    if(step===3){
      form.traffic_multiplier = +g('fMul').value / 10;
      form.count_traffic = g('fCount').checked;
      form.notify = g('fNotify').checked;
    }
  };

  const validate = ()=>{
    if(step===1){
      if(!form.name) return 'Укажите название';
      if(form.country_code.length!==2) return 'Выберите страну из списка';
      if(!form.address) return 'Укажите адрес сервера';
    }
    if(step===2 && form.profile_id!==null){
      if(!form.inbound_tags.length) return 'Выберите хотя бы один инбаунд — иначе ноду никто не увидит';
    }
    return null;
  };

  openModal({
    title:'Подключить ноду', sub:'Сервер → конфигурация → параметры', icon:'server', size:'lg',
    body:`<div id="cnWrap">${stepsHtml()}<div id="cnBody">${bodies()[1]}</div></div>`,
    footer:`<button class="btn" id="cnBack" style="visibility:hidden">Назад</button>
            <div class="spacer"></div><button class="btn" data-close>Отмена</button>
            <button class="btn primary" id="cnNext">Далее ${I('chevR',13)}</button>`,
    onMount(layer, close){
      const redraw = ()=>{
        layer.querySelector('#cnWrap').innerHTML = stepsHtml() + `<div id="cnBody">${bodies()[step]}</div>`;
        layer.querySelector('#cnBack').style.visibility = step>1?'visible':'hidden';
        layer.querySelector('#cnNext').innerHTML = step<total ? `Далее ${I('chevR',13)}` : `${I('check',13)} Создать ноду`;

        // Выбор страны живёт только на первом шаге, но redraw пересоздаёт
        // разметку целиком — оживляем заново после каждой перерисовки.
        wireCountryField(layer, 'fCC', cc => { form.country_code = cc; });
        const search=layer.querySelector('#cnProfileSearch');
        if(search)search.addEventListener('input',()=>layer.querySelectorAll('[data-cn-profile]').forEach(row=>row.hidden=!row.dataset.cnProfile.includes(search.value.trim().toLowerCase())));
        const r = layer.querySelector('#fMul');
        if(r) r.addEventListener('input', ()=>layer.querySelector('#fMulV').textContent = '×'+(r.value/10).toFixed(1));
        layer.querySelectorAll('#fInb .chip-opt').forEach(c=>c.addEventListener('click', ()=>{c.classList.toggle('on');c.setAttribute('aria-pressed',c.classList.contains('on'));}));
        // Смена профиля меняет набор инбаундов — перерисовываем сразу,
        // иначе можно выбрать инбаунд от другого профиля.
        layer.querySelectorAll('input[name=cnProf]').forEach(el=>el.addEventListener('change', ()=>{
          form.profile_id = el.value ? Number(el.value) : null; form.inbound_tags = null; redraw();
        }));
      };

      layer.querySelector('#cnNext').addEventListener('click', async ()=>{
        collect(layer);
        const err = validate();
        if(err) return toast(err, 'err');

        if(step < total){ step++; redraw(); return; }

        const btn = layer.querySelector('#cnNext');
        btn.disabled = true; btn.textContent = 'Создаём…';
        try{
          const res = await API.call('/api/nodes', { method:'POST', body: form });
          close();
          await refreshDB();
          nodeInstallModal(res.install);
        }catch(e){
          toast('Не создалась: '+e.message, 'err');
          btn.disabled = false; btn.innerHTML = I('check',13)+' Создать ноду';
        }
      });
      layer.querySelector('#cnBack').addEventListener('click', ()=>{ collect(layer); if(step>1){ step--; redraw(); } });
      redraw();
    }
  });
}

/// Установочные материалы после создания ноды.
///
/// Секрет виден только здесь: в базе лежит лишь его хэш. Поэтому окно
/// закрывается осознанным нажатием, а не кликом мимо.
function nodeInstallModal(inst){
  // Адрес для строки входа. Если его почему-то нет — не выдумываем, а
  // ставим заметную заглушку: неверный адрес в готовой команде хуже,
  // чем явное «подставьте свой».
  const ssh_host = (inst.address || '').trim() || 'АДРЕС_СЕРВЕРА';
  openModal({
    title:'Нода создана', sub:esc(inst.name)+' · осталось запустить агент', icon:'server', size:'lg',
    body:`
      <div class="notice" style="background:color-mix(in srgb, var(--warn) 10%, transparent);border:1px solid color-mix(in srgb, var(--warn) 30%, transparent);color:var(--warn-ink);border-radius:10px;padding:11px 14px;font-size:12.5px;margin-bottom:16px">
        ${I('alert',13)} Секрет показывается один раз — в базе хранится только его хэш.
        Скопируйте сейчас; потерялся — перевыпустите.
      </div>

      <div class="tabs" id="instTabs">
        <button class="on" data-t="script">Обычный сервер</button>
        <button data-t="compose">Docker</button>
        <button data-t="systemd">Свой бинарь</button>
      </div>

      <div class="tab-pane on" data-p="script">
        <p class="sub-note" style="margin-bottom:12px">Подходит для чистого Debian или Ubuntu: скрипт сам
          поставит Xray <b>${esc(inst.engine_version||'из настроек панели')}</b>, агента и автозапуск.
          Нужны root, systemd и доступ к панели и GitHub по HTTPS. Откройте на сервере TCP/UDP-порты выбранных инбаундов; входящий порт агента открывать не нужно.</p>

        <ol class="steps">
          <li>
            <div class="steps-t">Зайдите на сервер ноды по SSH</div>
            <div class="code wrap"><pre>ssh root@${esc(ssh_host)}</pre></div>
            <button class="btn sm" style="margin-top:8px" data-copy="ssh root@${esc(ssh_host)}"
                    data-copy-msg="Команда входа скопирована">${I('copy',12)} Копировать</button>
          </li>
          <li>
            <div class="steps-t">Вставьте туда эту команду целиком и нажмите Enter</div>
            <div class="code wrap"><pre>${esc(inst.one_liner)}</pre></div>
            <button class="btn primary sm" style="margin-top:8px" data-copy="${esc(inst.one_liner)}"
                    data-copy-msg="Команда установки скопирована">${I('copy',12)} Копировать команду</button>
          </li>
          <li>
            <div class="steps-t">Проверьте завершение установки</div>
            <div class="sub-note">Скрипт проверит ключ, загрузит проверенные файлы и дождётся ответа панели.
              «Готово» появится после запуска Xray. Затем проверьте подключение тестового клиента.</div>
          </li>
        </ol>

        <div class="hint" style="margin-top:14px">${I('info',12)}
          Установка одинаковая для всех нод. Reality, который стоит в заготовках по умолчанию,
          сертификата не требует. Планируете Trojan или Hysteria2 — выпустите сертификат
          на домен этой ноды сами и укажите пути в профиле.</div>
      </div>

      <div class="tab-pane" data-p="compose">
        ${inst.compose?`<p class="sub-note">Сохраните как <code>docker-compose.yml</code> и выполните <code>docker compose up -d</code>. Образ задан владельцем панели и должен содержать Xray и агент.</p>
        <div class="code"><pre>${esc(inst.compose)}</pre></div>
        <button class="btn primary sm" data-copy="${esc(inst.compose)}">${I('copy',12)} Копировать compose</button>`:`<div class="notice">Docker-образ в этой панели ещё не настроен.</div><p class="sub-note">Используйте вкладку «Обычный сервер». Для Docker владелец панели должен указать доступный образ агента с Xray в NODE_IMAGE.</p>`}
      </div>

      <div class="tab-pane" data-p="systemd">
        <p class="sub-note" style="margin-bottom:10px">Для тех, кто собрал агента сам и положил его
          в <code>/usr/local/bin/sn-node</code>. Юнит кладётся в
          <code>/etc/systemd/system/sn-node.service</code> с правами 644. Создайте каталог <code>/etc/sn-node</code> и файл <code>/etc/sn-node/node.env</code> с правами 600: в нём хранится секрет ноды.
          Отдельно установите Xray и гео-базы в <code>/usr/local/share/xray</code>, затем выполните <code>systemctl daemon-reload &amp;&amp; systemctl enable --now sn-node</code>.</p>
        <p class="sub-note"><strong>/etc/sn-node/node.env · права 600</strong></p>
        <div class="code"><pre>${esc(inst.env)}</pre></div>
        <button class="btn sm" style="margin:10px 0 16px" data-copy="${esc(inst.env)}" data-copy-msg="Настройки скопированы">${I('copy',12)} Копировать настройки</button>
        <p class="sub-note"><strong>/etc/systemd/system/sn-node.service · права 644</strong></p>
        <div class="code" style="max-height:34vh"><pre>${esc(inst.systemd)}</pre></div>
        <button class="btn primary sm" style="margin-top:10px" data-copy="${esc(inst.systemd)}"
                data-copy-msg="Юнит скопирован">${I('copy',12)} Копировать юнит</button>
      </div>

      <div class="hint" style="margin-top:14px">${I('info',12)}
        После установки откройте карточку ноды: агент должен быть на связи, а Xray — работать.
        Во вкладке «Сеть и BGP» можно проверить IP, ASN и оператора через bgp.tools.
        Если профиль требует TLS, сертификаты должны существовать по путям из профиля.</div>`,
    footer:`<button class="btn" data-bgp-install>${I('globe',14)} Проверить сеть</button><div class="spacer"></div><button class="btn primary" data-close>Я скопировал</button>`,
    onMount(layer){
      layer.querySelector('[data-bgp-install]').onclick=async e=>{const b=e.currentTarget;b.disabled=true;try{await refreshDB();const n=DB.nodes.find(n=>String(n.id)===String(inst.node_id));if(!n)throw new Error('Нода не найдена в панели');openNodeOverview(n,'network');}catch(e){toast(e.message,'err');}finally{b.disabled=false;}};
      const tabs = layer.querySelector('#instTabs');
      tabs.querySelectorAll('button').forEach(b=>b.addEventListener('click', ()=>{
        tabs.querySelectorAll('button').forEach(x=>x.classList.remove('on')); b.classList.add('on');
        layer.querySelectorAll('.tab-pane').forEach(p=>p.classList.toggle('on', p.dataset.p===b.dataset.t));
      }));
      // Копирование здесь своего обработчика не заводит: им занимается
      // общий делегат в core.js. Раньше их было два, и делегат копировал
      // значение атрибута буквально — в буфер попадало слово «one_liner».
    }
  });
}

/* редактор ноды: аккордеон */
function editNodeDrawer(n){
  openDrawer({
    title:'Нода · '+n.name, sub:`${esc(n.addr)}:${n.port} · ${esc(n.profile)}`, icon:'server', size:'lg',
    body:`
      <div class="form-section" style="margin-top:0"><h4>${I('cpu',13)} Система</h4>
        <div class="sys-grid">
          <div class="sys-cell"><div class="label">Процессор</div>
            <div class="v mono">${esc(n.cpuModel || '—')}${n.cpuCores?` · ${n.cpuCores} ядер`:''}</div></div>
          <div class="sys-cell"><div class="label">Память</div>
            <div class="v">${fmtBytes(n.memUsed)} / ${fmtBytes(n.memTotal)}${
              n.memTotal ? ` <span class="sub-note">(${Math.round(n.memUsed/n.memTotal*100)}%)</span>` : ''}</div></div>
          <div class="sys-cell"><div class="label">Ядро</div><div class="v mono">${esc(n.kernel || '—')}</div></div>
          <div class="sys-cell"><div class="label">Аптайм</div><div class="v">${fmtUptime(n.uptimeSec)}</div></div>
          <div class="sys-cell"><div class="label">Интерфейс ${esc(n.iface || '')}</div>
            <div class="v">${I('download',11)} ${fmtBps(n.rxBps)} &nbsp; ${I('upload',11)} ${fmtBps(n.txBps)}</div>
            <div class="sub-note" style="margin-top:3px">всего ${fmtBytes(n.rxTotal)} / ${fmtBytes(n.txTotal)}</div></div>
          <div class="sys-cell"><div class="label">Загрузка</div>
            <div class="v">${n.cpu == null ? '—' : Math.round(n.cpu)+'%'}
              <span class="sub-note">LA ${n.la.map(v=>v==null?'—':v.toFixed(2)).join(' ')}</span></div></div>
        </div>
        <div class="hint" style="margin-top:8px">${I('info',12)} Данные приходят от агента каждые 15 секунд. Последняя связь: ${fmtAgo(n.lastSeen)} назад.</div>
      </div>
      <div class="accordion">
        <div class="acc-item open">
          <div class="acc-head"><span class="a-ic">${I('settings',14)}</span> Основные ${I('chevD',14).replace('<svg','<svg class="chev"')}</div>
          <div class="acc-body">
            <div class="two-col" style="padding-top:12px">
              <div class="field"><label>Название</label><input class="inp mono" id="enName" value="${esc(n.name)}"></div>
              <div class="field"><label>Страна</label>
                ${countryField('enCC', n.cc)}
                <div class="hint">Флаг локации у клиента. Ищите по названию или коду.</div></div>
              <div class="field"><label>Адрес</label><input class="inp mono" id="enAddr" value="${esc(n.addr)}"></div>
              <div class="field"><label>Порт API</label><input class="inp num" id="enPort" value="${n.port}"></div>
            </div>
          </div>
        </div>
        <div class="acc-item">
          <div class="acc-head"><span class="a-ic">${I('json',14)}</span> Конфигурация <span class="chip accent" style="margin-left:4px">${n.profile}</span> ${I('chevD',14).replace('<svg','<svg class="chev"')}</div>
          <div class="acc-body">
            <div style="padding-top:12px">
              <button class="btn sm" data-chprof>${I('json',12)} Сменить профиль</button>
              <div class="field" style="margin-top:12px"><label>Активные инбаунды</label>
                <div class="chips-select">${(DB.profiles.find(p=>p.name===n.profile)?.inbounds||[]).map(i=>`<span class="chip-opt ${n.inbounds.includes(i)?'on':''}" data-inb="${esc(i)}">${esc(i)}</span>`).join('')}</div></div>
            </div>
          </div>
        </div>
        <div class="acc-item">
          <div class="acc-head"><span class="a-ic">${I('activity',14)}</span> Трафик и лимиты ${I('chevD',14).replace('<svg','<svg class="chev"')}</div>
          <div class="acc-body">
            <div style="padding-top:12px">
              <div class="field"><label>Множитель потребления</label>
                <div class="slider-row"><input type="range" min="0" max="30" value="${n.multiplier*10}" id="emR"><span class="num" style="width:34px" id="emV">×${n.multiplier.toFixed(1)}</span></div></div>
              <div class="switch-row"><label class="switch"><input type="checkbox" id="enTrack" ${n.trackTraffic?'checked':''}><span class="tr"></span></label>
                <div class="sw-txt"><b>Учитывать трафик</b><span>Списывать с лимитов клиентов</span></div></div>
              <div class="switch-row"><label class="switch"><input type="checkbox" id="enNotify" ${n.notify?'checked':''}><span class="tr"></span></label>
                <div class="sw-txt"><b>Уведомления</b><span>Алерты offline / high-load в Telegram</span></div></div>
            </div>
          </div>
        </div>
        <div class="acc-item">
          <div class="acc-head"><span class="a-ic">${I('dollar',14)}</span> Биллинг ${I('chevD',14).replace('<svg','<svg class="chev"')}</div>
          <div class="acc-body">
            <div class="two-col" style="padding-top:12px">
              <div class="field"><label>Профиль конфигурации</label>
                <input class="inp mono" value="${esc(n.profile)}" disabled>
                <div class="hint">Меняется отдельной кнопкой ниже — смена профиля перезапускает движок.</div></div>
              <div class="field"><label>Стоимость аренды</label>
                <div class="inp-group"><input class="inp num" id="enCost" type="number" step="0.01"
                  value="${n.costMinor ? minorToUnits(n.costMinor, DB.currency) : ''}" placeholder="0">
                  <span class="suffix">${esc(DB.currency)}/мес</span></div></div>
              <div class="field"><label>День списания</label>
                <input class="inp num" id="enBillDay" type="number" min="1" max="31" value="${n.billDay || ''}" placeholder="5">
                <div class="hint">Число месяца, когда хостер снимает деньги. Видно в «Инфра-биллинге».</div></div>
            </div>
          </div>
        </div>
      </div>`,
    footer:`<button class="btn sm" data-actions>${I('more',12)} Действия</button><div class="spacer"></div>
            <button class="btn" data-close>Отмена</button><button class="btn primary" data-save>Сохранить</button>`,
    onMount(layer, close){
      layer.querySelectorAll('.acc-head').forEach(h=>h.addEventListener('click', ()=>h.parentElement.classList.toggle('open')));
      const r = layer.querySelector('#emR');
      r.addEventListener('input', ()=>layer.querySelector('#emV').textContent = '×'+(r.value/10).toFixed(1));
      layer.querySelectorAll('.chip-opt').forEach(c=>c.addEventListener('click', ()=>c.classList.toggle('on')));
      // После смены профиля карточку открываем заново на свежих данных:
      // инбаунды принадлежат профилю, и старый список тут уже неверен.
      layer.querySelector('[data-chprof]').addEventListener('click', ()=>
        nodeProfileDrawer(n, fresh => { close(); editNodeDrawer(fresh); }));
      wireCountryField(layer, 'enCC');

      const btn = layer.querySelector('[data-save]');
      btn.addEventListener('click', async ()=>{
        const body = {
          name: layer.querySelector('#enName').value.trim(),
          country_code: layer.querySelector('#enCC').value.trim().toUpperCase(),
          address: layer.querySelector('#enAddr').value.trim(),
          api_port: parseInt(layer.querySelector('#enPort').value, 10) || 8080,
          inbound_tags: [...layer.querySelectorAll('[data-inb]')].filter(c=>c.classList.contains('on')).map(c=>c.dataset.inb),
          traffic_multiplier: parseInt(layer.querySelector('#emR').value, 10) / 10,
          count_traffic: layer.querySelector('#enTrack').checked,
          notify: layer.querySelector('#enNotify').checked,
        };

        // Расходы: пустая стоимость — это ноль, а не «не менять», иначе
        // стереть ошибочно введённую сумму было бы нечем.
        const cost = layer.querySelector('#enCost').value.trim();
        body.monthly_cost_minor = cost === '' ? 0 : unitsToMinor(parseFloat(cost), DB.currency);
        const day = layer.querySelector('#enBillDay').value.trim();
        body.bill_day = day === '' ? null : parseInt(day, 10);
        if (!body.name || !body.address) { toast('Нужны название и адрес', 'err'); return; }

        btn.disabled = true;
        try {
          await API.call('/api/nodes/'+n.id, { method:'PATCH', body });
          close();
          toast('Нода сохранена — агент заберёт конфиг в течение 15 секунд');
          await refreshDB();
        } catch(e){ toast('Не сохранилось: '+e.message, 'err'); btn.disabled = false; }
      });
      // «Действия» — тот же набор, что в меню строки: разные наборы
      // в двух местах заставляют помнить, где что лежит.
      layer.querySelector('[data-actions]').addEventListener('click', e=>{
        menu(e.currentTarget, [
          {label:'Скопировать адрес', icon:'copy', onClick:()=>copyText(n.addr+':'+n.port, 'Адрес скопирован')},
          {label:'Перевыпустить секрет', icon:'key', onClick:()=>{ close(); rotateNodeSecret(n); }},
          {label:'Сбросить трафик ноды', icon:'refresh', onClick:()=>{ close(); resetNodeTraffic(n); }},
          {label:n.status==='disabled'?'Включить ноду':'Отключить ноду',
            icon:n.status==='disabled'?'power':'ban', onClick:()=>{ close(); toggleNode(n); }},
          '-',
          {label:'Удалить', icon:'trash', danger:true, onClick:()=>{ close(); deleteNode(n); }},
        ]);
      });
    }
  });
}
/// Смена профиля ноды.
///
/// `after` вызывается после успешной смены: набор инбаундов принадлежит
/// профилю, поэтому редактор, из которого сюда пришли, надо перерисовать.
/// Раньше он оставался со списком от прежнего профиля, и приходилось
/// закрывать карточку и открывать заново.
function nodeProfileDrawer(n, after){
  openDrawer({
    title:'Профиль для '+n.name, sub:'Нода получит новый конфиг и перезапустит Xray', icon:'json',
    body: `<label class="check node-profile-choice">
        <input type="radio" name="npf" value="" ${!n.profileId?'checked':''}>
        <span><b>Без профиля</b><small>После синхронизации VPN-подключения на этой ноде закроются. Агент останется на связи, ноду можно будет использовать для тестов.</small></span>
      </label>` + DB.profiles.map(p=>`
      <label class="check node-profile-choice" data-pf="${p.id}">
        <input type="radio" name="npf" value="${p.id}" ${String(p.id)===String(n.profileId)?'checked':''}>
        <span><b>${esc(p.name)}</b><small>${p.inbounds.map(esc).join(' · ')}</small></span>
      </label>`).join(''),
    footer:`<div class="spacer"></div><button class="btn" data-close>Отмена</button><button class="btn primary" data-save>Применить</button>`,
    onMount(l, close){
      // Раньше кнопка показывала «Профиль применён», не отправляя запроса:
      // нода продолжала работать на старом профиле, а в панели значился новый.
      l.querySelector('[data-save]').addEventListener('click', async (e)=>{
        const picked = l.querySelector('input[name="npf"]:checked');
        if (!picked) { toast('Выберите профиль конфигурации','err'); return; }
        const btn = e.currentTarget; btn.disabled = true;
        try {
          await API.call('/api/nodes/'+n.id, { method:'PATCH', body:{ profile_id: picked.value ? Number(picked.value) : null } });
          close();
          toast(picked.value ? 'Профиль назначен — нода заберёт конфиг в течение 15 секунд' : 'Профиль снят — ожидаем синхронизации ноды');
          await refreshDB();
          // Данные ноды перечитываем из обновлённого DB: у объекта, с
          // которым открывали карточку, и профиль, и инбаунды прежние.
          const fresh = DB.nodes.find(x => x.id === n.id);
          if (after && fresh) after(fresh);
        } catch(err){ btn.disabled = false; toast('Не применился: '+err.message,'err'); }
      });
    }
  });
}
/* Столбики из реального ряда. Отдельная функция, потому что рисуем это
   в трёх местах, а генератор случайных чисел выкинут насовсем. */
function realBars(points, label){
  if (!points.length) {
    return `<div class="empty" style="padding:26px">${I('chart',28)}<b>Данных нет</b>
              <span>За выбранный период счётчики ничего не зафиксировали.</span></div>`;
  }
  const max = Math.max(...points.map(p=>p.v), 1);
  return `
    <div style="display:flex;align-items:flex-end;gap:3px;height:140px;padding:0 2px">
      ${points.map(p=>`
        <div title="${esc(p.t)}: ${esc(p.label)}"
          style="flex:1;min-width:3px;height:${Math.max(2, p.v/max*100)}%;
                 background:var(--accent);opacity:.85;border-radius:2px 2px 0 0"></div>`).join('')}
    </div>
    <div class="sub-note" style="display:flex;justify-content:space-between;margin-top:6px">
      <span>${esc(points[0].t)}</span><span>${esc(label||'')}</span><span>${esc(points[points.length-1].t)}</span>
    </div>`;
}

function nodeStatsModal(n){
  openModal({
    title:'Статистика · '+esc(n.name), sub:'Трафик и онлайн', icon:'chart', size:'xl',
    body:`
      <div class="section-h" style="margin-top:0"><h2>Трафик по дням</h2>
        <div class="spacer"></div>
        <select class="inp" id="nsDays" style="width:130px">
          <option value="14">14 дней</option><option value="30" selected>30 дней</option><option value="90">90 дней</option>
        </select></div>
      <div id="nsTraffic" class="sub-note">Загружаем…</div>
      <div class="section-h"><h2>Онлайн и нагрузка</h2><span>последние 24 часа</span></div>
      <div id="nsMetrics" class="sub-note">Загружаем…</div>`,
    footer:`<div class="spacer"></div><button class="btn" data-close>Закрыть</button>`,
    onMount(layer){
      const drawTraffic = async () => {
        const days = layer.querySelector('#nsDays').value;
        try {
          const d = await API.call('/api/nodes/traffic?days='+days);
          const me = d.nodes.find(x=>String(x.id) === String(n.id));
          const pts = Object.entries(me?.days || {}).sort()
            .map(([day, bytes])=>({ t: day, v: bytes, label: fmtBytes(bytes) }));
          const total = pts.reduce((a,p)=>a+p.v, 0);
          layer.querySelector('#nsTraffic').innerHTML = realBars(pts, 'всего '+fmtBytes(total));
        } catch(e){ layer.querySelector('#nsTraffic').textContent = 'Не загрузилось: '+e.message; }
      };
      const drawMetrics = async () => {
        try {
          const m = await API.call('/api/nodes/'+n.id+'/metrics?hours=24');
          const pts = m.map(x=>({ t: new Date(x.at).toISOString().slice(11,16),
                                  v: x.online, label: x.online+' клиентов' }));
          const peak = pts.length ? Math.max(...pts.map(p=>p.v)) : 0;
          layer.querySelector('#nsMetrics').innerHTML = realBars(pts, 'пик '+peak);
        } catch(e){ layer.querySelector('#nsMetrics').textContent = 'Не загрузилось: '+e.message; }
      };
      layer.querySelector('#nsDays').addEventListener('change', drawTraffic);
      drawTraffic(); drawMetrics();
    }
  });
}

function allNodesStatsModal(){
  openModal({
    title:'Статистика по всем нодам', sub:'Суммарный трафик', icon:'chart', size:'xl',
    body:`<div id="anBody" class="sub-note">Загружаем…</div>`,
    footer:`<div class="spacer"></div><button class="btn" data-close>Закрыть</button>`,
    async onMount(layer){
      let d;
      try { d = await API.call('/api/nodes/traffic?days=30'); }
      catch(e){ layer.querySelector('#anBody').textContent = 'Не загрузилось: '+e.message; return; }

      // Пустой ответ — не ошибка: на свежей установке трафика ещё нет.
      // Раньше здесь падало «Cannot read properties of undefined», и окно
      // оставалось с надписью «Загружаем…» навсегда.
      const nodes = (d && Array.isArray(d.nodes)) ? d.nodes : [];
      if (!nodes.length) {
        layer.querySelector('#anBody').innerHTML =
          `<div class="empty">${I('chart',30)}<b>Данных о трафике нет</b>
             <span>Ноды ещё не присылали счётчики за выбранный период.</span></div>`;
        return;
      }

      // Суммируем по дням: интересует нагрузка парка, а не отдельной ноды.
      const byDay = {};
      nodes.forEach(n=>Object.entries(n.days||{}).forEach(([day, b])=>{ byDay[day] = (byDay[day]||0) + b; }));
      const pts = Object.entries(byDay).sort().map(([day, b])=>({ t: day, v: b, label: fmtBytes(b) }));

      const totals = nodes.map(n=>({ n, total: Object.values(n.days||{}).reduce((a,b)=>a+b, 0) }))
                            .sort((a,b)=>b.total - a.total);
      const max = Math.max(1, ...totals.map(x=>x.total));

      layer.querySelector('#anBody').innerHTML =
        realBars(pts, 'всего ' + fmtBytes(pts.reduce((a,p)=>a+p.v, 0))) + `
        <div class="info-rows" style="margin-top:16px">
          ${totals.map(({n, total})=>`
            <div class="info-row"><span class="v">${ccChip(n.country_code)}</span>
              <span class="v mono" style="width:150px">${esc(n.name)}</span>
              <div class="progress" style="flex:1"><i style="width:${total/max*100}%"></i></div>
              <span class="num" style="width:80px;text-align:right">${fmtBytes(total)}</span></div>`).join('')}
        </div>`;
    }
  });
}
function nodeSessionsDrawer(n){return activityDrawer('Активность · '+esc(n.name),{node_id:n.id});}
function nodeLinkedHostsDrawer(n){
  const hosts = DB.hosts.filter(h=>h.profile===n.profile&&n.inbounds.includes(h.inbound));
  openDrawer({
    title:'Связанные хосты · '+n.name, sub:'Хосты, чьи инбаунды активны на ноде', icon:'host',
    body: hosts.map(h=>`
      <div class="info-row">
        <span class="v" style="flex:1"><b style="font-size:12.5px">${esc(h.remark)}</b></span>
        <span class="chip">${h.inbound}</span>
        <span class="bdg ${h.enabled?'ok':'neutral'}">${h.enabled?'включён':'выключен'}</span>
      </div>`).join(''),
    footer:`<div class="spacer"></div><button class="btn" data-close>Закрыть</button>`,
  });
}
function nodeInboundsDrawer(n){
  openDrawer({
    title:'Инбаунды и хосты · '+esc(n.name), sub:'Что раздаёт нода и кто на это ссылается', icon:'layers', size:'lg',
    body: n.inbounds.map(i=>`
      <div style="border:1px solid var(--border);border-radius:10px;padding:12px 14px;margin-bottom:10px">
        <div style="display:flex;align-items:center;gap:8px"><span class="chip accent">${esc(i)}</span>
          <span class="sub-note">сквады: ${DB.squadsInt.filter(s=>squadNodeTags(s,n).includes(i)).map(s=>esc(s.name)).join(', ')||'—'}</span></div>
        <div style="margin-top:8px">${DB.hosts.filter(h=>h.profile===n.profile&&h.inbound===i).map(h=>`
          <div class="info-row" style="padding:6px 0"><span class="v" style="flex:1;font-size:12px">${esc(h.remark)}</span>
            <span class="mono" style="font-size:11px;color:var(--text-3)">${esc(h.addr)}:${h.port}</span></div>`).join('')||'<span class="sub-note">нет хостов</span>'}</div>
      </div>`).join(''),
    footer:`<div class="spacer"></div><button class="btn" data-close>Закрыть</button>`,
  });
}

/* ── МЕТРИКИ НОД ── */
registerPage({
  id:'nodes-metrics', title:'Метрики нод', group:'Инфраструктура', icon:'cpu',
  render(){
    return `
    <div class="page-head">
      <div><h1>Метрики нод</h1><div class="desc">Последний замер от агента: нагрузка, память, скорость канала. Обновляется раз в 15 секунд.</div></div>
      <div class="actions"><button class="btn" id="nmRefresh">${I('refresh',14)} Обновить</button></div>
    </div>
    <div class="tbl-wrap"><table class="tbl readonly">
      <thead><tr><th>Нода</th><th>CPU</th><th>Память</th><th>Приём</th><th>Отдача</th><th>Онлайн</th><th>Связь</th></tr></thead>
      <tbody>${DB.nodes.map(n=>{
        const off = n.status !== 'online';
        const cpu = n.cpu == null ? null : Math.round(n.cpu);
        const memPct = n.memTotal ? Math.round(n.memUsed / n.memTotal * 100) : null;
        const bar = (pct, tone) => pct == null ? '<span class="sub-note">—</span>' : `
          <div style="display:flex;align-items:center;gap:8px">
            <div class="progress" style="flex:1;min-width:70px"><i style="width:${Math.min(100,pct)}%" class="${tone}"></i></div>
            <span class="num" style="font-size:11.5px;width:38px;text-align:right">${pct}%</span></div>`;
        return `<tr style="${off?'opacity:.6':''}">
          <td><div class="cell-main">${ccChip(n.cc)}<div><b class="mono">${esc(n.name)}</b>
            <span class="sub">${off?'нет связи':'аптайм '+fmtUptime(n.uptimeSec)}</span></div></div></td>
          <td style="min-width:140px">${bar(cpu, cpu>90?'err':cpu>75?'warn':'')}</td>
          <td style="min-width:140px">${bar(memPct, memPct>90?'warn':'')}
            <span class="sub-note">${fmtBytes(n.memUsed)} / ${fmtBytes(n.memTotal)}</span></td>
          <td class="num nr-dl">${fmtBps(n.rxBps)}</td>
          <td class="num nr-ul">${fmtBps(n.txBps)}</td>
          <td class="num">${fmtN(n.online)}</td>
          <td class="sub-note">${fmtAgo(n.lastSeen)} назад</td>
        </tr>`;}).join('') || '<tr><td colspan="7" class="sub-note" style="padding:20px">Нод нет.</td></tr>'}</tbody>
    </table></div>
    <div class="sub-note" style="margin-top:10px">${I('info',12)} Историю по времени показывает «Статистика нод» — там графики строятся по сохранённым замерам.</div>`;
  },
  bind(root){
    root.querySelector('#nmRefresh').addEventListener('click', ()=>refreshDB());
  }
});


/* ── СТАТИСТИКА НОД ── */


/* ── ПЛАГИНЫ НОД ── */

function pluginsDrawer(n, onSaved){
  openDrawer({
    title:'Плагины · '+esc(n.name), sub:'Правила применяются на самой ноде', icon:'plug', size:'lg', className:'sectioned-dialog',
    body:`<div id="pgBody" class="sub-note">Загружаем…</div>`,
    footer:`<div class="spacer"></div><button class="btn" data-close>Отмена</button>
            <button class="btn primary" data-save>Применить</button>`,
    async onMount(layer, close){
      let d;
      try { d = await API.call('/api/nodes/'+n.id+'/plugins'); }
      catch(e){ layer.querySelector('#pgBody').textContent = 'Не загрузилось: '+e.message; return; }

      const c = d.config || {};
      const tb = c.torrentBlocker || {}, inf = c.ingressFilter || {}, eg = c.egressFilter || {};
      const st = d.status;
      const lines = (a) => (a || []).join('\n');

      layer.querySelector('#pgBody').innerHTML = `
        ${st && (!st.nft_available || !st.can_modify) ? `
          <div class="notice" style="background:color-mix(in srgb, var(--err) 10%, transparent);border:1px solid color-mix(in srgb, var(--err) 30%, transparent);color:var(--err-ink);border-radius:10px;padding:11px 14px;font-size:12.5px;margin-bottom:14px">
            ${I('alert',13)} ${st.nft_available
              ? 'Агенту не хватает прав <code>NET_ADMIN</code> — правила не применятся.'
              : 'На сервере нет <code>nftables</code> — правила не применятся.'}
            Настройки сохранятся, но работать начнут только после исправления.
          </div>` : ''}

        <div class="form-section" style="margin-top:0"><h4>${I('magnet',13)} Блокировщик торрентов</h4>
          <label class="check"><input type="checkbox" id="pgTb" ${tb.enabled?'checked':''}>
            <span>Отрезать адрес при попытке торрент-трафика</span></label>
          <div class="two-col" style="margin-top:10px">
            <div class="field"><label>На сколько блокировать</label>
              <div class="inp-group"><input class="inp num" id="pgTbDur" type="number" min="0"
                value="${tb.blockDuration ?? 3600}"><span class="suffix">секунд</span></div>
              <div class="hint">0 — до перезагрузки ноды. Ядро снимает блокировку само.</div></div>
            <div class="field"><label>Не трогать эти адреса</label>
              <textarea class="inp mono" id="pgTbIgn" rows="3" placeholder="один адрес в строке">${esc(lines(tb.ignoreIps))}</textarea>
              <button class="btn sm" style="margin-top:6px" id="pgMyIp">${I('plus',12)} Добавить мой адрес</button>
              <div class="hint">Клиент ходит через VPN с того же адреса, с которого вы управляете сервером — блокировка отрежет и вас. Служебные диапазоны агент не трогает никогда, но свой внешний адрес добавьте сами. Помните и про общий адрес у мобильных операторов: под блокировку попадут соседи.</div></div>
          </div>
        </div>

        <div class="form-section"><h4>${I('shieldCheck',13)} Входящий фильтр</h4>
          <label class="check"><input type="checkbox" id="pgIn" ${inf.enabled?'checked':''}>
            <span>Не пускать эти адреса на ноду</span></label>
          <div class="field" style="margin-top:10px"><label>Адреса и подсети</label>
            <textarea class="inp mono" id="pgInIps" rows="4" placeholder="1.2.3.4&#10;10.0.0.0/8&#10;2001:db8::/32&#10;ext:мой-список">${esc(lines(inf.blockedIps))}</textarea>
            <div class="hint">По одному в строке. Поддерживаются подсети и IPv6.</div></div>
        </div>

        <div class="form-section"><h4>${I('ban',13)} Исходящий фильтр</h4>
          <label class="check"><input type="checkbox" id="pgEg" ${eg.enabled?'checked':''}>
            <span>Не выпускать с ноды на эти адреса и порты</span></label>
          <div class="two-col" style="margin-top:10px">
            <div class="field"><label>Адреса</label>
              <textarea class="inp mono" id="pgEgIps" rows="4">${esc(lines(eg.blockedIps))}</textarea></div>
            <div class="field"><label>Порты</label>
              <textarea class="inp mono" id="pgEgPorts" rows="4" placeholder="25&#10;465&#10;587">${esc(lines(eg.blockedPorts))}</textarea>
              <div class="hint">Почтовые порты закрывают, чтобы с ноды не рассылали спам и её не занесли в списки.</div></div>
          </div>
        </div>

        <div class="form-section"><h4>${I('layers',13)} Общие списки</h4>
          <div class="field"><label>Списки в формате JSON</label>
            <textarea class="inp mono" id="pgLists" rows="4" spellcheck="false" placeholder='[{"name":"мой-список","items":["10.0.0.0/8"]}]'>${esc(JSON.stringify(c.sharedLists || [], null, 1))}</textarea>
            <div class="hint">На список ссылаются как <code>ext:имя</code> в полях выше — удобно, когда один набор нужен в нескольких фильтрах.</div></div>
        </div>`;

      // Свой внешний адрес подставляем кнопкой: набирать его руками —
      // лишний повод ошибиться в том, что защищает от блокировки.
      layer.querySelector('#pgMyIp').addEventListener('click', async ()=>{
        try {
          const r = await API.call('/api/whoami');
          const ta = layer.querySelector('#pgTbIgn');
          if (ta.value.includes(r.ip)) { toast('Уже в списке', 'info'); return; }
          ta.value = (ta.value.trim() ? ta.value.trim() + '\n' : '') + r.ip;
          toast('Добавлен ' + r.ip);
        } catch(e){ toast('Не определился: '+e.message, 'err'); }
      });

      layer.querySelector('[data-save]').addEventListener('click', async ()=>{
        const arr = (id) => layer.querySelector(id).value.split('\n').map(x=>x.trim()).filter(Boolean);
        let sharedLists;
        try { sharedLists = JSON.parse(layer.querySelector('#pgLists').value || '[]'); }
        catch(e){ toast('Общие списки: не разбирается как JSON', 'err'); return; }

        const body = {
          torrentBlocker: {
            enabled: layer.querySelector('#pgTb').checked,
            blockDuration: parseInt(layer.querySelector('#pgTbDur').value, 10) || 0,
            ignoreIps: arr('#pgTbIgn'),
          },
          ingressFilter: { enabled: layer.querySelector('#pgIn').checked, blockedIps: arr('#pgInIps') },
          egressFilter: {
            enabled: layer.querySelector('#pgEg').checked,
            blockedIps: arr('#pgEgIps'),
            blockedPorts: arr('#pgEgPorts').map(Number).filter(Number.isFinite),
          },
          sharedLists,
        };
        try {
          await API.call('/api/nodes/'+n.id+'/plugins', { method:'PATCH', body });
          close();
          toast('Применится на ноде в течение 15 секунд');
          await onSaved();
        } catch(e){ toast('Не сохранилось: '+e.message, 'err'); }
      });
    }
  });
}

function blocksDrawer(n){
  openDrawer({
    title:'Блокировки · '+esc(n.name), sub:'Кого сейчас не пускает нода', icon:'ban',
    body:`<div id="blBody" class="sub-note">Загружаем…</div>`,
    footer:`<div class="spacer"></div><button class="btn" data-close>Закрыть</button>`,
    async onMount(layer){
      const draw = async () => {
        let list;
        try { list = await API.call('/api/nodes/'+n.id+'/blocks'); }
        catch(e){ layer.querySelector('#blBody').textContent = 'Не загрузилось: '+e.message; return; }

        layer.querySelector('#blBody').innerHTML = list.map(b=>`
          <div class="info-row">
            <span class="v mono" style="flex:1">${esc(b.ip)}</span>
            <span class="chip">${esc(b.reason)}</span>
            <span class="sub-note" style="width:120px;text-align:right">${
              b.until ? 'до '+fmtDT(b.until) : 'до перезагрузки'}</span>
            <button class="btn ghost icon-only" data-unbl="${esc(b.ip)}" title="Снять">${I('x',13)}</button>
          </div>`).join('') || `<div class="empty" style="padding:26px">${I('shieldCheck',28)}
            <b>Никто не заблокирован</b><span>Записи появляются, когда срабатывает блокировщик торрентов.</span></div>`;

        layer.querySelectorAll('[data-unbl]').forEach(b=>b.addEventListener('click', async ()=>{
          try {
            await API.call('/api/nodes/'+n.id+'/blocks/'+encodeURIComponent(b.dataset.unbl), { method:'DELETE' });
            toast('Снято — нода уберёт правило при следующем применении');
            await draw();
          } catch(e){ toast('Не получилось: '+e.message, 'err'); }
        }));
      };
      await draw();
    }
  });
}

/* ── СТАТИСТИКА НОД ── */
registerPage({
  id:'nodes-stats', title:'Статистика нод', group:'Инфраструктура', icon:'chart',
  render(){
    return `
    <div class="page-head">
      <div><h1>Статистика нод</h1><div class="desc">Трафик по дням. Данные — из счётчиков движка, которые присылает агент.</div></div>
      <div class="actions">
        <select class="inp" id="stDays" style="width:150px">
          <option value="14">14 дней</option><option value="30" selected>30 дней</option><option value="90">90 дней</option>
        </select>
      </div>
    </div>
    <div class="card" style="padding:0;overflow:auto"><div id="stBody" class="sub-note" style="padding:20px">Загружаем…</div></div>`;
  },
  async bind(root){
    const draw = async () => {
      let data;
      try { data = await API.call('/api/nodes/traffic?days=' + root.querySelector('#stDays').value); }
      catch(e){ root.querySelector('#stBody').textContent = 'Не загрузилось: '+e.message; return; }

      // Проверка пустоты — до первого обращения к данным. Раньше она стояла
      // ниже, и пустой ответ падал на flatMap, не дойдя до неё.
      if (!data || !Array.isArray(data.nodes) || !data.nodes.length) {
        root.querySelector('#stBody').innerHTML =
          `<div class="empty" style="padding:32px">${I('chart',30)}<b>Данных нет</b>
             <span>Подключите ноду — статистика появится, когда агент пришлёт счётчики.</span></div>`;
        return;
      }

      // Ось дней строим от сегодня назад, а не по тому, что пришло:
      // день без трафика должен остаться пустой клеткой, а не исчезнуть.
      const days = [];
      for (let i = (data.days || 30) - 1; i >= 0; i--) {
        const d = new Date(Date.now() - i * 86400000);
        days.push(d.toISOString().slice(0, 10));
      }

      const all = data.nodes.flatMap(n => Object.values(n.days || {}));
      const max = Math.max(1, ...all);

      root.querySelector('#stBody').innerHTML = `
        <table class="tbl" style="min-width:${240 + days.length * 34}px">
          <thead><tr><th style="min-width:200px">Нода</th>
            ${days.map(d=>`<th style="text-align:right;font-size:9.5px">${d.slice(8)}.${d.slice(5,7)}</th>`).join('')}
            <th style="text-align:right;min-width:90px">Всего</th></tr></thead>
          <tbody>${data.nodes.map(n=>{
            const total = Object.values(n.days || {}).reduce((a,b)=>a+b, 0);
            return `<tr>
              <td><div class="cell-main">${ccChip(n.country_code)}<b class="mono">${esc(n.name)}</b></div></td>
              ${days.map(d=>{
                const v = n.days[d];
                if (v == null) return `<td style="text-align:right" title="${d}: данных нет"><span class="sub-note">·</span></td>`;
                const a = Math.min(.85, v / max);
                return `<td style="text-align:right">
                  <span class="heat num" title="${d}: ${fmtBytes(v)}"
                    style="background:rgba(159,232,112,${a.toFixed(2)});color:${a>.45?'var(--on-accent)':'var(--text-2)'}">
                    ${v ? fmtBytes(v).replace(/ /,'') : '0'}</span></td>`;
              }).join('')}
              <td style="text-align:right"><b class="num">${fmtBytes(total)}</b></td>
            </tr>`;
          }).join('')}</tbody>
        </table>`;
    };
    root.querySelector('#stDays').addEventListener('change', draw);
    await draw();
  }
});


/* ── ПЛАГИНЫ НОД ── */
registerPage({
  id:'node-plugins',title:'Плагины нод',group:'Инфраструктура',icon:'plug',
  render(){return `<div class="page-head"><div><h1>Защита и фильтры нод</h1><div class="desc">Встроенные возможности агента: IP-фильтры, исходящие порты и блокировки. Состояние применения приходит с сервера.</div></div></div><div class="tbl-wrap"><table class="tbl"><thead><tr><th>Нода</th><th>Движок</th><th>Возможности</th><th>Действия</th></tr></thead><tbody>${DB.nodes.map(n=>`<tr><td>${flag(n.cc)} ${esc(n.name)}</td><td>${esc(n.xray)}</td><td data-plugin-state="${n.id}">Проверяем…</td><td><button class="btn sm" data-config="${n.id}">Настроить фильтры</button> <button class="btn sm" data-blocks="${n.id}">Блокировки</button></td></tr>`).join('')||'<tr><td colspan="4">Сначала подключите ноду в разделе «Ноды».</td></tr>'}</tbody></table></div>`;},
  async bind(root){
    root.querySelectorAll('[data-config]').forEach(b=>b.onclick=()=>pluginsDrawer(DB.nodes.find(n=>n.id===b.dataset.config),()=>this.bind(root)));
    root.querySelectorAll('[data-blocks]').forEach(b=>b.onclick=()=>blocksDrawer(DB.nodes.find(n=>n.id===b.dataset.blocks)));
    await Promise.all(DB.nodes.map(async n=>{const el=root.querySelector('[data-plugin-state="'+n.id+'"]');try{const d=await API.call('/api/nodes/'+n.id+'/plugins');const st=d.status;el.textContent=!st?'Агент ещё не отчитался':st.error?'Ошибка: '+st.error:!st.nft_available?'Установите nftables':!st.can_modify?'Нет прав NET_ADMIN':st.applied?'Правила применены':'Готов к настройке';}catch(e){el.textContent='Не удалось проверить: '+e.message;}}));
  }
});

/* ═══════════ Операции над нодой ═══════════ */

function resetNodeTraffic(n){
  confirmModal({
    title:'Сбросить трафик ноды?', danger:false,
    text:`Обнулится статистика <b>${esc(n.name)}</b>. Счётчики клиентов не трогаем — иначе одна кнопка раздала бы всем безлимит.`,
    okText:'Сбросить',
    onOk:async ()=>{
      try {
        const r = await API.call('/api/nodes/'+n.id+'/reset-traffic', { method:'POST' });
        toast('Статистика ноды обнулена (записей: '+r.deleted_rows+')');
        await refreshDB();
      } catch(e){ toast('Не получилось: '+e.message, 'err'); }
    }
  });
}

/// Версия Xray, приведена ли к ней вся ферма.
///
/// Показываем, только когда есть расхождение: ровный парк — обычное
/// состояние, и полоска про него была бы шумом. А вот нода, отставшая на
/// пару версий, — причина, по которой у части клиентов «не работает
/// локация», и найти её иначе можно только перебором.
function engineBanner(){
  const want = (DB.engineVersion || '').trim();
  if (!want) return '';
  const norm = v => String(v || '').trim().replace(/^v/, '');
  const behind = DB.nodes.filter(n => n.xray && n.xray !== '—' && norm(n.xray) !== norm(want));
  if (!behind.length) return '';
  return `
    <div class="notice warn" style="margin-bottom:14px">
      ${I('alert',13)} Xray ${esc(want)} стоит не везде: отстают
      ${behind.map(n=>`<b>${esc(n.name)}</b> (${esc(n.xray)})`).join(', ')}.
      Ноды обновятся сами в течение минуты — если этого не произошло,
      причина будет в строке ноды.
    </div>`;
}

/// Команда ручного обновления ноды.
///
/// Нужна там, где агент старше панели и её команд не понимает. Секрет
/// ноды в ней не участвует: он уже прописан в systemd-юните, и показывать
/// его повторно панель всё равно не умеет.
function agentUpdateCmd(){
  const base = (DB.panelPublicUrl || location.origin).replace(/\/+$/, '');
  const q = v => "'"+v.replaceAll("'", "'\"'\"'")+"'";
  return `env PANEL_URL=${q(base)} bash -o pipefail -c 'curl -fsSL "$PANEL_URL/update-node.sh" | bash'`;
}

/// Какую версию движка держать на нодах.
///
/// Одна на весь парк: разные ядра означают локации, которые работают у
/// одних клиентов и не работают у других. Агент сверяет свою версию с
/// этой при каждом опросе и обновляется сам.
function engineVersionModal(){
  const normal = v => v ? 'v'+v.replace(/^v/,'') : '';
  const cur = normal((DB.engineVersion || '').trim());
  const seen = [...new Set(DB.nodes.map(n=>n.xray).filter(v=>v && v!=='—'))];
  let selected=cur, items=[];
  const oldAgents = () => DB.nodes.filter(n => {
    // A brand-new node has no executable yet; the installer supplies the current agent.
    if (n.safeEngineUpdate || !n.lastSeen && (!n.agent || n.agent==='—')) return false;
    const v=String(n.agent||'').match(/^v?(\d+)\.(\d+)\.(\d+)$/);
    return !v || Number(v[1])===0 && (Number(v[2])<1 || Number(v[2])===1 && Number(v[3])<2);
  });
  openModal({
    title:'Версия Xray', sub:'Выберите релиз для установки и обновления нод', icon:'refresh', size:'lg',
    body:`<div class="xray-picker">
      <div class="xray-current"><div><span>Установлено на нодах</span><b class="mono">${seen.length?seen.map(esc).join(', '):'Нет данных'}</b></div>
        <div><span>Задано в панели</span><b class="mono">${esc(cur)||'Не задано'}</b></div></div>
      <div class="xray-picker-tools"><label class="field"><span>Поиск версии</span><input class="inp mono" id="evSearch" placeholder="Номер релиза" autocomplete="off"></label>
        <label class="field"><span>Канал релизов</span><select class="inp" id="evChannel"><option value="all">Все релизы</option><option value="stable">Только стабильные</option></select></label>
        <button class="btn icon" id="evReload" aria-label="Обновить список релизов">${I('refresh',16)}</button></div>
      <div id="evCatalogStatus" class="hint" role="status"></div>
      <div id="evReleases" class="xray-releases" role="group" aria-label="Релизы Xray"><p class="hint">Загружаем релизы с GitHub…</p></div>
      <div class="xray-selection" id="evSelection" aria-live="polite"></div>
      <section class="xray-selection" aria-label="Готовность агентов"><div><strong>Сначала проверьте агентов</strong></div>
        <p id="evAgents" role="status"></p><div class="btns" style="margin-top:12px;flex-wrap:wrap"><button class="btn sm" data-upd-agents>${I('refresh',12)} Обновить агентов</button><button class="btn sm" data-check-agents>Проверить готовность</button></div></section>
      <p class="hint xray-impact">Смена версии перезапустит Xray и прервёт соединения. Агент с поддержкой безопасного обновления проверяет архив и текущий конфиг; если новая версия не запустится, восстанавливает предыдущую. Новые ноды получат выбранную версию при установке.</p>
      <details class="xray-advanced"><summary>Указать тег вручную или отключить управление</summary>
        <div class="field"><label for="evVal">Тег релиза GitHub</label><input class="inp mono" id="evVal" value="${esc(cur)}" placeholder="vГОД.МЕСЯЦ.ДЕНЬ" autocomplete="off"></div>
        <button class="btn sm" id="evUnpin">Оставить текущие версии на нодах</button><p class="hint">Без закреплённой версии установщик использует проверенный релиз из поставки панели.</p>
      </details>
      <details class="xray-advanced"><summary>Агент ноды и ручное обновление</summary>
        <p class="hint">Для проверки архива и отката нужен актуальный агент STEALTHNET. Обновите агентов перед сменой Xray.</p>
        <p class="hint">Если старая нода не принимает команду, выполните на её сервере от root:</p>
        <div class="code wrap"><pre>${esc(agentUpdateCmd())}</pre></div>
        <button class="btn sm" data-copy="${esc(agentUpdateCmd())}" data-copy-msg="Команда скопирована">${I('copy',12)} Копировать команду</button>
      </details>
    </div>`,
    footer:`<div class="spacer"></div><button class="btn" data-close>Отмена</button><button class="btn primary" data-save disabled>Применить к нодам</button>`,
    onMount(l, close){
      const save=l.querySelector('[data-save]'), manual=l.querySelector('#evVal');
      function selection(){
        const chosen=items.find(r=>normal(r.tag)===selected);
        const valid=!selected||/^v\d{1,5}\.\d{1,5}\.\d{1,5}$/.test(selected);
        const old=oldAgents();
        save.disabled=!valid || selected===cur || Boolean(selected && old.length);
        l.querySelector('#evAgents').textContent=old.length ? `Нужно обновить агента на ${old.length} нодах: ${old.slice(0,3).map(n=>n.name).join(', ')}${old.length>3?'…':''}. Обновите агентов и проверьте готовность. До этого смена версии недоступна; отключить управление можно.` : 'Агенты готовы. Ноды без первого отчёта установите командой из их карточки: она содержит актуального агента.';
        l.querySelector('#evSelection').innerHTML=selected?`<div><span>Выбрано</span><strong class="mono">${esc(selected)}</strong>${chosen?`<span class="bdg ${chosen.prerelease?'warn':'ok'}">${chosen.prerelease?'Предварительный':'Стабильный'}</span>`:''}</div>
          <p>${!valid?'Введите тег в формате vГОД.МЕСЯЦ.ДЕНЬ.':chosen?.prerelease?'Предварительный релиз: может содержать изменения, которые ещё проходят проверку.':chosen?'Стабильный релиз. Перед понижением версии проверьте поддержку протоколов вашего профиля.':'Тег задан вручную. Его наличие и совместимость проверит агент перед заменой.'}</p>
          ${chosen?`<a href="${esc(chosen.url)}" target="_blank" rel="noopener">Изменения в релизе ${I('external',12)}</a>`:''}`:'<div><strong>Версии останутся на нодах</strong></div><p>Панель перестанет автоматически обновлять Xray.</p>';
      }
      function render(){
        const query=l.querySelector('#evSearch').value.trim().toLowerCase();
        const stable=l.querySelector('#evChannel').value==='stable';
        const rows=items.filter(r=>(!stable||!r.prerelease)&&r.tag.toLowerCase().includes(query));
        l.querySelector('#evReleases').innerHTML=rows.length?rows.map(r=>`<label class="xray-release"><input type="radio" name="xray-release" value="${esc(r.tag)}" ${normal(r.tag)===selected?'checked':''}>
          <span class="xray-release-title"><b class="mono">${esc(r.tag)}</b><small>${esc(r.published_at?new Date(r.published_at).toLocaleDateString(currentLang(),{day:'numeric',month:'short',year:'numeric'}):'Дата не указана')}${r===items[0]?' · '+(currentLang()==='en'?'Newest':'Самый свежий'):''}</small></span>
          <span class="bdg ${r.prerelease?'warn':'ok'}">${r.prerelease?'Предварительный':'Стабильный'}</span></label>`).join(''):'<p class="hint">Нет подходящих релизов. Измените поиск или канал.</p>';
        selection();
      }
      async function fetchReleases(){
        const reload=l.querySelector('#evReload');reload.disabled=true;
        l.querySelector('#evCatalogStatus').textContent='Загружаем список…';
        try {const r=await API.call('/api/nodes/xray-releases');if(!l.isConnected)return;items=r.items||[];render();l.querySelector('#evCatalogStatus').textContent=r.warning||'Источник: XTLS/Xray-core · версии для Linux amd64 и arm64';}
        catch(e){if(!l.isConnected)return;l.querySelector('#evCatalogStatus').textContent=e.message;l.querySelector('#evReleases').innerHTML='<p class="hint">Нажмите «Обновить список релизов» или укажите тег вручную ниже.</p>';}
        finally{reload.disabled=false;}
      }
      l.querySelector('#evReleases').addEventListener('change',e=>{if(e.target.matches('input[type=radio]')){selected=normal(e.target.value);manual.value=selected;selection();}});
      l.querySelector('#evSearch').addEventListener('input',render);
      l.querySelector('#evChannel').addEventListener('change',render);
      l.querySelector('#evReload').addEventListener('click',fetchReleases);
      manual.addEventListener('input',()=>{selected=normal(manual.value.trim());render();});
      l.querySelector('#evUnpin').addEventListener('click',()=>{selected='';manual.value='';render();});
      l.querySelector('[data-upd-agents]').addEventListener('click',async e=>{const btn=e.currentTarget;btn.disabled=true;try{const r=await API.call('/api/nodes/update-agents',{method:'POST'});toast(`Команда принята, нод: ${r.nodes}. Результат появится в карточках.`);}catch(err){toast(err.message,'err');}finally{btn.disabled=false;}});
      l.querySelector('[data-check-agents]').addEventListener('click',async e=>{const btn=e.currentTarget;btn.disabled=true;try{await loadDB();selection();}catch(err){toast(err.message,'err');}finally{btn.disabled=false;}});
      save.addEventListener('click',async()=>{if(selected && oldAgents().length){selection();return;}save.disabled=true;try{await API.call('/api/settings',{method:'PATCH',body:{'nodes.engine_version':selected}});close();toast(selected?`Задана ${selected}. Следите за версиями и статусом Xray в списке нод.`:'Автоматическое обновление Xray отключено');await refreshDB();}catch(e){save.disabled=false;toast(e.message,'err');}});
      selection();fetchReleases();
    }
  });
}

/// Перезапуск движка на ноде.
///
/// Панель ставит отметку, агент видит её на ближайшем опросе. Мгновенно
/// это не происходит и не может: агент ходит в панель сам, обратного
/// канала до ноды нет.
function restartNodeEngine(n){
  confirmModal({
    title:'Перезапустить Xray?', danger:true,
    text:`Движок на <b>${esc(n.name)}</b> будет перезапущен. Соединения клиентов
          на этой ноде оборвутся — приложения переподключатся сами, обычно за несколько секунд.
          <br><br>Нужно, когда движок формально жив, но работает не так: подхватил
          обновлённый сертификат, завис после смены сети.`,
    okText:'Перезапустить',
    onOk:async ()=>{
      try {
        await API.call('/api/nodes/'+n.id+'/restart-engine', { method:'POST' });
        toast('Команда принята — нода перезапустит движок в течение 15 секунд');
      } catch(e){ toast('Не получилось: '+e.message, 'err'); }
    }
  });
}

function toggleNode(n){
  // Выключенную ноду агент не обслуживает, а её хосты исчезают из подписок:
  // выдавать клиенту локацию, которой нет, хуже, чем не выдать её вовсе.
  const on = n.status === 'disabled';
  confirmModal({
    title: on ? 'Включить ноду?' : 'Отключить ноду?',
    danger: !on,
    text: on
      ? `<b>${esc(n.name)}</b> снова начнёт обслуживать клиентов и появится в подписках.`
      : `Клиенты на <b>${esc(n.name)}</b> переключатся на другие локации, её хосты пропадут из подписок.`,
    okText: on ? 'Включить' : 'Отключить',
    onOk:async ()=>{
      try {
        await API.call('/api/nodes/'+n.id, { method:'PATCH',
          body:{ status: on ? 'provisioning' : 'disabled' } });
        toast(on ? 'Нода включена — дождитесь связи с агентом' : 'Нода отключена');
        await refreshDB();
      } catch(e){ toast('Не получилось: '+e.message, 'err'); }
    }
  });
}

function deleteNode(n){
  confirmModal({
    title:`Удалить ${esc(n.name)}?`, danger:true,
    text:'Нода будет отвязана от панели. Сервер и его данные не трогаем — только связь.',
    okText:'Удалить',
    onOk:async ()=>{
      try {
        await API.call('/api/nodes/'+n.id, { method:'DELETE' });
        toast('Нода удалена из панели');
        await refreshDB();
      } catch(e){ toast('Не удалилась: '+e.message, 'err'); }
    }
  });
}
