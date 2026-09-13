/* Infrastructure console. Product data only; no synthetic measurements. */
'use strict';

function nodeReportFresh(n){
  const at=new Date(n.lastSeen).getTime();
  return !!n.lastSeen&&Number.isFinite(at)&&Date.now()-at<=180000;
}
function nodeHealth(n){
  if(n.status==='disabled')return {key:'disabled',tone:'neutral',label:'Отключён',detail:'Отключён администратором',rank:4};
  if(n.status!=='online')return {key:'attention',tone:'err',label:n.status==='connecting'?'Подключается':'Нет связи',detail:n.lastSeen?'Агент перестал передавать данные':'Ожидаем первый отчёт агента',rank:0};
  if(n.engineOk===false)return {key:'attention',tone:'err',label:'Xray не работает',detail:n.engineError||'Агент отвечает, но движок не запущен',rank:0};
  if(!nodeReportFresh(n))return {key:'attention',tone:'warn',label:'Данные устарели',detail:'Нет актуального отчёта за последние три минуты',rank:1};
  if(n.engineOk!==true)return {key:'attention',tone:'warn',label:'Статус Xray неизвестен',detail:'Агент не подтвердил работу VPN-движка',rank:1};
  if(n.cpu>=90||n.ram>=90)return {key:'attention',tone:'warn',label:'Высокая нагрузка',detail:n.cpu>=90?'CPU занят на 90% или больше':'Память занята на 90% или больше',rank:1};
  return {key:'healthy',tone:'ok',label:'Работает',detail:'Агент на связи, Xray запущен',rank:3};
}
function nodeHosts(n){return DB.hosts.filter(h=>h.profile===n.profile&&n.inbounds.includes(h.inbound));}
function nodeMeasure(value,format){return value==null?'—':format(value);}
function nodePercent(value){return nodeMeasure(value,x=>Math.round(x)+'%');}
function nodeMeter(value,label){
  if(value==null)return '<span class="node-no-data">Нет замера</span>';
  const pct=Math.max(0,Math.min(100,value));
  return `<div class="node-meter"><span class="num">${Math.round(value)}%</span><meter min="0" max="100" value="${pct}" class="${pct>=90?'critical':pct>=75?'loaded':''}" aria-label="${esc(label)}">${Math.round(value)}%</meter></div>`;
}
function nodeConsoleRow(n){
  const h=nodeHealth(n),hosts=nodeHosts(n),stale=h.key!=='healthy'&&h.label!=='Высокая нагрузка';
  return `<tr class="${stale?'node-stale':''}">
    <td><button class="node-identity" data-node-open="${n.id}">${ccChip(n.cc)}<span><b>${esc(n.name)}</b><small class="mono">${esc(n.addr)}</small></span></button></td>
    <td><span class="bdg ${h.tone}"><span class="dot"></span>${h.label}</span><small class="node-secondary">${esc(n.profile)}</small></td>
    <td>${nodeMeter(n.cpu,'Загрузка процессора')}<small class="node-secondary">${n.cpuCores?esc(n.cpuCores)+' ядер':'CPU'}</small></td>
    <td>${nodeMeter(n.ram,'Загрузка памяти')}<small class="node-secondary">${nodeMeasure(n.memUsed,fmtBytes)} / ${nodeMeasure(n.memTotal,fmtBytes)}</small></td>
    <td><div class="node-transfer"><span>${I('download',12)} ${nodeMeasure(n.rxBps,fmtBps)}</span><span>${I('upload',12)} ${nodeMeasure(n.txBps,fmtBps)}</span></div></td>
    <td><b class="num">${fmtBytes(n.todayBytes)}</b><small class="node-secondary">${hosts.filter(x=>x.enabled).length} активных хостов</small></td>
    <td><small class="node-secondary">${n.lastSeen?fmtAgo(n.lastSeen):'Ещё не подключён'}</small><button class="btn ghost icon-only" aria-label="Действия: ${esc(n.name)}" data-node-menu="${n.id}">${I('more',16)}</button></td>
  </tr>`;
}
function nodeConsoleCard(n){
  const h=nodeHealth(n),hosts=nodeHosts(n);
  return `<article class="fleet-server-card"><div class="server-card-heading">${ccChip(n.cc)}<button class="server-card-name" data-node-open="${n.id}"><b>${esc(n.name)}</b><span>${esc(n.addr)}</span></button><button class="btn ghost icon-only" data-node-menu="${n.id}" aria-label="Действия: ${esc(n.name)}">${I('more',16)}</button></div>
    <div class="server-card-status"><span class="bdg ${h.tone}"><span class="dot"></span>${h.label}</span><span>${n.lastSeen?fmtAgo(n.lastSeen):'Ждём агента'}</span></div>
    <div class="server-card-resources"><div><span>Процессор</span>${nodeMeter(n.cpu,'Загрузка CPU')}</div><div><span>Память</span>${nodeMeter(n.ram,'Загрузка памяти')}</div></div>
    <dl class="server-card-facts"><div><dt>Трафик сегодня</dt><dd>${fmtBytes(n.todayBytes)}</dd></div><div><dt>Канал · приём / отдача</dt><dd>${nodeMeasure(n.rxBps,fmtBps)} / ${nodeMeasure(n.txBps,fmtBps)}</dd></div><div><dt>Профиль</dt><dd>${esc(n.profile)}</dd></div><div><dt>Активные хосты</dt><dd>${hosts.filter(x=>x.enabled).length}</dd></div></dl>
    <div class="server-card-footer"><button class="btn sm ${n.lastSeen?'':'primary'}" data-node-install="${n.id}">${I('terminal',14)} Команда установки</button><button class="btn sm" data-node-open="${n.id}">Открыть сервер ${I('chevR',13)}</button></div></article>`;
}
function nodeFleetOverview(nodes){
  const healthy=nodes.filter(n=>nodeHealth(n).key==='healthy'),issues=nodes.filter(n=>nodeHealth(n).key==='attention');
  const countries=[...new Set(nodes.map(n=>n.cc))];
  const live=nodes.filter(n=>n.status==='online'&&nodeReportFresh(n));
  const hasNetworkMeasurement=live.some(n=>n.rxBps!=null||n.txBps!=null);
  const sum=(arr,key)=>arr.reduce((s,n)=>s+(n[key]||0),0);
  return `
      <div class="fleet-status"><div class="fleet-status-title">${I(issues.length?'alert':'shieldCheck',26)}<h2>${!nodes.length?'Начните со своего сервера':issues.length?'Сеть требует внимания':'Серверы под контролем'}</h2></div><p>${!nodes.length?'Мастер подготовит команду установки агента и поможет связать сервер с конфигурацией.':`${healthy.length} из ${nodes.length} серверов работают без обнаруженных проблем. Доступность VPN проверяйте также через клиентское приложение.`}</p>
        <div class="fleet-counts"><div><b>${nodes.length}</b><span>серверов</span></div><div><b>${countries.length}</b><span>стран</span></div><div><b>${fmtBytes(sum(nodes,'todayBytes'))}</b><span>трафик сегодня · UTC</span></div><div><b>${hasNetworkMeasurement?fmtBps(sum(live,'rxBps')+sum(live,'txBps')):'—'}</b><span>сеть · приём + отдача</span></div></div>
      </div>
      <div class="fleet-attention"><div class="fleet-section-label"><h3>${issues.length?'Требуют внимания':'Быстрые переходы'}</h3>${issues.length?`<span class="bdg warn">${issues.length}</span>`:''}</div>${issues.length?issues.slice(0,3).map(n=>`<button class="fleet-issue" data-node-open="${n.id}"><span class="status-dot ${nodeHealth(n).tone}"></span><span><b>${esc(n.name)}</b><small>${esc(nodeHealth(n).detail)}</small></span>${I('chevR',14)}</button>`).join('')+(issues.length>3?'<button class="btn ghost sm" data-all-issues>Показать все проблемы</button>':''):`<button class="fleet-shortcut" data-fleet-engine>${I('refresh',18)}<span><b>Движок и агент</b><small>Версии и инструкции обновления</small></span>${I('chevR',14)}</button><button class="fleet-shortcut" data-fleet-stats>${I('chart',18)}<span><b>История трафика</b><small>Потребление по дням и серверам</small></span>${I('chevR',14)}</button>`}</div>
    `;
}
function renderNodeConsole(metrics=false){
  const nodes=DB.nodes,healthy=nodes.filter(n=>nodeHealth(n).key==='healthy'),issues=nodes.filter(n=>nodeHealth(n).key==='attention');
  const countries=[...new Set(nodes.map(n=>n.cc))].sort();
  return `<!-- THESIS: Find the server problem, inspect evidence, act. OWN-WORLD: navy and teal operating surfaces, clear ruled inventory, status color only. STORY: fleet health to server to diagnosis. FIRST VIEWPORT: health summary and attention queue above searchable inventory; primary add action top right. FORM: operations overview, candidate 1, seed 4f9eba03. FINISH: unreviewed and undocumented is unfinished; this build ends with the finish review, the verdict, DESIGN.md, and every shipping raster carrying its provenance -->
  <div class="node-console">
    <div class="page-head"><div><h1>${metrics?'Нагрузка и ресурсы':'Ваша сеть'}</h1><div class="desc">${metrics?'Сравнивайте серверы и открывайте историю нагрузки. Устаревшие показания отмечены отдельно.':'От общего состояния — к конкретному серверу. Подключайте локации, следите за ресурсами и устраняйте проблемы.'}</div></div><div class="actions"><button class="btn" data-fleet-engine>${I('cpu',14)} Версия Xray</button><button class="btn" data-fleet-refresh>${I('refresh',14)} Обновить</button><button class="btn primary" data-fleet-add>${I('plus',14)} Подключить сервер</button></div></div>
    <section class="fleet-overview" id="fleetOverview" aria-label="Состояние сети">${nodeFleetOverview(nodes)}</section>
    <p class="fleet-updated" id="fleetUpdated" role="status"></p>
    <section class="fleet-inventory" aria-label="Серверы"><div class="fleet-inventory-head"><h2>${metrics?'Показатели серверов':'Серверы и локации'}</h2><div class="fleet-view-toggle" aria-label="Вид серверов"><button data-fleet-view="cards" aria-pressed="true">${I('dash',14)} Карточки</button><button data-fleet-view="table" aria-pressed="false">${I('columns',14)} Таблица</button></div></div>
    <div class="fleet-toolbar"><div class="search-inp">${I('search',16)}<input class="inp" id="fleetSearch" aria-label="Поиск серверов" placeholder="Название, IP, профиль, хостер…"></div><select class="inp" id="fleetCountry" aria-label="Страна сервера"><option value="">Все страны</option>${countries.map(cc=>`<option value="${esc(cc)}">${esc(cc)}</option>`).join('')}</select><select class="inp" id="fleetSort" aria-label="Сортировка серверов"><option value="health">Сначала проблемы</option><option value="cpu" ${metrics?'selected':''}>По загрузке CPU</option><option value="ram">По загрузке памяти</option><option value="traffic">По трафику</option><option value="name">По имени</option></select></div>
    <div class="fleet-filters" aria-label="Фильтр состояния">${[['all','Все',nodes.length],['healthy','Работают',healthy.length],['attention','Требуют внимания',issues.length],['disabled','Отключены',nodes.filter(n=>n.status==='disabled').length]].map(([key,label,count])=>`<button class="${key==='all'?'active':''}" data-fleet-filter="${key}" aria-pressed="${key==='all'}">${label}<span>${count}</span></button>`).join('')}<span class="fleet-result" id="fleetCount" aria-live="polite"></span></div>
    <div id="fleetCards" class="fleet-cards"></div><div id="fleetTable" class="fleet-table-scroll" hidden><table class="tbl fleet-table"><thead><tr><th>Сервер</th><th>Состояние / профиль</th><th>Процессор</th><th>Память</th><th>Канал сейчас</th><th>Сегодня</th><th>Последняя связь</th></tr></thead><tbody id="fleetRows"></tbody></table></div><p class="fleet-footnote">${I('info',14)} Показатели — из последнего отчёта агента. Нет замера — не значит нулевая нагрузка. Нажмите на сервер, чтобы открыть диагностику.</p></section>
  </div>`;
}
function bindNodeConsole(root){
  let filter='all',view=currentPage.id==='nodes-metrics'?'table':'cards',loading=false,refreshError='';
  const draw=()=>{
    root.querySelector('#fleetOverview').innerHTML=nodeFleetOverview(DB.nodes);
    root.querySelectorAll('[data-fleet-filter]').forEach(b=>{b.querySelector('span').textContent=DB.nodes.filter(n=>b.dataset.fleetFilter==='all'||nodeHealth(n).key===b.dataset.fleetFilter).length;});
    root.querySelector('#fleetUpdated').textContent=refreshError||`Данные обновлены ${DB.nodesFetchedAt?new Date(DB.nodesFetchedAt).toLocaleTimeString('ru',{hour:'2-digit',minute:'2-digit',second:'2-digit'}):'при открытии панели'} · обновление каждые 30 секунд`;
    const q=root.querySelector('#fleetSearch').value.trim().toLowerCase(),cc=root.querySelector('#fleetCountry').value,sort=root.querySelector('#fleetSort').value;
    const rows=DB.nodes.filter(n=>(filter==='all'||nodeHealth(n).key===filter)&&(!cc||n.cc===cc)&&(!q||[n.name,n.addr,n.profile,n.infraProvider].join(' ').toLowerCase().includes(q)));
    rows.sort((a,b)=>sort==='cpu'?(b.cpu??-1)-(a.cpu??-1):sort==='ram'?(b.ram??-1)-(a.ram??-1):sort==='traffic'?b.todayBytes-a.todayBytes:sort==='name'?a.name.localeCompare(b.name):nodeHealth(a).rank-nodeHealth(b).rank||a.name.localeCompare(b.name));
    root.querySelector('#fleetRows').innerHTML=rows.map(nodeConsoleRow).join('')||`<tr><td colspan="7"><div class="empty"><b>${DB.nodes.length?'Серверы не найдены':'Подключите первый сервер'}</b><span>${DB.nodes.length?'Измените поиск или фильтры.':'Нажмите «Подключить сервер», чтобы получить команду установки.'}</span></div></td></tr>`;
    root.querySelector('#fleetCount').textContent=`${rows.length} из ${DB.nodes.length}`;
    root.querySelector('#fleetCards').innerHTML=rows.map(nodeConsoleCard).join('')||'<div class="empty"><b>Серверы не найдены</b><span>Измените фильтры или подключите новый сервер.</span></div>';
    root.querySelector('#fleetCards').hidden=view!=='cards';root.querySelector('#fleetTable').hidden=view!=='table';
    root.querySelectorAll('[data-fleet-view]').forEach(b=>b.setAttribute('aria-pressed',String(b.dataset.fleetView===view)));
  };
  const refresh=async()=>{
    if(loading||!root.isConnected)return;
    loading=true;root.querySelector('[data-fleet-refresh]').disabled=true;
    try{
      const nodes=await API.call('/api/nodes');
      if(!root.isConnected)return;
      if(!Array.isArray(nodes))throw new Error('Некорректный ответ сервера');
      DB.nodes=nodes.map(mapNode);DB.nodesFetchedAt=Date.now();refreshError='';
    }catch(e){refreshError='Не удалось обновить данные. '+e.message+' · Повторите кнопкой «Обновить»';}
    finally{loading=false;if(root.isConnected){root.querySelector('[data-fleet-refresh]').disabled=false;draw();}}
  };
  const timer=setInterval(()=>{if(!root.isConnected){clearInterval(timer);return;}if(document.visibilityState!=='hidden')refresh();},30000);
  const applyFilter=value=>{filter=value;root.querySelectorAll('[data-fleet-filter]').forEach(b=>{b.classList.toggle('active',b.dataset.fleetFilter===value);b.setAttribute('aria-pressed',String(b.dataset.fleetFilter===value));});draw();};
  root.querySelector('#fleetSearch').oninput=draw;root.querySelector('#fleetCountry').onchange=draw;root.querySelector('#fleetSort').onchange=draw;
  root.addEventListener('click',e=>{
    const b=e.target.closest('button');if(!b)return;
    if(b.dataset.nodeOpen)openNodeOverview(DB.nodes.find(n=>n.id===b.dataset.nodeOpen));
    if(b.dataset.nodeInstall)nodeInstallationCommand(DB.nodes.find(n=>n.id===b.dataset.nodeInstall));
    if(b.dataset.nodeMenu)nodeMenu(b,DB.nodes.find(n=>n.id===b.dataset.nodeMenu));
    if(b.dataset.fleetFilter)applyFilter(b.dataset.fleetFilter);
    if(b.dataset.fleetView){view=b.dataset.fleetView;draw();}
    if(b.hasAttribute('data-all-issues'))applyFilter('attention');
    if(b.hasAttribute('data-fleet-add'))createNodeStepper();
    if(b.hasAttribute('data-fleet-engine'))engineVersionModal();
    if(b.hasAttribute('data-fleet-stats'))navigate('nodes-stats');
    if(b.hasAttribute('data-fleet-refresh'))refresh();
  });draw();
}

