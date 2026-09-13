/* Subscription connection page settings. Delivery workspaces: subscription-studio.js. */
'use strict';

/* ── СТРАНИЦА ПОДПИСКИ ── */
registerPage({
  id:'subpage', title:'Страница подписки', group:'Подписка', icon:'browser',
  render(){
    return `
    <div class="page-head">
      <div><h1>Страница подписки</h1><div class="desc">Веб-страница по ссылке подписки: приложения под каждую платформу, кнопки установки и инструкции.</div></div>
      <div class="actions">
        <button class="btn" id="spPreview">${I('eye',14)} Открыть страницу</button>
        <button class="btn primary" id="spAdd">${I('plus',14)} Приложение</button>
      </div>
    </div>
    <div class="card" style="margin-bottom:var(--panel-gap,14px)"><div class="section-h"><h2>${I('sliders',14)} Общее оформление сервиса</h2><a class="btn sm" href="#/cabinet">Настроить бренд</a></div><p class="sub-note">Страница подписки использует название, светлый и тёмный логотипы, favicon и цвета из «Кабинет и Mini App». Приложения и инструкции настраиваются ниже. Заголовок профиля в VPN-приложении задаётся отдельно в настройках подписки.</p></div>
    <div class="grid g3" id="spList" style="align-items:start"></div>
    <div class="card" style="margin-top:var(--panel-gap,14px)">
      <div class="section-h" style="margin:0 0 10px"><h2>${I('info',14)} Как это работает</h2></div>
      <p class="sub-note">Страница открывается, когда ссылку подписки открывают в браузере — по правилу ответов для <code>mozilla</code>. Рекомендуемое приложение выбрано по умолчанию для устройства клиента. Другие приложения доступны в окне выбора; QR-код, локации и инструкции открываются отдельными кнопками.</p>
    </div>`;
  },
  async bind(root){
    const PLAT = { ios:'iOS', android:'Android', windows:'Windows', macos:'macOS', linux:'Linux' };
    const ICON = { ios:'smartphone', android:'smartphone', windows:'monitor', macos:'monitor', linux:'monitor' };

    const draw = async () => {
      let apps;
      try { apps = await API.call('/api/sub/page-apps'); }
      catch(e){ toast('Не загрузилось: '+e.message, 'err'); return; }

      const byPlat = {};
      for (const a of apps) (byPlat[a.platform] = byPlat[a.platform] || []).push(a);

      root.querySelector('#spList').innerHTML = Object.keys(PLAT).map(k=>{
        const list = byPlat[k] || [];
        return `
        <div class="card">
          <div class="section-h" style="margin:0 0 12px">
            <h2>${I(ICON[k],14)} ${PLAT[k]}</h2><span>${list.length} прил.</span>
            <div class="spacer"></div>
            <button class="btn ghost sm" aria-label="Добавить приложение для ${PLAT[k]}" data-add="${k}">${I('plus',12)}</button>
          </div>
          ${list.map(a=>`
            <div class="platform-app" style="${a.is_active?'':'opacity:.55'}">
              <div style="display:flex;align-items:center;gap:9px">
                <b style="font-size:12.5px;flex:1">${esc(a.name)}</b>
                ${a.has_icon ? '' : `<span class="bdg neutral" title="иконки нет — на странице будет запасная">${I('alert',10)}</span>`}
                <button class="btn ghost icon-only" aria-label="Действия: ${esc(a.name)}" data-amenu="${a.id}">${I('more',13)}</button>
              </div>
              ${a.deeplink ? `<div class="sub-note mono" style="margin-top:5px;word-break:break-all">${esc(a.deeplink)}</div>` : ''}
            </div>`).join('') || '<div class="sub-note">Приложений нет.</div>'}
        </div>`;
      }).join('');

      root.querySelectorAll('[data-add]').forEach(b=>b.addEventListener('click', ()=>appModal(null, b.dataset.add, draw)));
      root.querySelectorAll('[data-amenu]').forEach(b=>b.addEventListener('click', e=>{
        const a = apps.find(x=>String(x.id)===e.currentTarget.dataset.amenu);
        menu(e.currentTarget, [
          {label:'Редактировать', icon:'edit', onClick:()=>appModal(a, a.platform, draw)},
          {label:a.is_active?'Скрыть со страницы':'Показывать', icon:a.is_active?'ban':'eye', onClick:async ()=>{
            try {
              await API.call('/api/sub/page-apps/'+a.id, { method:'PATCH', body:{ is_active: !a.is_active } });
              toast(a.is_active ? 'Скрыто' : 'Показывается');
              await draw();
            } catch(e){ toast('Не получилось: '+e.message, 'err'); }
          }},
          '-',
          {label:'Удалить', icon:'trash', danger:true, onClick:()=>confirmModal({
            title:`Удалить «${esc(a.name)}»?`, text:'Приложение пропадёт со страницы подписки.', okText:'Удалить',
            onOk:async ()=>{
              try {
                await API.call('/api/sub/page-apps/'+a.id, { method:'DELETE' });
                toast('Удалено');
                await draw();
              } catch(e){ toast('Не удалилось: '+e.message, 'err'); }
            }})},
        ]);
      }));
    };

    root.querySelector('#spAdd').addEventListener('click', ()=>appModal(null, 'ios', draw));
    root.querySelector('#spPreview').addEventListener('click', async ()=>{
      // Открываем настоящую страницу настоящего клиента: рисованный
      // предпросмотр расходится с тем, что видит человек.
      const u = DB.users[0];
      if (!u) { toast('Нужен хотя бы один клиент', 'err'); return; }
      try {
        const r = await API.call('/api/sub-service');
        window.open(r.sub_public_url.replace(/\/$/, '') + '/' + u.shortUuid, '_blank');
      } catch(e){ toast('Не получилось: '+e.message, 'err'); }
    });

    await draw();
  }
});

