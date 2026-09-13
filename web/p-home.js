/* ═══════════ PAGES: home, login, 404 ═══════════ */
'use strict';

/* ── КОМАНДНЫЙ ЦЕНТР ── */

/* Выбранный ряд и период живут вне render(): перерисовка не должна
   сбрасывать то, что человек только что выбрал. */
let homeMetric = 'revenue';
let homeDays = 30;
function homeSet(metric, days){
  if (metric) homeMetric = metric;
  if (days) homeDays = +days;
  const main = document.getElementById('main');
  if (main) { main.innerHTML = PAGES.home.render(); translateNode(main); enhancePanelUI(main); }
}

registerPage({
  id:'home', title:'Командный центр', group:'Обзор', icon:'dash',
  render(){
    const d = DB.dashboard || {clients:{},revenue:{},traffic:{},nodes:{},revenue_series:[],traffic_series:[]};
    const cur = d.revenue.currency || DB.currency || 'USD';
    const money = m => fmtMoney(minorToUnits(m||0, cur), cur);
    const bytes = b => fmtBytes(b||0);

    /* Ряды приходят за 30 дней; «7 дней» — это хвост того же ряда, а не
       отдельный запрос. Просить у сервера то, что уже пришло, незачем. */
    const cut = arr => (arr||[]).slice(-homeDays);
    const revRow = cut(d.revenue_series);
    const trfRow = cut(d.traffic_series);
    const dayLbl = iso => { const p = String(iso).split('-'); return p.length===3 ? `${p[2]}.${p[1]}` : ''; };

    /* Изменение за период — половина ряда против предыдущей половины.
       Это то же сравнение, которое человек сделал бы глазами, и оно
       честное: если данных меньше двух половин, изменение не считаем. */
    const delta = vals => {
      if (!vals || vals.length < 4) return null;
      const h = Math.floor(vals.length/2);
      const a = vals.slice(0,h).reduce((x,y)=>x+y,0);
      const b = vals.slice(h).reduce((x,y)=>x+y,0);
      if (!a) return b ? null : 0;
      return Math.round((b-a)/a*100);
    };
    const deltaBdg = v => v===null ? '' :
      `<span class="bdg ${v>0?'ok':v<0?'err':'neutral'} sm">${v>0?'↑':v<0?'↓':'='} ${Math.abs(v)}%</span>`;

    const revVals = revRow.map(x => minorToUnits(x.amount_minor||0, cur));
    const trfVals = trfRow.map(x => x.bytes||0);
    const isRev = homeMetric === 'revenue';

    /* Трафик по нодам — из того же списка нод, что и на странице «Ноды».
       Показываем шесть верхних: дальше идут доли процента. */
    const topNodes = [...(DB.nodes||[])]
      .filter(n => (n.todayBytes||0) > 0)
      .sort((a,b) => (b.todayBytes||0) - (a.todayBytes||0))
      .slice(0,6)
      .map(n => ({ label:n.name, value:n.todayBytes||0, icon:flag(n.cc), color:'var(--info)' }));

    const st = d.clients || {};
    const statuses = [
      {label:'Активные',  value:st.active||0,   color:'var(--ok)'},
      {label:'Истёкшие',  value:st.expired||0,  color:'var(--err)'},
      {label:'На лимите', value:st.limited||0,  color:'var(--warn)'},
      {label:'Отключены', value:st.disabled||0, color:'var(--text-3)'},
    ];

    // Лента строится из реальных платежей и состояния нод — выдуманных событий нет.
    const timeOf = iso => iso ? new Date(iso).toLocaleTimeString(currentLang(),{hour:'2-digit',minute:'2-digit'}) : '';
    const feed = [
      ...DB.payments.slice(0, 6).map(p => [
        p.status === 'success' ? 'pay' : p.status === 'failed' ? 'err' : 'warn',
        `${({success:'Оплата',pending:'Ожидает оплаты',failed:'Ошибка оплаты',canceled:'Платёж отменён',refunded:'Возврат'})[p.status] || 'Платёж'} · ${p.item}`,
        `@${p.user} · ${p.method}${p.err ? ' · ' + p.err : ''}`,
        timeOf(p.at),
        p.status === 'success' ? '+' + fmtMoney(p.amount, p.currency) : '',
      ]),
      ...DB.nodes.filter(n => n.status !== 'online').map(n => [
        'err', `Нода ${n.name} ${n.status}`, `${n.cc} · ${n.addr}`, '', '',
      ]),
    ].slice(0, 8);
    const evIc = {
      pay:['card','var(--ok)'], user:['user','var(--info)'],
      warn:['alert','var(--warn)'], err:['ban','var(--err)'],
    };

    const kpi = (icon, label, value, note, spark_, badge) => `
      <div class="card kpi">
        <div class="label">${I(icon,13)} ${label}</div>
        <div class="kpi-val num">${value}</div>
        <div class="kpi-note">${note||''}${badge||''}</div>
        ${spark_||''}
      </div>`;

    return `
    <div class="page-head"><div><h1>Командный центр</h1><div class="desc">Продажи, клиенты и состояние сети. Выберите показатель, чтобы увидеть динамику за нужный период.</div></div><div class="actions"><button class="btn" onclick="navigate('getting-started')">${I('activity',14)} Запуск сервиса</button><button class="btn section-help" onclick="sectionHelp('home')">${I('help',14)} Инструкция</button></div></div>
    <div class="grid g4">
      ${kpi('activity','Выручка за месяц', money(d.revenue.month_minor),
            `сегодня ${money(d.revenue.today_minor)} · ${d.revenue.today_count||0} платежей${(d.revenue_by_currency||[]).filter(w=>w.currency!==cur).map(w=>'<br>'+fmtMoney(minorToUnits(w.month_minor,w.currency),w.currency)+' за месяц').join('')}`,
            sparkline(revVals,{color:'var(--accent)'}), deltaBdg(delta(revVals)))}
      ${kpi('history','Активные подписки', fmtN(st.active||0),
            `из ${fmtN(st.total||0)} клиентов`, '', '')}
      ${kpi('pulse','Трафик сегодня', bytes(d.traffic.today_bytes),
            `за 30 дней — ${bytes(d.traffic.month_bytes)}`,
            sparkline(trfVals,{color:'var(--info)'}), deltaBdg(delta(trfVals)))}
      ${kpi('server','Ноды в строю', `${d.nodes.online||0} / ${d.nodes.total||0}`,
            '', '',
            `<span class="bdg ${(d.nodes.online||0)===(d.nodes.total||0)?'ok':'warn'}">${(d.nodes.online||0)===(d.nodes.total||0)?'все в строю':'есть offline'}</span>`)}
    </div>

    <div class="grid home-layout">
      <div class="home-primary">
        <div class="card">
          <div class="section-h" style="margin:0 0 var(--s-3)">
            <h2>${isRev ? 'Выручка по дням' : 'Трафик по дням'}</h2>
            <span>${homeDays} дней</span>
            <div class="spacer"></div>
            <div class="seg">
              <button class="${isRev?'on':''}" onclick="homeSet('revenue')">Выручка</button>
              <button class="${isRev?'':'on'}" onclick="homeSet('traffic')">Трафик</button>
            </div>
            <div class="seg">
              <button class="${homeDays===7?'on':''}"  onclick="homeSet(null,7)">7д</button>
              <button class="${homeDays===30?'on':''}" onclick="homeSet(null,30)">30д</button>
            </div>
          </div>
          ${isRev
            ? areaChart(revVals, {color:'var(--accent)', labels:revRow.map(x=>dayLbl(x.day)),
                                  fmt:v => fmtMoney(v, cur)})
            : areaChart(trfVals, {color:'var(--info)', labels:trfRow.map(x=>dayLbl(x.day)),
                                  fmt:v => fmtBytes(v)})}
        </div>

        <div class="grid g2" style="align-items:start">
          <div class="card">
            <div class="section-h" style="margin:0 0 var(--s-3)"><h2>Клиенты по статусам</h2>
              <span>всего ${fmtN(st.total||0)}</span></div>
            <div class="home-statuses">
              ${donut(statuses, {center:{top:fmtN(st.active||0), sub:'активны'}})}
              <div class="donut-legend">
                ${statuses.map(s2=>`<div class="dl-row"><i style="background:${s2.color}"></i>
                  <span>${s2.label}</span><b>${fmtN(s2.value)}</b></div>`).join('')}
              </div>
            </div>
          </div>

          <div class="card">
            <div class="section-h" style="margin:0 0 var(--s-3)"><h2>Трафик по нодам</h2><span>сегодня</span></div>
            ${topNodes.length
              ? rankBars(topNodes, {fmt:v => fmtBytes(v)})
              : chartEmpty(120, 'Сегодня ноды ещё не передавали трафик')}
          </div>
        </div>
      </div>

      <div class="card feed">
        <div class="feed-head">
          <h3>Поток событий</h3>
          <span class="sub-note">Последние платежи и ноды</span>
        </div>
        <div class="feed-list">
          ${feed.length ? feed.map(e=>`
            <div class="feed-row">
              <div class="feed-ic" style="color:${evIc[e[0]][1]};background:color-mix(in srgb, ${evIc[e[0]][1]} var(--tint), transparent)">${I(evIc[e[0]][0],14)}</div>
              <div style="min-width:0;flex:1">
                <b>${esc(e[1])} ${e[4]?`<span class="num" style="color:var(--ok-ink)">${e[4]}</span>`:''}</b>
                <p>${esc(e[2])}</p>
              </div>
              <span class="num feed-time">${e[3]}</span>
            </div>`).join('')
            : `<div class="empty" style="padding:34px 20px"><b>Событий пока нет</b>
                 <span>Здесь появятся оплаты и аварии нод.</span></div>`}
        </div>
        <div class="feed-actions">
          <button class="btn primary sm" onclick="navigate('nodes')">${I('plus',12)} Добавить ноду</button>
          <button class="btn sm" onclick="navigate('broadcasts')">${I('send',12)} Рассылка</button>
          <button class="btn sm" onclick="navigate('users')">${I('user',12)} Создать клиента</button>
          <button class="btn sm" onclick="navigate('tariffs')">${I('layers',12)} Новый тариф</button>
        </div>
      </div>
    </div>`;
  },
});