function nodeFact(label,value){return `<div class="node-fact"><dt>${label}</dt><dd>${value}</dd></div>`;}
function nodeDiagnostic(n){
  const h=nodeHealth(n),hosts=nodeHosts(n),fresh=n.status==='online'&&nodeReportFresh(n);
  const checks=[
    [fresh?'ok':'warn','Связь с агентом',fresh?'Получен актуальный отчёт агента':n.status==='disabled'?'Сервер отключён в панели':'Проверьте службу sn-node, адрес панели и доступ в интернет'],
    [n.engineOk===false?'err':fresh&&n.engineOk===true?'ok':'warn','VPN-движок',n.engineOk===false?n.engineError||'Проверьте конфигурацию и журнал агента':fresh&&n.engineOk===true?`Xray ${n.xray}; запуск подтверждён отчётом агента`:'Нет актуального подтверждения работы движка. Обновите данные и проверьте журнал агента'],
    [n.profile!=='—'?'ok':'warn','Конфигурация',n.profile!=='—'?`Профиль ${n.profile}; инбаундов: ${n.inbounds.length}`:'Назначьте профиль конфигурации'],
    [hosts.some(x=>x.enabled)?'ok':'warn','Выдача клиентам',hosts.some(x=>x.enabled)?`${hosts.filter(x=>x.enabled).length} активных хостов для выбранных инбаундов`:'Добавьте хост и включите его в доступный клиенту сквад'],
  ];
  return `<div class="node-diagnostic"><h3>Проверка готовности</h3><p class="node-muted">Это проверка состояния панели и отчётов агента. Фактическое VPN-подключение проверяется из приложения.</p>${checks.map(([tone,title,text])=>`<div class="node-check"><span class="bdg ${tone}">${I(tone==='ok'?'check':'alert',14)}</span><div><b>${title}</b><p>${esc(text)}</p></div></div>`).join('')}<details class="node-command-help"><summary>Команды для диагностики на сервере</summary><p>Выполните по SSH. Xray запускается агентом sn-node; его ошибки попадают в журнал агента. Команды только читают состояние.</p>${['systemctl status sn-node --no-pager','journalctl -u sn-node -n 80 --no-pager','ps -C xray -o pid,etime,%cpu,%mem'].map(cmd=>`<div class="node-command"><code>${cmd}</code><button class="btn ghost icon-only" data-copy="${cmd}" aria-label="Скопировать команду">${I('copy',14)}</button></div>`).join('')}</details></div>`;
}
function nodeHistoryChart(samples,key,label,format){
  const rows=samples.filter(x=>x[key]!=null&&Number.isFinite(Number(x[key])));
  if(!rows.length)return `<section class="node-chart"><h3>${label}</h3><p class="node-muted">Нет измерений за этот период.</p></section>`;
  const buckets=new Map();for(const x of rows){const k=Math.floor(new Date(x.at).getTime()/900000);if(!buckets.has(k))buckets.set(k,[]);buckets.get(k).push(Number(x[key]));}
  const data=[...buckets].sort((a,b)=>a[0]-b[0]).map(([t,a])=>({t:t*900000,v:a.reduce((s,x)=>s+x,0)/a.length}));
  const max=key==='cpu'?100:Math.max(...data.map(x=>x.v),1),first=data[0].t,last=data[data.length-1].t;
  const points=data.map((p,i)=>`${i===0||p.t-data[i-1].t>1800000?'M':'L'}${(12+(p.t-first)/(last-first||1)*576).toFixed(1)},${(112-p.v/max*96).toFixed(1)}`).join(' ');
  const clock=t=>new Date(t).toLocaleString('ru',{day:'2-digit',month:'2-digit',hour:'2-digit',minute:'2-digit'});
  return `<section class="node-chart"><div class="node-chart-head"><h3>${label}</h3><span>макс. ${format(Math.max(...rows.map(x=>x[key])))}</span></div><svg viewBox="0 0 600 128" role="img" aria-label="${label}: история за выбранный период"><path d="M12 16H588M12 64H588M12 112H588" class="node-chart-grid"/><path d="${points}" class="node-chart-line"/>${data.length===1?`<circle cx="12" cy="${112-data[0].v/max*96}" r="3" class="node-chart-point"/>`:''}</svg><div class="node-chart-axis"><span>${clock(first)}</span><span>${clock(last)}</span></div></section>`;
}
function nodeBgpPanel(n){
  return `<section class="node-bgp"><div class="node-bgp-heading"><div><span class="node-bgp-label">${I('globe',15)} bgp.tools</span><h3>Сеть и маршрутизация</h3><p class="node-muted">Проверяем адрес ноды <code>${esc(n.addr)}</code>. Для домена определим его публичные IP.</p></div><button class="btn primary" data-bgp-check>${I('refresh',14)} Проверить сеть</button></div><div data-bgp-result aria-live="polite"><div class="node-bgp-empty">${I('globe',30)}<div><b>Узнайте, в какой сети работает сервер</b><p>ASN, оператор и BGP-префиксы появятся после проверки.</p></div></div></div><p class="node-bgp-note">Данные о сети не подтверждают работу VPN. Состояние агента и Xray смотрите в «Диагностике». Если адрес ведёт на прокси, здесь будет сеть прокси.</p></section>`;
}
function nodeBgpResult(data){
  const time=new Date(data.checked_at).toLocaleString(currentLang());
  return `<div class="node-bgp-meta"><span>${I('clock',13)} Проверено ${esc(time)}</span><span>Обновление не чаще раза в минуту</span></div>${data.addresses.map(item=>`<article class="node-bgp-address"><div class="node-bgp-address-head"><span class="bdg neutral">${item.ip.includes(':')?'IPv6':'IPv4'}</span><code>${esc(item.ip)}</code></div>${item.routes.length?item.routes.map(route=>`<div class="node-bgp-route"><div class="node-bgp-operator"><a href="https://bgp.tools/as/${encodeURIComponent(route.asn)}" target="_blank" rel="noopener noreferrer">AS${esc(route.asn)} ${I('external',13)}</a><strong>${esc(route.name||'Имя оператора не указано')}</strong></div><dl class="node-bgp-facts"><div><dt>BGP-префикс</dt><dd><code>${esc(route.prefix)}</code></dd></div><div><dt>Страна регистрации сети</dt><dd>${esc(route.country_code||'Не указана')}</dd></div><div><dt>Реестр адресов</dt><dd>${esc(route.registry||'Не указан')}</dd></div></dl><a class="btn sm" href="https://bgp.tools/prefix/${route.prefix.split('/').map(encodeURIComponent).join('/')}" target="_blank" rel="noopener noreferrer">Маршруты и RPKI ${I('external',13)}</a></div>`).join(''):'<div class="node-bgp-route"><b>Нет данных об анонсе</b><p class="node-muted">bgp.tools не определил ASN для этого IP. Это не проверка доступности сервера.</p></div>'}</article>`).join('')}${data.truncated?'<p class="node-muted">У домена больше четырёх IP. Показаны первые четыре адреса.</p>':''}`;
}
function bindNodeBgp(layer,n){
  const button=layer.querySelector('[data-bgp-check]'),result=layer.querySelector('[data-bgp-result]');
  button.onclick=async()=>{
    if(button.disabled)return;
    button.disabled=true;result.setAttribute('aria-busy','true');button.innerHTML=I('refresh',14)+' Проверяем…';result.innerHTML='<p class="node-muted">Получаем сведения из bgp.tools…</p>';
    try{
      const data=await API.call('/api/nodes/'+n.id+'/bgp');
      if(!layer.isConnected)return;
      result.innerHTML=nodeBgpResult(data);
    }catch(e){
      if(layer.isConnected)result.innerHTML='<div class="node-bgp-error" role="alert">'+I('alert',18)+'<div><b>Проверка не завершена</b><p>'+esc(e.message)+'</p></div></div>';
    }finally{
      button.disabled=false;button.innerHTML=I('refresh',14)+' Проверить снова';result.removeAttribute('aria-busy');
    }
  };
}