function appModal(a, platform, onSaved){
  const isNew = !a;
  const PLAT = [['ios','iOS'],['android','Android'],['windows','Windows'],['macos','macOS'],['linux','Linux']];
  openModal({
    title: isNew ? 'Новое приложение' : 'Приложение · '+esc(a.name), icon:'smartphone', size:'sm',
    body:`
      <div class="two-col">
        <div class="field"><label>Платформа</label>
          <select class="inp" id="apPlat" ${isNew?'':'disabled'}>
            ${PLAT.map(([v,n])=>`<option value="${v}" ${(isNew?platform:a.platform)===v?'selected':''}>${n}</option>`).join('')}
          </select></div>
        <div class="field"><label>Название <span class="req">*</span></label>
          <input class="inp" id="apName" value="${isNew?'':esc(a.name)}" placeholder="Happ"></div>
      </div>
      <div class="field"><label>Ссылка установки</label>
        <input class="inp mono" id="apStore" value="${isNew?'':esc(a.store_url||'')}" placeholder="https://apps.apple.com/app/...">
        <div class="hint">Магазин приложений или страница загрузки.</div></div>
      <div class="field"><label>Схема добавления подписки</label>
        <input class="inp mono" id="apDeep" value="${isNew?'':esc(a.deeplink||'')}" placeholder="happ://add/{{URL}}">
        <div class="hint">Метка <code>{{URL}}</code> заменится ссылкой подписки — по ней работает кнопка «Добавить». Понимается и <code>{url}</code>.</div></div>
      <div class="field"><label>Иконка</label>
        <input class="inp mono" id="apIcon" value="${isNew?'':esc(a.icon_url||'')}" placeholder="https://…/icon.png">
        <div class="hint">Необязательно: у известных приложений иконка уже встроена в страницу.</div></div>
      <div class="field"><label>Инструкция</label>
        <textarea class="inp" id="apGuide" rows="3" placeholder="Установите приложение и нажмите «Добавить подписку»">${isNew?'':esc(a.guide||'')}</textarea></div>`,
    footer:`<div class="spacer"></div><button class="btn" data-close>Отмена</button>
            <button class="btn primary" data-save>${isNew?'Добавить':'Сохранить'}</button>`,
    onMount(layer, close){
      layer.querySelector('[data-save]').addEventListener('click', async ()=>{
        const name = layer.querySelector('#apName').value.trim();
        if (!name) { toast('Нужно название', 'err'); return; }
        const deeplink = layer.querySelector('#apDeep').value.trim();
        // Схема без {url} даёт кнопку, которая открывает приложение
        // и не передаёт ему подписку — человек решит, что оно сломано.
        if (deeplink && !/\{\{?URL\}?\}|\{url\}/i.test(deeplink)) {
          toast('В схеме нет метки {{URL}} — приложению не передастся ссылка подписки', 'err');
          return;
        }
        const body = {
          name, deeplink: deeplink || null,
          store_url: layer.querySelector('#apStore').value.trim() || null,
          icon_url: layer.querySelector('#apIcon').value.trim() || null,
          guide: layer.querySelector('#apGuide').value.trim() || null,
        };
        if (isNew) body.platform = layer.querySelector('#apPlat').value;
        try {
          if (isNew) await API.call('/api/sub/page-apps', { method:'POST', body });
          else await API.call('/api/sub/page-apps/'+a.id, { method:'PATCH', body });
          close();
          toast(isNew ? 'Приложение добавлено' : 'Сохранено');
          await onSaved();
        } catch(e){ toast('Не сохранилось: '+e.message, 'err'); }
      });
    }
  });
}