/* ── LOGIN ── */
registerPage({
  id:'login', title:'Вход', nav:false, bare:true,
  render(){
    return `
    <div class="auth-card">
      <div class="logo">
        ${logoMark(44)}
        <div><div class="logo-name">STEALTHNET</div><div class="logo-sub">CONTROL PANEL</div></div>
      </div>
      <div id="loginStep1">
        ${/* Кнопка одна и настоящая. Раньше их было две: сверху заглушка
             «Passkey появится в следующей версии», снизу — рабочая, но
             спрятанная. Человек жал верхнюю и уходил ни с чем. */''}
        <div id="loginPkBox" style="display:none">
          <button class="btn" style="width:100%;justify-content:center;margin-bottom:14px" id="loginPk">${I('fingerprint',15)} Войти по ключу</button>
          <div style="display:flex;align-items:center;gap:12px;margin:4px 0 14px;color:var(--text-3);font-size:11px"><i style="flex:1;height:1px;background:var(--border)"></i>или<i style="flex:1;height:1px;background:var(--border)"></i></div>
        </div>
        <div class="field"><label>Логин</label><input class="inp" id="loginU" placeholder="admin" autocomplete="username"></div>
        <div class="field"><label>Пароль</label><input class="inp" id="loginP" type="password" placeholder="••••••••" autocomplete="current-password"></div>
        <div class="field" id="loginCodeBox" style="display:none"><label>Код из приложения</label>
          <input class="inp mono" id="loginCode" inputmode="numeric" maxlength="6" placeholder="123456"
            autocomplete="one-time-code"></div>
        <div id="loginErr" style="display:none;background:color-mix(in srgb, var(--err) 10%, transparent);border:1px solid color-mix(in srgb, var(--err) 30%, transparent);color:var(--err-ink);border-radius:9px;padding:9px 12px;font-size:12px;margin-bottom:12px"></div>
        <button class="btn primary" style="width:100%;justify-content:center" id="loginBtn">Войти</button>
      </div>
      <p style="font-size:10.5px;color:var(--text-3);text-align:center;margin-top:20px" class="mono">STEALTHNET v0.1.0 · стенд</p>
    </div>`;
  },
  bind(root){
    const err = (msg)=>{
      const box = root.querySelector('#loginErr');
      box.textContent = msg; box.style.display = 'block';
    };
    const doLogin = async ()=>{
      const btn = root.querySelector('#loginBtn');
      const u = root.querySelector('#loginU').value.trim();
      const p = root.querySelector('#loginP').value;
      const code = root.querySelector('#loginCode').value.trim();
      if(!u || !p) return err('Введите логин и пароль');
      btn.disabled = true; btn.textContent = 'Проверяем…';
      try{
        const res = await API.login(u, p, code);
        API.setToken(res.token);
        await loadDB();
        authed = true;
        navigate('home');
        toast(`Добро пожаловать, ${res.admin.username}!`);
      }catch(e){
        // Просьбу ввести код показываем полем, а не ошибкой: человек
        // ввёл верный пароль, и сообщение «неверный пароль» тут врёт.
        if (/код/i.test(e.message)) {
          root.querySelector('#loginCodeBox').style.display = '';
          root.querySelector('#loginCode').focus();
        }
        err(e.message === 'нужен вход' ? 'Неверный логин или пароль' : e.message);
        btn.disabled = false; btn.textContent = 'Войти';
      }
    };
    root.querySelector('#loginBtn').addEventListener('click', doLogin);

    // Кнопку показываем, только если ключи вообще заведены: вести
    // человека к входу, которого у него нет, — тупик.
    const pk = root.querySelector('#loginPk');
    if (passkeysSupported()) {
      fetch('/api/auth/passkey/available')
        .then(r=>r.json())
        .then(r=>{ if (r.available) root.querySelector('#loginPkBox').style.display = ''; })
        .catch(()=>{});
    }

    pk.addEventListener('click', async ()=>{
      const u = root.querySelector('#loginU').value.trim();
      if (!u) return err('Введите логин — ключ привязан к учётной записи');
      pk.disabled = true;
      try {
        const start = await API.call('/api/auth/passkey', { method:'POST', body:{ username: u } });
        const cred = await navigator.credentials.get(decodeWebauthn(start.options));
        const res = await API.call('/api/auth/passkey/finish', { method:'POST',
          body:{ ticket: start.ticket, credential: encodeCredential(cred) } });
        API.setToken(res.token);
        await loadDB();
        authed = true;
        navigate('home');
        toast(`Добро пожаловать, ${res.admin.username}!`);
      } catch(e){
        const msg = String(e.message || e);
        err(/NotAllowed|abort/i.test(msg) ? 'Вход по ключу отменён' : msg);
        pk.disabled = false;
      }
    });
    root.querySelectorAll('.inp').forEach(i=>i.addEventListener('keydown', e=>{ if(e.key==='Enter') doLogin(); }));
  }
});

/* ── 404 ── */
registerPage({
  id:'404', title:'Не найдено', nav:false, bare:true,
  render(){
    return `<div class="auth-card" style="text-align:center">
      <div class="num" style="font-size:52px;font-weight:600;color:var(--accent)">404</div>
      <p style="color:var(--text-2);font-size:13.5px;margin:8px 0 20px">Такой страницы нет. Возможно, она переехала или ссылка устарела.</p>
      <button class="btn primary" onclick="navigate('home')" style="width:100%;justify-content:center">${I('home',14)} В командный центр</button>
    </div>`;
  }
});