function openNodeOverview(n,initialTab='overview'){
  if(!n)return;
  const health=nodeHealth(n),hosts=nodeHosts(n);
  openModal({title:esc(n.name),sub:`${esc(n.addr)} · ${esc(n.cc)} · ${esc(n.infraProvider||'Хостер не указан')}`,icon:'server',size:'xl',initialFocus:'title',
    body:`<div class="node-detail"><div class="node-detail-status"><span class="bdg ${health.tone}"><span class="dot"></span>${health.label}</span><span>${esc(health.detail)}</span><small>Данные отчёта: ${n.lastSeen?fmtDT(n.lastSeen):'ещё не было'}</small></div>
      ${!n.lastSeen?`<div class="node-install-banner"><div><b>Агент ещё не подключён</b><p>Получите команду и выполните её на сервере ноды по SSH.</p></div><button class="btn primary" data-node-action="installation">${I('terminal',16)} Команда установки</button></div>`:''}<div class="node-detail-tabs" role="tablist" aria-label="Информация о сервере">${[['overview','Обзор'],['history','Графики'],['network','Сеть и BGP'],['access','Хосты и доступ'],['diagnostics','Диагностика']].map(([key,title],i)=>`<button role="tab" id="node-tab-${key}" aria-controls="node-pane-${key}" aria-selected="${i===0}" tabindex="${i===0?0:-1}" data-node-tab="${key}">${title}</button>`).join('')}</div>
      <div id="node-pane-overview" role="tabpanel" aria-labelledby="node-tab-overview" class="node-pane"><div class="node-detail-resources"><section><h3>Процессор</h3><b>${nodePercent(n.cpu)}</b><p>${esc(n.cpuModel||'Модель не передана')}</p>${nodeMeter(n.cpu,'Загрузка CPU')}</section><section><h3>Память</h3><b>${nodeMeasure(n.memUsed,fmtBytes)}</b><p>из ${nodeMeasure(n.memTotal,fmtBytes)}</p>${nodeMeter(n.ram,'Загрузка памяти')}</section><section><h3>Трафик сегодня</h3><b>${fmtBytes(n.todayBytes)}</b><p>Счётчики VPN · день по UTC</p><span class="node-muted">Множитель учёта ×${n.multiplier??1}</span></section></div><div class="node-detail-columns"><section><h3>Система и сеть</h3><dl class="node-facts">${nodeFact('Адрес агента',`<code>${esc(n.addr)}:${n.port}</code><button class="btn ghost icon-only" data-copy="${esc(n.addr+':'+n.port)}" aria-label="Скопировать адрес">${I('copy',13)}</button>`)}${nodeFact('Работает без перезагрузки',nodeMeasure(n.uptimeSec,fmtUptime))}${nodeFact('Ядро системы',esc(n.kernel||'—'))}${nodeFact('Интерфейс',esc(n.iface||'—'))}${nodeFact('Приём / отдача',nodeMeasure(n.rxBps,fmtBps)+' / '+nodeMeasure(n.txBps,fmtBps))}${nodeFact('Load average · 1 / 5 / 15 мин.',(n.la||[]).map(x=>x==null?'—':x.toFixed(2)).join(' / '))}</dl></section><section><h3>Конфигурация и расходы</h3><dl class="node-facts">${nodeFact('Профиль',esc(n.profile))}${nodeFact('Xray / агент',esc(n.xray)+' / '+esc(n.agent))}${nodeFact('Активные инбаунды',n.inbounds.map(esc).join(', ')||'Не выбраны')}${nodeFact('Учёт трафика',n.trackTraffic?'Включён':'Выключен')}${nodeFact('Аренда в месяц',fmtMoney(minorToUnits(n.costMinor,DB.currency)))}${nodeFact('День оплаты',n.billDay?esc(n.billDay)+' число':'Не задан')}</dl></section></div></div>
      <div id="node-pane-history" role="tabpanel" aria-labelledby="node-tab-history" class="node-pane" hidden><div class="node-history-head"><div><h3>История работы сервера</h3><p class="node-muted">Средние значения за 15 минут. Пропуски не заменяются нулями.</p></div><select class="inp" id="nodeHistoryPeriod" aria-label="Период графиков"><option value="24">24 часа</option><option value="72">3 дня</option><option value="168">7 дней</option></select></div><div id="nodeHistory" aria-live="polite">Загружаем историю…</div></div>
      <div id="node-pane-network" role="tabpanel" aria-labelledby="node-tab-network" class="node-pane" hidden>${nodeBgpPanel(n)}</div>
      <div id="node-pane-access" role="tabpanel" aria-labelledby="node-tab-access" class="node-pane" hidden><h3>Хосты этого профиля и инбаундов</h3><p class="node-muted">Это доступные связи конфигурации. Адрес хоста может указывать на прокси или балансировщик, поэтому совпадение не доказывает прохождение трафика через этот сервер.</p><div class="node-access-path"><span>Профиль <b>${esc(n.profile)}</b></span>${I('chevR',16)}<span>Инбаунды <b>${n.inbounds.length}</b></span>${I('chevR',16)}<span>Хосты <b>${hosts.length}</b></span></div>${hosts.length?`<div class="tbl-wrap"><table class="tbl"><thead><tr><th>Хост</th><th>Адрес</th><th>Инбаунд</th><th>Состояние</th></tr></thead><tbody>${hosts.map(h=>`<tr><td>${esc(h.remark)}</td><td><code>${esc(h.addr)}:${h.port}</code></td><td>${esc(h.inbound)}</td><td><span class="bdg ${h.enabled?'ok':'neutral'}">${h.enabled?'Включён':'Выключен'}</span></td></tr>`).join('')}</tbody></table></div>`:'<div class="empty"><b>Связанных хостов пока нет</b><span>Создайте хост для выбранного профиля и инбаунда.</span></div>'}<button class="btn" data-node-action="hosts">Открыть хосты ${I('chevR',14)}</button></div>
      <div id="node-pane-diagnostics" role="tabpanel" aria-labelledby="node-tab-diagnostics" class="node-pane" hidden>${nodeDiagnostic(n)}${n.selfsteal?`<section class="node-selfsteal"><h3>Сайт на ноде</h3>${SelfstealUI.status(n.selfsteal)}</section>`:''}<div class="node-detail-actions"><button class="btn" data-node-action="installation">${I('terminal',14)} Команда установки</button><button class="btn" data-node-action="plugins">${I('shield',14)} Фильтры и защита</button><button class="btn" data-node-action="blocks">${I('ban',14)} Журнал блокировок</button><button class="btn" data-node-action="restart">${I('refresh',14)} Перезапустить Xray</button></div></div></div>`,
    footer:`<button class="btn" data-node-action="menu">${I('more',14)} Все действия</button><div class="spacer"></div><button class="btn" data-close>Закрыть</button><button class="btn primary" data-node-action="edit">${I('settings',14)} Настроить сервер</button>`,
    onMount(layer,close){
      layer.querySelector('.modal').classList.add('node-overview-modal');
      let historyLoaded=false,request=0;
      const loadHistory=async()=>{const id=++request,el=layer.querySelector('#nodeHistory');el.textContent='Загружаем историю…';try{const data=await API.call('/api/nodes/'+n.id+'/metrics?hours='+layer.querySelector('#nodeHistoryPeriod').value);if(id!==request||!layer.isConnected)return;el.innerHTML=nodeHistoryChart(data,'cpu','Загрузка CPU',x=>Math.round(x)+'%')+nodeHistoryChart(data,'rx_bps','Входящий трафик',fmtBps)+nodeHistoryChart(data,'tx_bps','Исходящий трафик',fmtBps);}catch(e){if(id===request)el.textContent='Не удалось загрузить графики: '+e.message;}};
      const tabs=[...layer.querySelectorAll('[data-node-tab]')];
      const select=b=>{tabs.forEach(t=>{const on=t===b;t.setAttribute('aria-selected',String(on));t.tabIndex=on?0:-1;layer.querySelector('#node-pane-'+t.dataset.nodeTab).hidden=!on;});if(b.dataset.nodeTab==='history'&&!historyLoaded){historyLoaded=true;loadHistory();}};
      tabs.forEach((b,i)=>{b.onclick=()=>select(b);b.onkeydown=e=>{const j=e.key==='ArrowRight'?(i+1)%tabs.length:e.key==='ArrowLeft'?(i+tabs.length-1)%tabs.length:-1;if(j>=0){e.preventDefault();select(tabs[j]);tabs[j].focus();}};});
      bindNodeBgp(layer,n);
      const rail=layer.querySelector('.node-detail-tabs');
      const revealSelected=()=>{if(!layer.isConnected){tabResize.disconnect();tabText.disconnect();return;}const tab=tabs.find(b=>b.getAttribute('aria-selected')==='true');if(!tab)return;const r=rail.getBoundingClientRect(),b=tab.getBoundingClientRect();if(b.right>r.right)rail.scrollLeft+=b.right-r.right+8;else if(b.left<r.left)rail.scrollLeft-=r.left-b.left+8;};
      const tabResize=new ResizeObserver(revealSelected);tabResize.observe(rail);tabs.forEach(b=>tabResize.observe(b));
      const tabText=new MutationObserver(()=>requestAnimationFrame(revealSelected));tabText.observe(rail,{childList:true,subtree:true,characterData:true,attributes:true,attributeFilter:['aria-selected']});
      const initial=tabs.find(b=>b.dataset.nodeTab===initialTab);if(initial){select(initial);requestAnimationFrame(revealSelected);}
      layer.querySelector('#nodeHistoryPeriod').onchange=loadHistory;
      layer.querySelectorAll('[data-node-action]').forEach(b=>b.onclick=()=>{const action=b.dataset.nodeAction;if(action==='menu'){nodeMenu(b,n);return;}close();if(action==='installation')nodeInstallationCommand(n);if(action==='edit')editNodeDrawer(n);if(action==='plugins')pluginsDrawer(n);if(action==='blocks')blocksDrawer(n);if(action==='restart')restartNodeEngine(n);if(action==='hosts')navigate('hosts');});
    }
  });
}
PAGES.nodes.render=()=>renderNodeConsole(false);
PAGES.nodes.bind=bindNodeConsole;
PAGES['nodes-metrics'].render=()=>renderNodeConsole(true);
PAGES['nodes-metrics'].bind=bindNodeConsole;

function trafficDates(count){return Array.from({length:count},(_,i)=>new Date(Date.now()-(count-1-i)*86400000).toISOString().slice(0,10));}
function trafficTotal(node){return Object.values(node.days||{}).reduce((sum,v)=>sum+Number(v||0),0);}
PAGES['nodes-stats'].render=()=>`<div class="node-console traffic-console"><div class="page-head"><div><h1>Трафик сети</h1><div class="desc">Сравнивайте потребление серверов и находите изменения по дням. Данные из счётчиков VPN, календарные дни по UTC.</div></div><div class="actions"><select class="inp" id="trafficPeriod" aria-label="Период статистики"><option value="14">14 дней</option><option value="30" selected>30 дней</option><option value="90">90 дней</option></select><button class="btn" id="trafficReload">${I('refresh',14)} Обновить</button></div></div><div id="trafficContent" aria-live="polite"><div class="empty"><b>Загружаем статистику сети…</b></div></div></div>`;
PAGES['nodes-stats'].bind=root=>{
  let data=null,request=0,selected='';
  const draw=()=>{
    const el=root.querySelector('#trafficContent'),days=trafficDates(Number(data.days)||30),all=data.nodes||[],nodes=selected?all.filter(n=>String(n.id)===selected):all;
    if(!all.length){el.innerHTML='<div class="card empty"><b>Статистики пока нет</b><span>После подключения агента здесь появятся реальные счётчики трафика.</span></div>';return;}
    const points=days.map(day=>({day,value:nodes.some(n=>n.days?.[day]!=null)?nodes.reduce((s,n)=>s+Number(n.days?.[day]||0),0):null}));
    const total=nodes.reduce((s,n)=>s+trafficTotal(n),0),max=Math.max(...points.map(p=>p.value||0),1),measured=points.filter(p=>p.value!=null),peak=measured.reduce((best,p)=>!best||p.value>best.value?p:best,null);
    const ranking=[...nodes].sort((a,b)=>trafficTotal(b)-trafficTotal(a));
    const dateText=day=>new Date(day+'T12:00:00Z').toLocaleDateString('ru',{day:'numeric',month:'short',timeZone:'UTC'});
    el.innerHTML=`<section class="traffic-summary" aria-label="Итоги периода"><div><span>Передано за период</span><b>${fmtBytes(total)}</b><small>${selected?esc(nodes[0]?.name||''):'По всем серверам'}</small></div><div><span>Самый активный день</span><b>${peak?dateText(peak.day):'—'}</b><small>${peak?fmtBytes(peak.value):'Нет измерений'}</small></div><div><span>Среднее за день с данными</span><b>${measured.length?fmtBytes(total/measured.length):'—'}</b><small>${measured.length} из ${days.length} дней с отчётами</small></div></section>
      <div class="traffic-analysis"><section class="traffic-timeline"><div class="traffic-section-head"><div><h2>Потребление по дням</h2><p>Значения доступны в таблице по дням, в том числе с телефона.</p><button class="btn sm traffic-exact" id="trafficExact">Точные значения ${I('chevD',13)}</button></div><select class="inp" id="trafficNode" aria-label="Сервер для графика"><option value="">Вся сеть</option>${all.map(n=>`<option value="${n.id}" ${String(n.id)===selected?'selected':''}>${esc(n.name)}</option>`).join('')}</select></div><div class="traffic-plot"><div class="traffic-scale"><span>${fmtBytes(max)}</span><span>${fmtBytes(max/2)}</span><span>0</span></div><div class="traffic-bars" role="img" aria-label="Трафик по дням; точные значения доступны в таблице ниже">${points.map(p=>`<div class="traffic-bar-slot" title="${dateText(p.day)}: ${p.value==null?'нет отчёта':fmtBytes(p.value)}"><span class="traffic-bar ${p.value==null?'missing':''}" style="height:${p.value==null?'2px':Math.max(1,p.value/max*100)+'%'}"></span></div>`).join('')}</div></div><div class="traffic-time-axis"><span>${dateText(days[0])}</span><span>${dateText(days[Math.floor(days.length/2)])}</span><span>${dateText(days[days.length-1])}</span></div><p class="traffic-legend"><span class="traffic-legend-key"></span>Переданные данные <span class="traffic-legend-key missing"></span>Нет отчёта за день</p></section>
      <section class="traffic-ranking"><h2>Распределение трафика</h2><p>Доля каждого сервера в выбранном периоде</p>${ranking.slice(0,6).map(n=>{const amount=trafficTotal(n),pct=total?amount/total*100:0;return `<div class="traffic-rank-row"><div><span>${ccChip(n.country_code)} ${esc(n.name)}</span><b>${fmtBytes(amount)}</b></div><meter min="0" max="100" value="${pct}" aria-label="Доля ${esc(n.name)}">${pct.toFixed(1)}%</meter><small>${pct.toFixed(1)}% от выбранного трафика</small></div>`;}).join('')}</section></div>
      <section class="fleet-inventory traffic-table-section"><div class="fleet-inventory-head"><h2>Сравнение серверов</h2><button class="btn sm" id="trafficCsv">${I('download',14)} Скачать CSV</button></div><div class="fleet-table-scroll"><table class="tbl traffic-data"><thead><tr><th>Сервер</th><th>Всего за период</th><th>Сегодня · UTC</th><th>Пиковый день</th><th>Дней с данными</th></tr></thead><tbody>${ranking.map(n=>{const values=Object.entries(n.days||{}).filter(([d])=>days.includes(d));const top=values.sort((a,b)=>b[1]-a[1])[0];return `<tr><td><button class="node-identity" data-traffic-node="${n.id}">${ccChip(n.country_code)}<span><b>${esc(n.name)}</b></span></button></td><td class="num"><b>${fmtBytes(trafficTotal(n))}</b></td><td class="num">${n.days?.[days[days.length-1]]==null?'Нет отчёта':fmtBytes(n.days[days[days.length-1]])}</td><td>${top?fmtBytes(top[1])+`<span class="node-secondary">${dateText(top[0])}</span>`:'—'}</td><td class="num">${values.length} / ${days.length}</td></tr>`;}).join('')}</tbody></table></div><p class="fleet-footnote">Неактивные дни и отсутствие отчёта — разные состояния. Пустое значение означает, что счётчик за этот день не сохранён.</p></section>
      <details class="traffic-daily-details" id="trafficDaily"><summary>Все значения по дням</summary><div class="fleet-table-scroll"><table class="tbl traffic-data"><thead><tr><th>Дата · UTC</th>${nodes.map(n=>`<th>${esc(n.name)}</th>`).join('')}<th>Всего</th></tr></thead><tbody>${[...points].reverse().map(p=>`<tr><td>${dateText(p.day)}</td>${nodes.map(n=>`<td class="num">${n.days?.[p.day]==null?'—':fmtBytes(n.days[p.day])}</td>`).join('')}<td class="num">${p.value==null?'—':fmtBytes(p.value)}</td></tr>`).join('')}</tbody></table></div></details>`;
    el.querySelector('#trafficExact').onclick=()=>{const detail=el.querySelector('#trafficDaily');detail.open=true;detail.querySelector('summary').focus();detail.scrollIntoView({block:'start',behavior:'auto'});};
    el.querySelector('#trafficNode').onchange=e=>{selected=e.target.value;draw();};
    el.querySelectorAll('[data-traffic-node]').forEach(b=>b.onclick=()=>{const n=DB.nodes.find(n=>n.id===b.dataset.trafficNode);if(n)openNodeOverview(n);else toast('Сервер удалён; исторические данные сохранены','warn');});
    el.querySelector('#trafficCsv').onclick=()=>{const cell=x=>'"'+String(x??'').replace(/^[=+@-]/,"'$&").replaceAll('"','""')+'"';const lines=[['Дата UTC','Сервер','Байты'],...nodes.flatMap(n=>days.map(day=>[day,n.name,n.days?.[day]??'']))];const url=URL.createObjectURL(new Blob(['\ufeff'+lines.map(row=>row.map(cell).join(';')).join('\r\n')],{type:'text/csv;charset=utf-8'}));const a=document.createElement('a');a.href=url;a.download='node-traffic-'+days[0]+'-'+days[days.length-1]+'.csv';a.click();setTimeout(()=>URL.revokeObjectURL(url),1000);};
  };
  const load=async()=>{const id=++request;root.querySelector('#trafficContent').innerHTML='<div class="empty"><b>Загружаем статистику сети…</b></div>';try{const result=await API.call('/api/nodes/traffic?days='+root.querySelector('#trafficPeriod').value);if(id!==request||!root.isConnected)return;if(!Array.isArray(result.nodes))throw new Error('Сервер вернул некорректный ответ');data=result;draw();}catch(e){if(id===request&&root.isConnected)root.querySelector('#trafficContent').innerHTML='<div class="empty"><b>Статистика недоступна</b><span>'+esc(e.message)+'</span><span>Нажмите «Обновить», чтобы повторить.</span></div>';}};
  root.querySelector('#trafficPeriod').onchange=load;root.querySelector('#trafficReload').onclick=load;load();
};
