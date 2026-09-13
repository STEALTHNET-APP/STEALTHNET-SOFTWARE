/* ═══════════ PAGES: Хосты, Профили конфигураций, Сквады ═══════════ */
'use strict';

/* ── ХОСТЫ ── */
let selectedHosts = new Set();
registerPage({
  id:'hosts', title:'Хосты', group:'Инфраструктура', icon:'host',
  render(){
    selectedHosts = new Set();
    return `
    <div class="page-head">
      <div><h1>Хосты</h1><div class="desc">Строки в подписке клиента. Порядок здесь = порядок локаций в приложении. Хост ссылается на инбаунд профиля конфигурации.</div></div>
      <div class="actions"><button class="btn primary" id="newHost">${I('plus',14)} Добавить хост</button></div>
    </div>
    <div id="hostBulk"></div>
    <div class="tbl-wrap"><table class="tbl">
      <thead><tr><th style="width:30px"></th><th style="width:34px"><label class="check"><input type="checkbox" id="hSelAll"></label></th>
        <th>Ремарка</th><th>Адрес</th><th>Профиль → инбаунд</th><th>Безопасность</th><th>Включён</th><th style="width:36px"></th></tr></thead>
      <tbody id="hostRows"></tbody>
    </table></div>
    <div class="sub-note" style="margin-top:10px">${I('info',12)} Перетащите за ${I('grip',12)} чтобы изменить порядок в подписке. Выключенный хост скрыт из подписок, но не удалён.</div>`;
  },
  bind(root){
    const drawBulk = ()=>{
      const bar = root.querySelector('#hostBulk');
      if(!selectedHosts.size){ bar.innerHTML=''; return; }
      bar.innerHTML = `<div class="bulk-bar"><b>Выбрано: ${selectedHosts.size}</b>
        <button class="btn sm" id="hBulkEdit">${I('edit',12)} Изменить у всех</button>
        <button class="btn sm" id="hBulkOn">${I('power',12)} Включить</button>
        <button class="btn sm" id="hBulkOff">${I('ban',12)} Выключить</button>
        <div style="flex:1"></div><button class="btn ghost sm" id="hBulkClear">${I('x',12)} Сброс</button></div>`;
      const bulkEnable = async (on) => {
        const ids = [...selectedHosts];
        // По одному запросу на хост: массового эндпоинта нет, а молча
        // применить половину хуже, чем показать, сколько прошло.
        let ok = 0;
        for (const id of ids) {
          try { await API.call('/api/hosts/'+id, { method:'PATCH', body:{ is_enabled: on } }); ok++; }
          catch(_) {}
        }
        toast(ok === ids.length
          ? `Хостов ${on?'включено':'выключено'}: ${ok}`
          : `Применено к ${ok} из ${ids.length} — остальные не сохранились`, ok===ids.length?'ok':'err');
        selectedHosts.clear();
        await refreshDB();
      };
      bar.querySelector('#hBulkOn').addEventListener('click', ()=>bulkEnable(true));
      bar.querySelector('#hBulkOff').addEventListener('click', ()=>bulkEnable(false));
      bar.querySelector('#hBulkClear').addEventListener('click', ()=>{ selectedHosts.clear(); draw(); });
      bar.querySelector('#hBulkEdit').addEventListener('click', ()=>editManyHostsDrawer([...selectedHosts]));
    };
    const draw = ()=>{
      root.querySelector('#hostRows').innerHTML = DB.hosts.map(h=>`
        <tr data-hid="${h.id}" class="${selectedHosts.has(h.id)?'selected':''}">
          <td><span class="grip">${I('grip',14)}</span></td>
          <td><label class="check"><input type="checkbox" data-hsel ${selectedHosts.has(h.id)?'checked':''}></label></td>
          <td><b style="font-size:13px">${h.remark}</b></td>
          <td class="mono">${h.addr}:${h.port}</td>
          <td>${h.inbound
            ? `<span class="chip click" data-prof>${h.profile} → ${h.inbound}</span>`
            /* Привязка слетела при замене конфига профиля. Показываем
               прямо в строке: иначе хост тихо не попадает в подписки,
               и понять почему — только по отсутствию локации у клиентов. */
            : `<span class="chip click" data-prof style="color:var(--warn-ink);background:color-mix(in srgb, var(--warn) 12%, transparent)">${I('alert',10)} привязать инбаунд</span>`}</td>
          <td><span class="bdg ${h.security==='REALITY'?'accent':'info'}">${h.security}</span>${h.sni?`<span class="sub mono">sni: ${h.sni}</span>`:''}</td>
          <td><label class="switch"><input type="checkbox" ${h.enabled?'checked':''} data-htoggle><span class="tr"></span></label></td>
          <td><button class="btn ghost icon-only" data-hmenu>${I('more',14)}</button></td>
        </tr>`).join('');
      drawBulk();
      root.querySelectorAll('[data-hid]').forEach(tr=>{
        const h = DB.hosts.find(x=>x.id===tr.dataset.hid);
        tr.querySelector('[data-hsel]').addEventListener('change', e=>{
          e.target.checked ? selectedHosts.add(h.id) : selectedHosts.delete(h.id);
          tr.classList.toggle('selected', e.target.checked); drawBulk();
        });
        tr.querySelector('[data-htoggle]').addEventListener('change', async e=>{
          const on = e.target.checked;
          await saveHost(h.id, { is_enabled: on },
            `Хост «${h.remark}» ${on?'включён — появится':'выключен — исчезнет'} в подписках`);
        });
        tr.querySelector('[data-prof]').addEventListener('click', e=>{
          e.stopPropagation();
          hostProfileDrawer(h);
        });

        // Щелчок по строке открывает хост на правку — как в клиентах и
        // нодах. Раньше строка выглядела нажимаемой (курсор-палец), но
        // не отзывалась: добраться до настроек можно было только через
        // меню из трёх точек, и это приходилось угадывать.
        tr.addEventListener('click', e=>{
          // Служебные элементы внутри строки обрабатывают себя сами:
          // галочка выделения, переключатель и меню.
          if (e.target.closest('input, button, label, .chip, .grip')) return;
          hostDrawer(h);
        });
        tr.querySelector('[data-hmenu]').addEventListener('click', e=>menu(e.currentTarget, [
          {label:'Редактировать', icon:'edit', onClick:()=>hostDrawer(h)},
          {label:'Сменить профиль/инбаунд', icon:'json', onClick:()=>hostProfileDrawer(h)},
          {label:'Клонировать', icon:'copy', onClick:async ()=>{
            // Копия выключена: у неё тот же адрес, и включённой она бы сразу
            // задвоила локацию в подписках клиентов.
            try {
              await API.call('/api/hosts', { method:'POST', body:{
                remark: h.remark+' (копия)', address: h.addr, port: h.port,
                inbound_tag: h.inbound, profile_id: Number(DB.profiles.find(p=>p.name===h.profile)?.id), security: h.security.toLowerCase(),
                sni: h.sni || null, fingerprint: h.fp || null, alpn: h.alpn || null,
                path: h.path || null, public_key: h.pbk || null, short_id: h.sid || null,
                is_enabled: false,
              }});
              toast('Хост клонирован (выключен по умолчанию)');
              await refreshDB();
            } catch(e){ toast('Не клонировался: '+e.message, 'err'); }
          }},
          '-',
          {label:'Удалить', icon:'trash', danger:true, onClick:()=>confirmModal({
            title:'Удалить хост?',
            text:`«${h.remark}» пропадёт из всех подписок при следующем обновлении.`,
            okText:'Удалить',
            onOk:async ()=>{
              try {
                await API.call('/api/hosts/'+h.id, { method:'DELETE' });
                toast('Хост удалён');
                await refreshDB();
              } catch(e){ toast('Не удалился: '+e.message, 'err'); }
            }})},
        ]));
      });
    };
    root.querySelector('#hSelAll').addEventListener('change', e=>{ selectedHosts = e.target.checked ? new Set(DB.hosts.map(h=>h.id)) : new Set(); draw(); });
    root.querySelector('#newHost').addEventListener('click', ()=>hostDrawer());
    draw();
  }
});

/* Одна точка сохранения хоста: и переключатель в таблице, и массовые
   операции, и редактор ходят сюда. Разъехавшиеся вызовы — верный способ
   получить «в одном месте сохраняется, в другом нет». */
async function saveHost(id, body, okText){
  try {
    await API.call('/api/hosts/'+id, { method:'PATCH', body });
    if (okText) toast(okText);
    await refreshDB();
    return true;
  } catch(e) {
    toast('Не сохранилось: '+e.message, 'err');
    await refreshDB();   // вернуть интерфейс к тому, что реально в базе
    return false;
  }
}

function hostDrawer(h){
  const isNew = !h;
  const src = h || {};
  // Профиль по умолчанию — первый существующий, а не выдуманное имя:
  // иначе форма нового хоста открывалась с несуществующим инбаундом.
  const firstProfile = DB.profiles[0];
  h = {
    id: src.id,
    remark: src.remark || '', addr: src.addr || '', port: src.port || 443,
    profile: src.profile || (firstProfile && firstProfile.name) || '',
    inbound: src.inbound || (firstProfile && firstProfile.inbounds[0]) || '',
    sni: src.sni || '',
    // Подставлять «chrome» существующему хосту нельзя: очищенное поле
    // открывалось со значением, будто очистка не сработала. Значение по
    // умолчанию — только у нового хоста, там оно и правда полезно.
    fp: src.fp || (src.id ? '' : 'chrome'),
    alpn: src.alpn || '', path: src.path || '',
    pbk: src.pbk || '', sid: src.sid || '',
    security: src.security || 'REALITY', enabled: src.enabled !== false,
  };

  const opt = (v, cur) => `<option ${v===cur?'selected':''}>${esc(v)}</option>`;
  /* Пустой пункт с подписью. Раньше он был безымянным: в списке
     оставалась пустая строка, которую не видно и трудно выбрать. */
  const optПусто = (cur) =>
    `<option value="" ${cur ? '' : 'selected'}>— не задан —</option>`;

  openDrawer({
    title:isNew?'Новый хост':'Хост · '+h.remark, sub:'Строка в подписке клиента', icon:'host', size:'lg', className:'sectioned-dialog '+'host-dialog',
    body:`
      <div class="field"><label>Ремарка <span class="req">*</span></label>
        <input class="inp" id="hRemark" value="${esc(h.remark)}" placeholder="🇳🇱 Амстердам · Reality">
        <div class="hint">Название локации, как его увидит клиент в приложении. Эмодзи-флаги приветствуются.</div></div>
      <div class="two-col">
        <div class="field"><label>Адрес <span class="req">*</span></label>
          <input class="inp mono" id="hAddr" value="${esc(h.addr)}" placeholder="ams.stealthnet.app"></div>
        <div class="field"><label>Порт</label>
          <input class="inp num" id="hPort" type="number" min="1" max="65535" value="${h.port}"></div>
      </div>
      <div class="form-section"><h4>${I('json',13)} Привязка к конфигурации</h4>
        <div class="two-col">
          <div class="field"><label>Профиль</label><select class="inp" id="hProf">
            ${DB.profiles.map(p=>opt(p.name, h.profile)).join('')}</select></div>
          <div class="field"><label>Инбаунд</label><select class="inp" id="hInb">
            ${((DB.profiles.find(p=>p.name===h.profile)||firstProfile||{inbounds:[]}).inbounds).map(i=>opt(i, h.inbound)).join('')}</select></div>
        </div>
        <div class="hint">Хост наследует протокол и транспорт из инбаунда. UUID клиента подставляется автоматически.</div>
      </div>
      <div class="form-section"><h4>${I('lock',13)} TLS / Транспорт</h4>
        <div class="two-col">
          <div class="field"><label>Security</label><select class="inp" id="hSec">
            ${['REALITY','TLS','NONE'].map(v=>opt(v, h.security)).join('')}</select></div>
          <div class="field"><label>SNI</label>
            <input class="inp mono" id="hSni" value="${esc(h.sni)}" placeholder="www.cloudflare.com"></div>
          <div class="field"><label>Fingerprint</label><select class="inp" id="hFp">
            ${optПусто(h.fp)}${['chrome','firefox','safari','random','ios','android','edge']
              .map(v=>opt(v, h.fp)).join('')}</select>
            <div class="hint">Не задан — клиент подставит свой. Отпечаток должен
              совпадать у всех клиентов локации.</div></div>
          <div class="field"><label>ALPN</label>
            <input class="inp mono" id="hAlpn" value="${esc(h.alpn)}" placeholder="h2,http/1.1"></div>
        </div>
        <div class="field"><label>Path (для WS / xHTTP)</label>
          <input class="inp mono" id="hPath" value="${esc(h.path)}" placeholder="/ws"></div>
      </div>
      <div class="form-section" id="hRealityBox"><h4>${I('key',13)} Ключи Reality</h4>
        <div class="hint">Берутся из профиля и здесь не задаются. Публичный ключ выводится
          из приватного, short id и имя сервера — первые из списков. Так они не могут
          разойтись: раньше ключи копировались в хост, и после перевыпуска в профиле
          локация переставала подключаться, ничего об этом не сообщая.</div>
      </div>
      <label class="check"><input type="checkbox" id="hEnabled" ${h.enabled?'checked':''}>
        <span>Включён — выключенный хост исчезает из подписок</span></label>`,
    footer:`<div class="spacer"></div><button class="btn" data-close>Отмена</button>
            <button class="btn primary" data-save>${isNew?'Создать хост':'Сохранить'}</button>`,
    onMount(layer, close){
      const $ = (id) => layer.querySelector('#'+id);

      $('hProf').addEventListener('change', e=>{
        const p = DB.profiles.find(x=>x.name===e.target.value);
        $('hInb').innerHTML = (p ? p.inbounds : []).map(i=>`<option>${esc(i)}</option>`).join('');
        inheritTransport();
      });

      // Блок ключей нужен только reality — для tls/none он вводит в заблуждение.
      const syncReality = () => {
        $('hRealityBox').style.display = $('hSec').value === 'REALITY' ? '' : 'none';
      };
      const inheritTransport = () => {
        const profile = DB.profiles.find(p=>p.name===$('hProf').value);
        const part = profile?.parts.find(p=>p.tag===$('hInb').value);
        if (part) { $('hSec').value=part.sec.toUpperCase(); $('hPort').value=part.port||443; }
        syncReality();
      };
      $('hInb').addEventListener('change', inheritTransport);
      if (isNew) inheritTransport();
      $('hSec').addEventListener('change', syncReality);
      syncReality();

      const btn = layer.querySelector('[data-save]');
      btn.addEventListener('click', async ()=>{
        const security = $('hSec').value.toLowerCase();
        const body = {
          remark: $('hRemark').value.trim(),
          address: $('hAddr').value.trim(),
          port: parseInt($('hPort').value, 10) || 443,
          inbound_tag: $('hInb').value,
          profile_id: Number(DB.profiles.find(p=>p.name===$('hProf').value)?.id),
          security,
          sni: $('hSni').value.trim() || null,
          fingerprint: $('hFp').value || null,
          alpn: $('hAlpn').value.trim() || null,
          path: $('hPath').value.trim() || null,
          is_enabled: $('hEnabled').checked,
        };

        if (!body.remark || !body.address) { toast('Нужны ремарка и адрес', 'err'); return; }
        if (!body.inbound_tag) { toast('Сначала создайте профиль с инбаундом', 'err'); return; }

        btn.disabled = true;
        try {
          if (isNew) await API.call('/api/hosts', { method:'POST', body });
          else await API.call('/api/hosts/'+h.id, { method:'PATCH', body });
          close();
          toast(isNew?'Хост создан':'Хост сохранён — подписки обновятся');
          await refreshDB();
        } catch(e) {
          toast('Не сохранилось: '+e.message, 'err');
          btn.disabled = false;
        }
      });
    }
  });
}

function hostProfileDrawer(h){
  openDrawer({
    title:'Профиль и инбаунд', sub:h.remark, icon:'json',
    body: DB.profiles.map(p=>`
      <div style="margin-bottom:14px">
        <div style="font-size:12px;font-weight:600;margin-bottom:7px;display:flex;align-items:center;gap:8px">${I('json',13)} ${p.name}
          <span class="sub-note">· на ${p.nodes.length} нодах</span></div>
        ${p.inbounds.map(i=>`
          <label class="check" style="padding:9px 12px;border:1px solid ${h.profile===p.name&&h.inbound===i?'color-mix(in srgb, var(--accent) 40%, transparent)':'var(--border)'};border-radius:9px;margin-bottom:6px;background:${h.profile===p.name&&h.inbound===i?'var(--accent-dim)':'transparent'}">
            <input type="radio" name="hpi" data-profile-id="${p.id}" ${h.profile===p.name&&h.inbound===i?'checked':''} style="appearance:auto;accent-color:var(--accent)">
            <span class="mono" style="font-size:12px">${i}</span>
          </label>`).join('')}
      </div>`).join(''),
    footer:`<div class="spacer"></div><button class="btn" data-close>Отмена</button><button class="btn primary" data-save>Применить</button>`,
    onMount(layer, close){
      layer.querySelector('[data-save]').addEventListener('click', async ()=>{
        const picked = layer.querySelector('input[name="hpi"]:checked');
        if (!picked) { toast('Выберите инбаунд', 'err'); return; }
        const tag = picked.closest('label').querySelector('.mono').textContent.trim();
        if (await saveHost(h.id, { inbound_tag: tag, profile_id: Number(picked.dataset.profileId) }, 'Привязка хоста обновлена')) close();
      });
    }
  });
}

function editManyHostsDrawer(ids){
  // Поле формы ↔ поле API. Общий список для отрисовки и для отправки:
  // разъехавшись, они дают «галочку поставил, а не применилось».
  const FIELDS = [
    { key:'port',        label:'Порт',      num:true },
    { key:'sni',         label:'SNI' },
    { key:'fingerprint', label:'Fingerprint' },
    { key:'alpn',        label:'ALPN' },
    { key:'inbound_tag', label:'Инбаунд' },
    { key:'security',    label:'Security',  hint:'reality / tls / none' },
  ];

  openDrawer({
    title:'Изменить несколько хостов', sub:`Выбрано: ${ids.length} · отметьте поля`, icon:'edit',
    body: FIELDS.map(f=>`
      <div class="bulk-host-field">
        <label class="check"><input type="checkbox" data-f="${f.key}"></label>
        <label for="bulk-host-${f.key}">${f.label}</label>
        <input class="inp" id="bulk-host-${f.key}" data-v="${f.key}" placeholder="${f.hint||'новое значение'}" disabled>
      </div>`).join(''),
    footer:`<div class="spacer"></div><button class="btn" data-close>Отмена</button>
            <button class="btn primary" data-save>Применить к ${ids.length}</button>`,
    onMount(layer, close){
      layer.querySelectorAll('[data-f]').forEach(cb=>cb.addEventListener('change', ()=>{
        layer.querySelector(`[data-v="${cb.dataset.f}"]`).disabled = !cb.checked;
      }));

      const btn = layer.querySelector('[data-save]');
      btn.addEventListener('click', async ()=>{
        const body = {};
        for (const f of FIELDS) {
          if (!layer.querySelector(`[data-f="${f.key}"]`).checked) continue;
          const raw = layer.querySelector(`[data-v="${f.key}"]`).value.trim();
          if (f.num) {
            const n = parseInt(raw, 10);
            if (!(n > 0 && n < 65536)) { toast(f.label+': нужен порт от 1 до 65535', 'err'); return; }
            body[f.key] = n;
          } else if (f.key === 'security') {
            const v = raw.toLowerCase();
            if (!['reality','tls','none'].includes(v)) { toast('Security: reality, tls или none', 'err'); return; }
            body[f.key] = v;
          } else body[f.key] = raw || null;
        }
        if (!Object.keys(body).length) { toast('Не отмечено ни одного поля', 'err'); return; }

        btn.disabled = true;
        let ok = 0;
        for (const id of ids) {
          try { await API.call('/api/hosts/'+id, { method:'PATCH', body }); ok++; }
          catch(_) {}
        }
        close();
        toast(ok === ids.length
          ? `Изменено хостов: ${ok}`
          : `Применено к ${ok} из ${ids.length} — остальные не сохранились`, ok===ids.length?'ok':'err');
        selectedHosts.clear();
        await refreshDB();
      });
    }
  });
}

/* ── ПРОФИЛИ КОНФИГУРАЦИЙ ── */
registerPage({
  id:'profiles', title:'Профили конфигураций', group:'Инфраструктура', icon:'json',
  render(){
    return `
    <div class="page-head">
      <div><h1>Профили конфигураций</h1><div class="desc">Xray-конфиги, которые раздаются нодам. Один профиль — много нод. Инбаунды из профиля привязываются к хостам и сквадам.</div></div>
      <div class="actions"><button class="btn" id="profileLibrary">${I('layers',14)} ${LANG==='en'?'Template library':'Библиотека шаблонов'}</button><button class="btn primary" id="newProf">${I('plus',14)} Новый профиль</button></div>
    </div>
    <div class="prof-list">
      ${DB.profiles.map(p=>профильСтрокой(p)).join('') || `
        <div class="card empty" style="padding:40px">${I('json',32)}<b>Профилей пока нет</b>
          <span>Создайте первый — панель предложит готовые заготовки</span></div>`}
    </div>`;
  },
  bind(root){
    root.querySelector('#profileLibrary').onclick=()=>ProfileWorkshop.library();
    root.querySelector('#newProf').addEventListener('click', ()=>newProfileModal());
    root.querySelectorAll('[data-edit]').forEach(b=>b.addEventListener('click', ()=>navigate('config?id='+b.dataset.edit)));
    root.querySelectorAll('[data-pnodes]').forEach(b=>b.addEventListener('click', e=>{
      e.stopPropagation();
      profileNodesModal(DB.profiles.find(p=>p.id===b.dataset.pnodes));
    }));
    // Клик по строке — тот же редактор: строка выглядит нажимаемой,
    // и в нодах она себя так и ведёт.
    root.querySelectorAll('[data-prow]').forEach(el=>el.addEventListener('click', e=>{
      if (e.target.closest('button, [data-pnodes], .pp')) return;
      navigate('config?id=' + el.dataset.prow);
    }));
    root.querySelectorAll('.pr-parts .pp').forEach(c=>c.addEventListener('click', e=>{
      e.stopPropagation();
      profileInboundsDrawer(DB.profiles.find(p=>p.id===c.closest('[data-prow]').dataset.prow));
    }));
    root.querySelectorAll('[data-cfmenu]').forEach(b=>b.addEventListener('click', e=>{
      const p = DB.profiles.find(x=>x.id===e.currentTarget.dataset.cfmenu);
      menu(e.currentTarget, [
        {label:'Открыть редактор', icon:'terminal', onClick:()=>navigate('config?id='+p.id)},
        {label:'Инбаунды', icon:'layers', onClick:()=>profileInboundsDrawer(p)},
        {label:'Активные ноды', icon:'server', onClick:()=>ProfileWorkshop.status(p)},
        {label:'Переименовать', icon:'edit', onClick:()=>renameDrawer(p.name, async (n)=>{
          // Раньше здесь был только всплывающий текст: имя откатывалось
          // при первом же обновлении данных.
          try {
            await API.call('/api/profiles/'+p.id, { method:'PATCH', body:{ name:n } });
            toast('Профиль переименован в «'+n+'»');
            await refreshDB();
          } catch(e){ toast('Не переименовался: '+e.message, 'err'); }
        })},
        {label:'Дублировать', icon:'copy', onClick:async ()=>{
          try {
            await API.call('/api/profiles', { method:'POST',
              body:{ name: p.name+' (копия)', config: JSON.parse(p.json) } });
            toast('Копия профиля создана');
            await refreshDB();
          } catch(e){ toast('Не скопировался: '+e.message, 'err'); }
        }},
        '-',
        {label:'Удалить', icon:'trash', danger:true, onClick:()=>confirmModal({title:'Удалить профиль?', text: p.nodes.length?`Профиль используется на <b>${p.nodes.length} нодах</b> — сначала переключите их на другой профиль.`:'Профиль не привязан к нодам, удаление безопасно.', okText:'Удалить',
          onOk:async ()=>{
            try {
              await API.call('/api/profiles/'+p.id, { method:'DELETE' });
              toast('Профиль удалён');
              await refreshDB();
            } catch(e){ toast(e.message, 'err'); }
          }})},
      ]);
    }));
  }
});
/* ── РЕДАКТОР КОНФИГА (отдельная страница) ──
 *
 * Раньше был слоем поверх списка. Конфиг правят подолгу — сверяются с
 * нодами, ходят за ключами, возвращаются, — и всплывающее окно для
 * такой работы тесно: оно закрывает всё позади и само ограничено в
 * высоте. Отдельная страница со своим адресом ещё и открывается в
 * новой вкладке и переживает перезагрузку.
 *
 * Адрес: #/config?id=<профиль>.
 */
registerPage({
  id:'config', title:'Редактор конфига', group:'Инфраструктура', icon:'terminal', nav:false,
  render(){
    const p = текущийПрофиль();
    if(!p) return `<div class="empty">${I('json',32)}<b>Профиль не найден</b>
      <span>Возможно, его удалили. <a href="#/profiles">Вернуться к списку</a></span></div>`;

    return `
    <div class="page-head" style="margin-bottom:14px">
      <div style="display:flex;align-items:center;gap:12px;min-width:0">
        <button class="btn ghost icon-only" id="cfgBack" title="К списку профилей">${I('chevL',15)}</button>
        <div style="min-width:0">
          <h1 class="mono" style="overflow:hidden;text-overflow:ellipsis;white-space:nowrap">${esc(p.name)}</h1>
          <div class="desc">Xray-конфигурация · JSON · ${p.nodes.length
            ? 'раскатывается на ' + p.nodes.length + ' ' + скл2(p.nodes.length,'ноду','ноды','нод')
            : 'ни одной ноды не привязано'}</div>
        </div>
      </div>
      <div class="actions">
        <button class="btn" id="cfgCancel">Отменить правки</button>
        <button class="btn primary" id="cfgSave">${I('check',14)} Сохранить и раскатать</button>
      </div>
    </div>

    <div class="cfg-page">
      <div class="cfg-bar">
        <button class="btn sm" id="cfgParameters">${LANG==='en'?'Parameters / import':'Параметры / импорт'}</button>
        <button class="btn sm" id="cfgLibrary">${LANG==='en'?'Save template':'Сохранить шаблон'}</button>
        <button class="btn sm" id="cfgHistory">${LANG==='en'?'History':'История'}</button>
        <button class="btn sm" id="cfgTrial">${LANG==='en'?'Test node':'Тестовая нода'}</button>
        <button class="btn sm" id="cfgExport">${LANG==='en'?'Export full config':'Экспорт полного конфига'}</button>
        <button class="btn sm" id="cfgKeys">${I('key',12)} Ключи</button>
        <button class="btn sm" id="cfgSnip">${I('zap',12)} Сниппеты</button>
        <button class="btn sm" id="cfgFmt">${I('sliders',12)} Форматировать</button>
        <button class="btn sm" id="cfgCheck">${I('shieldCheck',12)} Проверить</button>
        <button class="btn sm" id="cfgHelp" title="Что нужно, чтобы заработало">?</button>
        <div style="flex:1"></div>
        <span class="sub-note" id="cfgStat"></span>
      </div>

      <div class="cfg-main">
        <div class="cfg-edit">
          <div class="cfg-gutter" id="cfgNums"></div>
          <div class="cfg-wrap">
            <pre class="cfg-hl" id="cfgHl" aria-hidden="true"></pre>
            <textarea class="cfg-area" id="cfgArea" spellcheck="false"
                      autocapitalize="off" autocomplete="off"></textarea>
          </div>
        </div>

        <aside class="cfg-side">
          <div id="cfgState"></div>
          <div class="cfg-box">
            <h4>${I('layers',11)} Инбаунды</h4>
            <div id="cfgInb"></div>
          </div>
          <div class="cfg-box">
            <h4>${I('info',11)} Как это работает</h4>
            <div class="cfg-note">Инбаунды пересобираются из конфига при сохранении —
              отдельно заводить их не нужно. Версия конфига поднимется, и агенты
              заберут его в течение 15 секунд.
              <div style="margin-top:8px">Сохранить — <span class="kbd">⌘S</span>,
                форматировать — <span class="kbd">⌥⇧F</span>.</div></div>
          </div>
        </aside>
      </div>
    </div>`;
  },
  bind(root){ const p = текущийПрофиль(); if(p) настроитьРедактор(root, p); }
});

/* Профиль из адреса. Читаем при каждой отрисовке: страницу открывают и
   ссылкой из другого места, и после перезагрузки. */
function текущийПрофиль(){
  const q = (location.hash.split('?')[1] || '');
  const id = new URLSearchParams(q).get('id');
  return DB.profiles.find(x => String(x.id) === String(id)) || null;
}

function скл2(n, одна, две, много){
  const a = Math.abs(n) % 100, b = a % 10;
  if(a > 10 && a < 20) return много;
  if(b > 1 && b < 5) return две;
  if(b === 1) return одна;
  return много;
}

/* Подсветка JSON.
 *
 * Разбираем построчно и только то, что различимо без полноценного
 * парсера: ключи, строки, числа, литералы, скобки. Пытаться подсветить
 * сломанный JSON деревом разбора нельзя — он на то и сломан, а
 * подсветка нужна именно в этот момент. */
function подсветить(текст, плохаяСтрока){
  return текст.split('\n').map((строка, i) => {
    const s = esc(строка)
      .replace(/&quot;([^&]*?)&quot;(\s*:)/g, '<span class="k">"$1"</span>$2')
      .replace(/:(\s*)&quot;([^&]*?)&quot;/g, ':$1<span class="s">"$2"</span>')
      .replace(/\b(-?\d+\.?\d*)\b/g, '<span class="n">$1</span>')
      .replace(/\b(true|false|null)\b/g, '<span class="b">$1</span>')
      .replace(/([{}\[\],])/g, '<span class="p">$1</span>');
    // Пустая строка схлопнулась бы по высоте и сбила бы совпадение с
    // полем ввода — держим её пробелом.
    const тело = s || ' ';
    return (i + 1) === плохаяСтрока ? `<i class="bad">${тело}</i>` : тело;
  }).join('\n');
}

/* Строка, на которой JSON.parse споткнулся. Разные движки сообщают
   по-разному: где-то «at position N», где-то сразу «line N». */
function строкаОшибки(текст, сообщение){
  let m = /line (\d+)/i.exec(сообщение);
  if(m) return +m[1];
  m = /position (\d+)/i.exec(сообщение);
  if(m) return текст.slice(0, +m[1]).split('\n').length;
  return 0;
}

/* Обвязка редактора: подсветка, номера строк, разбор, сохранение. */
function настроитьРедактор(root, p){
  const area  = root.querySelector('#cfgArea');
  const hl    = root.querySelector('#cfgHl');
  const nums  = root.querySelector('#cfgNums');
  const state = root.querySelector('#cfgState');
  const stat  = root.querySelector('#cfgStat');
  const inbBox= root.querySelector('#cfgInb');

  area.value = p.json;
  let исходный = p.json;
  let плохая = 0;

  /* Перерисовка подсветки и номеров. Номера строит тот же цикл, что и
     подсветку, — иначе они разъезжаются при любой правке. */
  function перерисовать(){
    const текст = area.value;
    hl.innerHTML = подсветить(текст, плохая);
    const строк = текст.split('\n').length;
    nums.innerHTML = Array.from({length: строк}, (_, i) =>
      `<b class="${i + 1 === плохая ? 'bad' : ''}" data-ln="${i + 1}">${i + 1}</b>`).join('');
    stat.textContent = `строк: ${строк} · символов: ${текст.length}`
      + (текст === исходный ? '' : ' · есть несохранённые правки');
    синхронизировать();
  }

  /* Три слоя прокручиваются вместе: поле ввода ведущее. */
  function синхронизировать(){
    hl.scrollTop = area.scrollTop;
    hl.scrollLeft = area.scrollLeft;
    nums.scrollTop = area.scrollTop;
  }

  /* Разбор — на каждой правке, но не чаще раза в четверть секунды:
     подсказка нужна сразу, а гонять её на каждое нажатие незачем. */
  let таймер = 0;
  function разобрать(){
    clearTimeout(таймер);
    таймер = setTimeout(() => {
      try {
        const cfg = JSON.parse(area.value);
        плохая = 0;
        показатьСостояние(null);
        показатьИнбаунды(cfg);
      } catch(e){
        плохая = строкаОшибки(area.value, e.message);
        показатьСостояние(e.message);
      }
      перерисовать();
    }, 250);
  }

  function показатьСостояние(ошибка){
    state.innerHTML = ошибка
      ? `<div class="cfg-state err">${I('x',12)} JSON не разбирается: ${esc(ошибка)}
           ${плохая ? `<div style="margin-top:6px">Строка
             <span class="line" data-goto="${плохая}">${плохая}</span></div>` : ''}</div>`
      : `<div class="cfg-state ok">${I('check',12)} JSON разбирается.
           Нажмите «Проверить», чтобы движок посмотрел конфиг целиком.</div>`;
    const g = state.querySelector('[data-goto]');
    if(g) g.addEventListener('click', () => кСтроке(+g.dataset.goto));
  }

  /* Состав конфига: по нему ориентируются в длинном файле. */
  function показатьИнбаунды(cfg){
    const список = Array.isArray(cfg && cfg.inbounds) ? cfg.inbounds : [];
    inbBox.innerHTML = список.length ? список.map(i => {
      const тег = i.tag || '(без тега)';
      const сеть = (i.streamSettings && i.streamSettings.network) || '';
      const безоп = (i.streamSettings && i.streamSettings.security) || '';
      return `<div class="cfg-inb" data-find="${esc(тег)}">
        ${I('chevR',11)}<span class="t">${esc(тег)}</span>
        <span class="pt">${esc([i.protocol, сеть, безоп].filter(Boolean).join(' · '))}</span>
        <span class="pt">:${i.port ?? '—'}</span></div>`;
    }).join('') : '<div class="cfg-note">В конфиге нет ни одного инбаунда.</div>';

    inbBox.querySelectorAll('[data-find]').forEach(el => el.addEventListener('click', () => {
      const где = area.value.indexOf('"' + el.dataset.find + '"');
      if(где < 0) return;
      кСтроке(area.value.slice(0, где).split('\n').length);
    }));
  }

  /* Перевести курсор и прокрутку на строку. */
  function кСтроке(n){
    const строки = area.value.split('\n');
    const позиция = строки.slice(0, n - 1).join('\n').length + (n > 1 ? 1 : 0);
    area.focus();
    area.setSelectionRange(позиция, позиция + (строки[n - 1] || '').length);
    // Прокручиваем так, чтобы строка оказалась примерно посередине.
    const высотаСтроки = area.scrollHeight / Math.max(строки.length, 1);
    area.scrollTop = Math.max(0, (n - 1) * высотаСтроки - area.clientHeight / 2);
    синхронизировать();
  }

  /* Ввод. Tab в обычном textarea уводит фокус — в редакторе это
     означает, что отступ не поставить вовсе. */
  area.addEventListener('keydown', (e) => {
    const мета = e.metaKey || e.ctrlKey;

    if(мета && e.key.toLowerCase() === 's'){ e.preventDefault(); сохранить(); return; }
    if(мета && e.altKey && e.key.toLowerCase() === 'f'){ e.preventDefault(); форматировать(); return; }

    if(e.key === 'Tab'){
      e.preventDefault();
      const [a, b] = [area.selectionStart, area.selectionEnd];
      if(a === b && !e.shiftKey){
        вставить('  ');
      } else {
        // Выделение — сдвигаем блок строк целиком.
        const начало = area.value.lastIndexOf('\n', a - 1) + 1;
        const кусок = area.value.slice(начало, b);
        const новый = e.shiftKey
          ? кусок.replace(/^ {1,2}/gm, '')
          : кусок.replace(/^/gm, '  ');
        area.setRangeText(новый, начало, b, 'select');
        разобрать(); перерисовать();
      }
      return;
    }

    // Парные скобки и кавычки: закрывающую дописываем сами.
    const пары = { '{':'}', '[':']', '"':'"' };
    if(пары[e.key] && area.selectionStart === area.selectionEnd){
      e.preventDefault();
      const p0 = area.selectionStart;
      area.setRangeText(e.key + пары[e.key], p0, p0, 'end');
      area.setSelectionRange(p0 + 1, p0 + 1);
      разобрать(); перерисовать();
      return;
    }

    // Enter внутри скобок: открываем блок с отступом.
    if(e.key === 'Enter'){
      const p0 = area.selectionStart;
      const до = area.value[p0 - 1], после = area.value[p0];
      const строка = area.value.slice(area.value.lastIndexOf('\n', p0 - 1) + 1, p0);
      const отступ = (/^\s*/.exec(строка) || [''])[0];
      if((до === '{' && после === '}') || (до === '[' && после === ']')){
        e.preventDefault();
        area.setRangeText('\n' + отступ + '  \n' + отступ, p0, p0, 'end');
        area.setSelectionRange(p0 + отступ.length + 3, p0 + отступ.length + 3);
      } else if(отступ){
        e.preventDefault();
        area.setRangeText('\n' + отступ, p0, p0, 'end');
      }
      разобрать(); перерисовать();
      return;
    }
  });

  area.addEventListener('input', () => { разобрать(); перерисовать(); });
  area.addEventListener('scroll', синхронизировать);
  nums.addEventListener('click', (e) => {
    const b = e.target.closest('[data-ln]');
    if(b) кСтроке(+b.dataset.ln);
  });

  function вставить(текст){
    const p0 = area.selectionStart;
    area.setRangeText(текст, p0, area.selectionEnd, 'end');
    разобрать(); перерисовать();
  }

  function разобратьСразу(){
    try { return JSON.parse(area.value); }
    catch(e){
      плохая = строкаОшибки(area.value, e.message);
      показатьСостояние(e.message);
      перерисовать();
      кСтроке(плохая || 1);
      return null;
    }
  }

  function форматировать(){
    const cfg = разобратьСразу();
    if(!cfg) return;
    area.value = JSON.stringify(cfg, null, 2);
    разобрать(); перерисовать();
    toast('Отформатировано');
  }

  function показатьПроверку(r){
    const ошибки = (r.errors || []).map(e => `<div>${I('x',12)} ${esc(e)}</div>`).join('');
    const предупр = (r.warnings || []).map(w => `<div>${I('alert',12)} ${esc(w)}</div>`).join('');
    const кем = r.engine_checked ? 'проверено движком' : 'структурная проверка (движок недоступен)';
    // Дописанное показываем всегда: человек вставил один конфиг, а
    // сохранится другой — узнать об этом он должен здесь, а не открыв
    // профиль заново и не поняв, откуда взялись чужие блоки.
    const дописано = (r.added || []).length
      ? `<div class="cfg-state warn" style="margin-top:8px">${I('info',12)}
           Панель добавит от себя: ${(r.added || []).map(esc).join(', ')}.
           Без этого не считается трафик.</div>` : '';
    state.innerHTML = (r.valid
      ? `<div class="cfg-state ok">${I('check',13)} Конфиг валиден · ${кем}
           ${предупр ? `<div style="margin-top:7px">${предупр}</div>` : ''}</div>`
      : `<div class="cfg-state err">${ошибки}
           ${предупр ? `<div style="margin-top:7px">${предупр}</div>` : ''}</div>`) + дописано;
  }

  async function проверить(){
    const config = разобратьСразу();
    if(!config) return null;
    try { const r = await API.call('/api/profiles/validate', {method:'POST', body:{config}});
          показатьПроверку(r); return r; }
    catch(e){ toast('Проверка не удалась: ' + e.message, 'err'); return null; }
  }

  async function сохранить(){
    const config = разобратьСразу();
    if(!config) return;
    // Проверяем ДО подтверждения: незачем спрашивать «раскатать?»,
    // если конфиг всё равно не примут.
    const r = await проверить();
    if(!r) return;
    if(!r.valid) return toast('Конфиг не сохранён — сначала исправьте ошибки', 'err');

    await ProfileWorkshop.review(p, config, async res => {
      исходный = area.value;
      p.version=res.version;
      await refreshDB();
      toast(LANG==='en'?'Configuration saved, version '+res.version:'Конфигурация сохранена, версия '+res.version);
      перерисовать();
    });
  }

  const loadDraft=config=>{area.value=JSON.stringify(config,null,2);area.dispatchEvent(new Event('input',{bubbles:true}));};
  root.querySelector('#cfgParameters').onclick=()=>{const config=разобратьСразу();if(config)ProfileWorkshop.wizard({config,name:p.name,onApply:loadDraft});};
  root.querySelector('#cfgLibrary').onclick=()=>{const config=разобратьСразу();if(config)ProfileWorkshop.saveTemplate(config,{name:p.name});};
  root.querySelector('#cfgHistory').onclick=()=>ProfileWorkshop.history(p,loadDraft);
  root.querySelector('#cfgTrial').onclick=()=>{const config=разобратьСразу();if(config)ProfileWorkshop.trial(p,config,async res=>{p.version=res.version;await refreshDB();});};
  root.querySelector('#cfgExport').onclick=()=>{const config=разобратьСразу();if(config)confirmModal({title:LANG==='en'?'Export full working configuration?':'Экспортировать полный рабочий конфиг?',text:LANG==='en'?'This file contains private keys and other credentials. Use “Save template” to export reusable parameters.':'Этот файл содержит приватные ключи и другие секреты. Для переносимого шаблона используйте «Сохранить шаблон».',okText:LANG==='en'?'Download full configuration':'Скачать полный конфиг',onOk:()=>ProfileWorkshop.download(config,p.name)});};
  root.querySelector('#cfgSave').addEventListener('click', сохранить);
  root.querySelector('#cfgCheck').addEventListener('click', проверить);
  root.querySelector('#cfgFmt').addEventListener('click', форматировать);
  root.querySelector('#cfgSnip').addEventListener('click', () => snippetsDrawer(area));
  root.querySelector('#cfgKeys').addEventListener('click', () => keygenModal());
  root.querySelector('#cfgHelp').addEventListener('click', () => {
    let cfg = null;
    try { cfg = JSON.parse(area.value); } catch(_){}
    profileHelpModal(cfg);
  });
  root.querySelector('#cfgBack').addEventListener('click', () => уйти());
  root.querySelector('#cfgCancel').addEventListener('click', () => {
    if(area.value === исходный) return уйти();
    confirmModal({ title:'Отменить правки?', danger:true,
      text:'Изменения в конфиге пропадут — они нигде не сохранены.',
      okText:'Отменить правки',
      onOk:() => { area.value = исходный; разобрать(); перерисовать(); } });
  });

  /* Уход со страницы с несохранёнными правками — самый обидный способ
     потерять работу, поэтому спрашиваем. */
  function уйти(){
    if(area.value === исходный) return navigate('profiles');
    confirmModal({ title:'Уйти без сохранения?', danger:true,
      text:'В конфиге есть несохранённые правки.', okText:'Уйти',
      onOk:() => navigate('profiles') });
  }

  /* Высоту считаем от того, где страница на самом деле началась.
     Вычитать «примерно 132 пикселя» нельзя: шапка меняется от длины
     имени профиля и от ширины окна, и при промахе редактор вылезал за
     экран — вместе с кнопкой сохранения. */
  const страница = root.querySelector('.cfg-page');
  function подогнатьВысоту(){
    const сверху = страница.getBoundingClientRect().top + window.scrollY;
    страница.style.height = Math.max(360, innerHeight - сверху - 24) + 'px';
  }
  подогнатьВысоту();
  addEventListener('resize', подогнатьВысоту);

  разобрать();
  перерисовать();
  // preventScroll обязателен: фокус на длинном поле утаскивает страницу
  // к каретке, и шапка с кнопкой «Сохранить» оказывается за экраном.
  area.focus({ preventScroll: true });
  area.setSelectionRange(0, 0);
  area.scrollTop = 0;
  синхронизировать();
}

/// Генератор ключей Reality.
///
/// Приватный ключ идёт в конфиг профиля, публичный — в хост (клиент по нему
/// проверяет сервер). Если их перепутать, подключение молча не встанет,
/// поэтому подписываем каждое поле, куда именно его вставлять.
function keygenModal(){
  openModal({
    title:'Генератор ключей', sub:'X25519 для Reality', icon:'key',
    body:`
      <div class="seg" style="margin-bottom:14px">
        <button class="on">X25519 · Reality</button>
      </div>
      <div id="kgBody">
        <div class="empty">${I('key',32)}<b>Ключи ещё не сгенерированы</b>
          <span>Нажмите кнопку ниже — панель создаст новую пару</span></div>
      </div>
      <div class="hint" style="margin-top:12px;color:var(--warn-ink)">${I('alert',12)}
        Смена ключей у работающего профиля отключит всех клиентов, пока они не
        обновят подписку. Для нового профиля это безопасно.</div>`,
    footer:`<div class="spacer"></div><button class="btn" data-close>Закрыть</button>
            <button class="btn primary" data-gen>${I('key',13)} Сгенерировать пару</button>`,
    onMount(layer){
      layer.querySelector('[data-gen]').addEventListener('click', async ()=>{
        const btn = layer.querySelector('[data-gen]');
        btn.disabled = true; btn.textContent = 'Генерируем…';
        try{
          const k = await API.call('/api/keygen/reality', {method:'POST'});
          layer.querySelector('#kgBody').innerHTML = `
            <div class="hint" style="margin-bottom:14px">${I('info',12)}
              Всё нужное — в одном блоке ниже. Публичный ключ панель выведет из приватного
              сама, а в хост ключи не вписываются вовсе: раньше их разносили по двум местам,
              и половинки от разных нажатий этой кнопки давали локацию, которая молча не
              подключалась.</div>
            <div class="field">
              <label>Вставьте в профиль, в <code>streamSettings</code> инбаунда</label>
              <div class="code wrap"><pre>"realitySettings": {
  "dest": "www.cloudflare.com:443",
  "serverNames": ["www.cloudflare.com"],
  "privateKey": "${esc(k.private_key)}",
  "shortIds": ["${esc(k.short_id)}"]
}</pre></div>
              <button class="btn primary sm" style="margin-top:10px"
                      data-copy='"realitySettings": {
  "dest": "www.cloudflare.com:443",
  "serverNames": ["www.cloudflare.com"],
  "privateKey": "${esc(k.private_key)}",
  "shortIds": ["${esc(k.short_id)}"]
}' data-copy-msg="Блок скопирован">${I('copy',12)} Копировать блок</button>
            </div>
            <div class="field">
              <label>Публичный ключ <span class="hint" style="margin:0;color:var(--dim2)">только для сверки — копировать никуда не нужно</span></label>
              <div class="copy-block"><span class="txt">${esc(k.public_key)}</span></div>
            </div>`;
          toast('Пара ключей создана');
        }catch(e){ toast('Не удалось: '+e.message,'err'); }
        btn.disabled = false; btn.innerHTML = I('key',13)+' Сгенерировать ещё';
      });
    }
  });
}

/* Сниппеты — готовые блоки конфига. Вставляются в открытый редактор,
   а не «куда-то»: иначе кнопка сообщает об успехе, а конфиг не меняется. */

/* Создание профиля из заготовки.

   Чистый лист — плохая отправная точка: конфиг Xray легко написать так,
   что он загрузится и не заработает. Раньше в этом окне был выпадающий
   список «Начать с», который ни на что не влиял: как ни выбери,
   создавался один и тот же пустой каркас. */
/// Что нужно сделать, чтобы заготовка заработала.
///
/// Требования у протоколов разные и снаружи не видны: Reality нужны
/// ключи, Trojan и Hysteria2 — свой домен и сертификат, Shadowsocks —
/// серверный ключ. Без этого человек создаёт профиль, получает отказ
/// движка и остаётся с сообщением, из которого не следует, что делать.
function presetHelpModal(preset){
  openModal({
    title: preset.name, sub:'Что нужно, чтобы заработало', icon:'info', size:'md',
    body:`
      <p class="sub-note" style="margin-bottom:16px">${esc(preset.desc)}</p>
      <ol class="steps">
        ${(preset.setup || []).map(s => `
          <li>
            <div class="steps-t">${esc(s.t)}</div>
            <div class="sub-note">${s.d}</div>
          </li>`).join('')}
      </ol>
      <div class="hint">${I('info',12)}
        Поля, написанные заглавными буквами, — заглушки: конфиг с ними
        намеренно не пройдёт проверку движком.</div>`,
    footer:`<div class="spacer"></div><button class="btn primary" data-close>Понятно</button>`,
  });
}

/// Подсказка для открытого конфига.
///
/// Заготовку подбираем по тому, что в конфиге сейчас: протокол инбаунда
/// и вид шифрования однозначно указывают, о чём человеку читать. Если
/// совпало несколько — показываем все: в одном профиле бывает и Reality,
/// и запасной веб-сокет, и требования у них разные.
function profileHelpModal(cfg){
  const inbounds = (cfg && Array.isArray(cfg.inbounds) ? cfg.inbounds : [])
    .filter(i => i.tag !== 'api-in');

  const key = (i) => {
    const proto = i.protocol;
    const sec = i.streamSettings?.security || 'none';
    const net = i.streamSettings?.network || 'tcp';
    if (proto === 'hysteria') return 'hysteria2';
    if (proto === 'shadowsocks') return 'shadowsocks-2022';
    if (proto === 'trojan') return 'trojan-tcp';
    if (proto === 'vmess') return 'vmess-ws';
    if (sec === 'reality') return net === 'grpc' ? 'reality-grpc' : 'reality-tcp';
    if (net === 'xhttp' || net === 'splithttp') return 'vless-xhttp';
    if (net === 'httpupgrade') return 'vless-httpupgrade';
    if (net === 'kcp') return 'vless-mkcp';
    if (net === 'ws') return 'vless-ws-tls';
    return null;
  };

  const ids = [...new Set(inbounds.map(key).filter(Boolean))];
  const matched = ids.map(id => PROFILE_PRESETS.find(p => p.id === id)).filter(Boolean);

  if (matched.length === 1) return presetHelpModal(matched[0]);

  openModal({
    title:'Что нужно, чтобы заработало', icon:'info', size:'md',
    body: matched.length
      ? matched.map(p => `
          <div class="form-section" style="margin-top:0">
            <h4>${I('layers',13)} ${esc(p.name)}</h4>
            <ol class="steps">
              ${(p.setup || []).map(s => `
                <li><div class="steps-t">${esc(s.t)}</div>
                    <div class="sub-note">${s.d}</div></li>`).join('')}
            </ol>
          </div>`).join('')
      : `<div class="empty">${I('info',28)}<b>Не разобрал конфиг</b>
           <span>Подсказка подбирается по протоколам инбаундов. Проверьте, что JSON
           корректен и в нём есть раздел <code>inbounds</code>.</span></div>`,
    footer:`<div class="spacer"></div><button class="btn primary" data-close>Понятно</button>`,
  });
}

function newProfileModal(){ return ProfileWorkshop.wizard(); }

/* Строка профиля.

   Была карточка в сетке из трёх колонок: на широком экране каждая
   растягивалась на треть ширины ради четырёх строк содержимого, и
   страница выглядела пустой. Профилей на установку — единицы, а знать
   про каждый нужно одно и то же: что за инбаунды внутри, на каких
   нодах раскатан и когда правили. Это строка, а не карточка. */
function профильСтрокой(p){
  // Транспорт и защита в одну подпись: «vless · reality :443» читается
  // быстрее трёх отдельных значков.
  const часть = (c) => `
    <span class="pp" title="${esc(c.tag)}">
      <b class="mono">${esc(c.tag)}</b>
      <span>${esc([c.protocol, c.net !== 'tcp' ? c.net : '', c.sec !== 'none' ? c.sec : '']
        .filter(Boolean).join(' · '))}</span>
      <i>:${c.port ?? '—'}</i>
    </span>`;

  const части = (p.parts && p.parts.length) ? p.parts.map(часть).join('')
    : `<span class="pp empty-pp">${I('alert',10)} нет клиентских инбаундов</span>`;

  return `
  <div class="prof-row" data-prow="${p.id}">
    <div class="pr-main">
      <span class="pr-ic">${I('json',15)}</span>
      <div class="pr-name">
        <b class="mono">${esc(p.name)}</b>
        <span class="sub-note">версия ${p.version} · правлен ${fmtDT(p.updated)}</span>
      </div>
      <div class="pr-parts">${части}</div>
    </div>
    <div class="pr-side">
      <span class="pr-nodes ${p.nodes.length ? '' : 'none'}" data-pnodes="${p.id}"
            title="${p.nodes.length ? esc(p.nodes.join(', ')) : 'ни одной ноды'}">
        ${I('server',12)} ${p.nodes.length ? esc(p.nodes.join(', ')) : 'не раскатан'}</span>
      <button class="btn sm" data-edit="${p.id}">${I('terminal',12)} Редактор</button>
      <button class="btn ghost icon-only" data-cfmenu="${p.id}">${I('more',14)}</button>
    </div>
  </div>`;
}

/* Блоки маршрутизации лежат в presets.js — один список на панель. */
const SNIPPETS = ROUTE_SNIPPETS;

function snippetsDrawer(target){
  openDrawer({
    title:'Сниппеты', sub:'Готовые блоки для вставки в конфиг', icon:'zap',
    body: SNIPPETS.map(([t,d],i)=>`
      <div class="info-row" style="cursor:pointer" data-snippet="${i}">
        <span class="v" style="flex:1;flex-direction:column;align-items:flex-start">
          <b style="font-size:12.5px">${esc(t)}</b>
          <span style="font-size:11px;color:var(--text-3)">${esc(d)}</span></span>
        ${I(target?'plus':'copy',14)}
      </div>`).join('') +
      `<div class="hint" style="margin-top:12px">${I('info',12)} ${target
        ? 'Блок вставится в позицию курсора. После вставки нажмите «Проверить» — конфиг должен остаться валидным JSON.'
        : 'Редактор не открыт, поэтому блок копируется в буфер.'}</div>`,
    footer:`<div class="spacer"></div><button class="btn" data-close>Закрыть</button>`,
    onMount(layer, close){
      layer.querySelectorAll('[data-snippet]').forEach(el=>el.addEventListener('click', ()=>{
        const body = SNIPPETS[+el.dataset.snippet][2];
        if (!target) { copyText(body, 'Сниппет скопирован'); return; }
        // Вставляем в позицию курсора, а не в конец: блок почти всегда
        // нужен внутри массива, а не после закрывающей скобки.
        const pos = target.selectionStart ?? target.value.length;
        target.value = target.value.slice(0, pos) + body + target.value.slice(pos);
        target.focus();
        target.selectionStart = target.selectionEnd = pos + body.length;
        target.dispatchEvent(new Event('input', {bubbles:true}));
        close();
        toast('Сниппет вставлен — проверьте конфиг');
      }));
    }
  });
}
function profileInboundsDrawer(p){
  openDrawer({title:'Инбаунды · '+esc(p.name),sub:'Параметры из сохранённого профиля',icon:'layers',initialFocus:'title',
    body:(p.parts||[]).map(i=>`<section class="client-section" style="margin-bottom:12px"><h4>${esc(i.tag)}</h4><div class="info-row"><span class="v">${esc(i.protocol)} · ${esc(i.net)} · ${esc(i.sec)}</span><span class="num">Порт ${esc(i.port??'не задан')}</span></div><p class="hint">Связанные хосты: ${DB.hosts.filter(h=>h.profile===p.name&&h.inbound===i.tag).length}. Сквады: ${DB.squadsInt.filter(s=>(s.inboundRefs||[]).some(r=>String(r.profile_id)===String(p.id)&&r.tag===i.tag)).map(s=>esc(s.name)).join(', ')||'не назначены'}.</p></section>`).join('')||'<div class="empty"><b>Инбаундов нет</b><span>Добавьте их в профиль конфигурации.</span></div>',
    footer:'<div class="spacer"></div><button class="btn" data-close>Закрыть</button>'});
}
function profileNodesModal(p){
  openModal({
    title:'Активные ноды', sub:p.name, icon:'server',
    body: p.nodes.length ? `<div class="info-rows">${p.nodes.map(n=>{
      const node = DB.nodes.find(x=>x.name===n);
      return `<div class="info-row"><span class="v">${ccChip(node.cc)}</span>
        <span class="v mono" style="flex:1">${n}</span>
        <span class="status-dot ${node.status==='online'?'ok':'err'}"></span>
        <span style="font-size:11.5px;color:var(--text-2)">${node.status==='online'?'online':'offline'}</span></div>`;
    }).join('')}</div>` : `<div class="empty">${I('server',32)}<b>Нет привязанных нод</b><span>Назначьте профиль ноде в её настройках</span></div>`,
    footer:`<div class="spacer"></div><button class="btn" data-close>Закрыть</button>`,
  });
}
function renameDrawer(current, cb){
  openModal({
    title:'Переименовать', icon:'edit', size:'sm',
    body:`<div class="field"><label>Новое название</label><input class="inp mono" value="${esc(current)}" id="rnInp"></div>`,
    footer:`<div class="spacer"></div><button class="btn" data-close>Отмена</button><button class="btn primary" data-ok>Сохранить</button>`,
    onMount(l, close){ l.querySelector('[data-ok]').addEventListener('click', ()=>{ const v=l.querySelector('#rnInp').value; close(); cb(v); }); }
  });
}

/* ── ВНУТРЕННИЕ СКВАДЫ ── */
registerPage({
  id:'squads-int', title:'Внутренние сквады', group:'Инфраструктура', icon:'squads',
  render(){
    return `
    <div class="page-head">
      <div><h1>Внутренние сквады</h1><div class="desc">Группа инбаундов, назначаемая клиентам. Тариф → сквады → инбаунды → ноды: так клиент получает свой набор локаций.</div></div>
      <div class="actions"><button class="btn primary" id="newSquad">${I('plus',14)} Создать сквад</button></div>
    </div>
    ${/* Плитка вместо трёх широких колонок: на пустой установке один
         сквад растягивался на весь экран и выглядел недоделанным. */''}
    <div class="tile-grid">
      ${DB.squadsInt.map(s=>`
        <div class="card card-click sq-card" data-sqrow="${s.id}">
          <div class="sq-head">
            <div class="avatar-sm sq-ic">${I('squads',15)}</div>
            <div class="sq-name">
              <b>${esc(s.name)}</b>
              ${s.desc ? `<span class="sub">${esc(s.desc)}</span>` : ''}
            </div>
            <button class="btn ghost icon-only" data-sqmenu="${s.id}">${I('more',14)}</button>
          </div>
          ${/* Два числа вместо двух крупных блоков: цифры здесь
               второстепенны, главное — состав. */''}
          <div class="sq-meta">
            <span>${I('users2',12)} ${fmtN(s.members)}</span>
            <span>${I('layers',12)} ${s.inbounds.length}</span>
          </div>
          <div class="sq-inb">
            ${s.inbounds.length
              ? s.inbounds.slice(0,4).map(i=>`<span class="chip">${esc(i)}</span>`).join('') +
                (s.inbounds.length > 4 ? `<span class="chip">+${s.inbounds.length - 4}</span>` : '')
              : '<span class="sub-note">инбаунды не выбраны</span>'}
          </div>
        </div>`).join('') || `<div class="card empty">${I('squads',30)}<b>Сквадов нет</b>
            <span>Сквад собирает инбаунды в набор, который выдаётся клиентам по тарифу.</span></div>`}
    </div>`;
  },
  bind(root){
    root.querySelector('#newSquad').addEventListener('click', ()=>squadInbDrawer({name:'Новый сквад', inbounds:[], members:0}, true));
    // Щелчок по карточке открывает инбаунды сквада — то, ради чего в неё
    // и заходят. Раньше карточка не отзывалась вовсе, и добраться до
    // состава можно было только через меню из трёх точек.
    root.querySelectorAll('[data-sqrow]').forEach(c=>c.addEventListener('click', e=>{
      if (e.target.closest('button, input, label')) return;
      const s = DB.squadsInt.find(x=>x.id===c.dataset.sqrow);
      if (s) squadInbDrawer(s);
    }));
    root.querySelectorAll('[data-sqmenu]').forEach(b=>b.addEventListener('click', e=>{
      const s = DB.squadsInt.find(x=>x.id===e.currentTarget.dataset.sqmenu);
      menu(e.currentTarget, [
        {label:'Инбаунды', icon:'layers', onClick:()=>squadInbDrawer(s)},
        {label:'Доступные ноды', icon:'server', onClick:()=>squadNodesDrawer(s)},
        {label:'Потребление трафика', icon:'chart', onClick:()=>squadUsageDrawer(s)},
        {label:'Переименовать', icon:'edit', onClick:()=>renameDrawer(s.name, async (n)=>{
          // Раньше здесь показывалось «Сквад переименован», но запроса не
          // было: имя возвращалось прежним при первом же обновлении.
          try {
            await API.call('/api/squads/'+s.id, { method:'PATCH', body:{ name:n } });
            toast('Сквад переименован в «'+n+'»');
            await refreshDB();
          } catch(e){ toast('Не переименовался: '+e.message, 'err'); }
        })},
        '-',
        {label:'Добавить всех клиентов', icon:'users2', onClick:()=>confirmModal({
          title:'Добавить всех?', danger:false,
          // Число берём из базы, а не из воздуха: раньше здесь стояло
          // фиксированное «8 412», и кнопка ничего не делала.
          text:`Все ${fmtN(DB.usersTotal || DB.users.length)} клиентов будут добавлены в сквад <b>${esc(s.name)}</b>.`,
          okText:'Добавить',
          onOk:async ()=>{
            try {
              const r = await API.call('/api/squads/'+s.id+'/clients', { method:'POST', body:{ action:'add' } });
              toast('Добавлено клиентов: '+fmtN(r.affected||0));
              await refreshDB();
            } catch(e){ toast('Не добавились: '+e.message, 'err'); }
          }})},
        {label:'Убрать всех клиентов', icon:'ban', onClick:()=>confirmModal({
        title:'Убрать всех?', text:`У ${fmtN(s.members)} клиентов пропадут локации этого сквада.`, okText:'Убрать',
        onOk:async ()=>{
          try {
            await API.call('/api/squads/'+s.id+'/clients', { method:'POST', body:{ action:'remove' } });
            toast('Клиенты убраны из сквада');
            await refreshDB();
          } catch(e){ toast('Не получилось: '+e.message, 'err'); }
        }})},
        '-',
        {label:'Удалить сквад', icon:'trash', danger:true, onClick:()=>confirmModal({
          title:'Удалить сквад?',
          text:`В скваде <b>${fmtN(s.members)}</b> участников — они потеряют его локации.`,
          okText:'Удалить',
          onOk:async ()=>{
            try {
              await API.call('/api/squads/'+s.id, { method:'DELETE' });
              toast('Сквад удалён');
              await refreshDB();
            } catch(e){ toast('Не удалился: '+e.message, 'err'); }
          }})},
      ]);
    }));
  }
});
function squadInbDrawer(s, isNew){
  openDrawer({
    title:(isNew?'Создание сквада':'Инбаунды · '+s.name), sub:'Отметьте инбаунды, которые входят в сквад', icon:'layers',
    body:`
      ${isNew?`<div class="field"><label>Название <span class="req">*</span></label><input class="inp" id="sqName" value=""></div>`:''}
      ${DB.profiles.map(p=>`
        <div style="margin-bottom:12px">
          <div class="sub-note" style="margin-bottom:6px">${p.name}</div>
          ${p.inbounds.map(i=>`
            <label class="check" style="padding:9px 12px;border:1px solid var(--border);border-radius:9px;margin-bottom:6px">
              <input type="checkbox" data-inb="${esc(i)}" data-profile-id="${p.id}" ${(s.inboundRefs||[]).some(r=>String(r.profile_id)===String(p.id)&&r.tag===i)?'checked':''}>
              <span class="mono" style="font-size:12px;flex:1">${i}</span>
              <span class="sub-note">${DB.nodes.filter(n=>n.profile===p.name&&n.inbounds.includes(i)).length} нод</span>
            </label>`).join('')}
        </div>`).join('')}`,
    footer:`<div class="spacer"></div><button class="btn" data-close>Отмена</button><button class="btn primary" data-save>${isNew?'Создать':'Сохранить'}</button>`,
    onMount(l, close){
      const btn = l.querySelector('[data-save]');
      btn.addEventListener('click', async ()=>{
        const inbound_refs = [...l.querySelectorAll('[data-inb]')]
          .filter(cb=>cb.checked).map(cb=>({profile_id:Number(cb.dataset.profileId),tag:cb.dataset.inb}));
        const body = { inbound_refs };
        if (isNew) {
          body.name = (l.querySelector('#sqName').value || '').trim();
          if (!body.name) { toast('Нужно название сквада', 'err'); return; }
        }
        btn.disabled = true;
        try {
          if (isNew) await API.call('/api/squads', { method:'POST', body });
          else await API.call('/api/squads/'+s.id, { method:'PATCH', body });
          close();
          toast(isNew?'Сквад создан':'Инбаунды сквада обновлены — подписки перегенерируются');
          await refreshDB();
        } catch(e) {
          toast('Не сохранилось: '+e.message, 'err');
          btn.disabled = false;
        }
      });
    }
  });
}
function squadNodesDrawer(s){
  const nodes = DB.nodes.filter(n=>squadNodeTags(s,n).length);
  openDrawer({
    title:'Доступные ноды · '+esc(s.name), sub:`Через инбаунды: ${s.inbounds.map(esc).join(', ')}`, icon:'server',
    body:`<div class="info-rows">${nodes.map(n=>`
      <div class="info-row"><span class="v">${ccChip(n.cc)}</span>
        <span class="v mono" style="flex:1">${esc(n.name)}</span>
        <span class="v">${squadNodeTags(s,n).map(i=>`<span class="chip">${i}</span>`).join(' ')}</span>
        <span class="status-dot ${n.status==='online'?'ok':'err'}"></span></div>`).join('')||'<div class="empty"><b>Ноды не назначены</b><span>Назначьте нодам профиль и инбаунды этого сквада.</span></div>'}</div>`,
    footer:`<div class="spacer"></div><button class="btn" data-close>Закрыть</button>`,
  });
}
function squadUsageDrawer(s){
  openDrawer({
    title:'Потребление · '+esc(s.name), sub:'За 30 дней · весь трафик текущих участников, включая другие доступы', icon:'chart',
    body:`<div id="suBody" class="sub-note">Загружаем…</div>`,
    footer:`<div class="spacer"></div><button class="btn" data-close>Закрыть</button>`,
    async onMount(layer){
      let data;
      try { data = await API.call('/api/squads/'+s.id+'/traffic?days=30'); }
      catch(e){layer.querySelector('#suBody').innerHTML=`<p role="alert">Не удалось получить потребление: ${esc(e.message)}</p>`;return;}
      const byDay = {}, byNode = {};
      for(const row of data.items){byDay[row.day]=(byDay[row.day]||0)+row.bytes;byNode[row.name]=(byNode[row.name]||0)+row.bytes;}
      const pts = Object.entries(byDay).sort();
      const total = pts.reduce((a,[,v])=>a+v, 0);
      const max = Math.max(1, ...pts.map(([,v])=>v));
      const topNode = Object.entries(byNode).sort((a,b)=>b[1]-a[1])[0];

      layer.querySelector('#suBody').innerHTML = (pts.length ? `
        <div style="display:flex;align-items:flex-end;gap:3px;height:130px">
          ${pts.map(([day,v])=>`<div title="${day}: ${fmtBytes(v)}"
            style="flex:1;min-width:3px;height:${Math.max(2, v/max*100)}%;background:var(--accent);
                   opacity:.85;border-radius:2px 2px 0 0"></div>`).join('')}
        </div>` : '<div class="sub-note">За 30 дней трафика не было.</div>') + `
        <div class="info-rows" style="margin-top:14px">
          <div class="info-row"><span class="k">Участников</span><span class="v num">${fmtN(data.members)}</span></div>
          <div class="info-row"><span class="k">Всего за 30 дней</span><span class="v num">${fmtBytes(total)}</span></div>
          <div class="info-row"><span class="k">Среднее на клиента</span>
            <span class="v num">${fmtBytes(Math.round(total/Math.max(1,data.members)))}</span></div>
          <div class="info-row"><span class="k">Топ-нода</span>
            <span class="v mono">${topNode ? esc(topNode[0])+' ('+Math.round(topNode[1]/Math.max(1,total)*100)+'%)' : '—'}</span></div>
        </div>`;
    }
  });
}

/* ── ВНЕШНИЕ СКВАДЫ ── */
registerPage({
  id:'squads-ext', title:'Внешние сквады', group:'Инфраструктура', icon:'globe',
  render(){
    return `
    <div class="page-head">
      <div><h1>Внешние сквады</h1><div class="desc">Отдают список ваших хостов наружу — партнёру или другой панели. Доступ по токену.</div></div>
      <div class="actions"><button class="btn primary" id="newExt">${I('plus',14)} Внешний сквад</button></div>
    </div>
    <div class="grid g2" id="extList" style="align-items:start"></div>
    <div class="card" style="margin-top:12px">
      <div class="section-h" style="margin:0 0 10px"><h2>${I('info',14)} Как этим пользуются</h2></div>
      <p class="sub-note">Партнёр запрашивает свои хосты и свою витрину по токену:</p>
      <div class="copy-block" style="margin:10px 0"><span class="txt mono">curl -H "x-squad-token: ТОКЕН" ${esc(location.origin)}/api/ext/hosts</span>
        <button data-copy="curl -H &quot;x-squad-token: ТОКЕН&quot; ${esc(location.origin)}/api/ext/hosts" data-copy-msg="Команда скопирована">${I('copy',14)}</button></div>
      <p class="sub-note">В ответе — только отмеченные вами хосты и брендинг партнёра. Токен показывается один раз, в базе лежит его хеш. Ограничение по адресам и частоте защищает от того, что утёкший токен будут выкачивать непрерывно.</p>
    </div>`;
  },
  async bind(root){
    const draw = async () => {
      let list;
      try { list = await API.call('/api/ext-squads'); }
      catch(e){ toast('Не загрузилось: '+e.message, 'err'); return; }

      root.querySelector('#extList').innerHTML = list.map(s=>`
        <div class="card" style="${s.is_active?'':'opacity:.6'}">
          <div style="display:flex;align-items:center;gap:10px">
            ${providerLogo(s.brand_name || s.name, s.logo_url, 34)}
            <div style="flex:1;min-width:0"><b style="font-size:13.5px">${esc(s.name)}</b>
              <span class="sub">${s.brand_name ? 'витрина: '+esc(s.brand_name) : 'витрина не настроена'}</span></div>
            <button class="btn ghost icon-only" data-emenu="${s.id}">${I('more',14)}</button>
          </div>
          <div style="display:flex;gap:16px;margin-top:12px">
            <div><div class="label" style="font-size:9.5px">Хостов</div>
              <div class="num" style="font-size:16px;font-weight:600">${s.host_count}</div></div>
            <div><div class="label" style="font-size:9.5px">Лимит</div>
              <div class="num" style="font-size:16px;font-weight:600">${s.rate_limit_per_min ? s.rate_limit_per_min+'/мин' : '∞'}</div></div>
            <div><div class="label" style="font-size:9.5px">Адреса</div>
              <div class="num" style="font-size:16px;font-weight:600">${(s.allowed_ips||[]).length || 'любые'}</div></div>
          </div>
          ${s.page_url ? `<div class="sub-note mono" style="margin-top:10px;word-break:break-all">${esc(s.page_url)}</div>` : ''}
        </div>`).join('') || `<div class="card sub-note">Внешних сквадов нет. Они нужны, только если вы делитесь хостами с кем-то ещё.</div>`;

      root.querySelectorAll('[data-emenu]').forEach(b=>b.addEventListener('click', e=>{
        const sq = list.find(x=>String(x.id)===e.currentTarget.dataset.emenu);
        menu(e.currentTarget, [
          {label:'Настройки', icon:'settings', onClick:()=>extSquadDrawer(sq, draw)},
          {label:'Перевыпустить токен', icon:'rotate', onClick:()=>confirmModal({
            title:'Перевыпустить токен?',
            text:'Старый перестанет работать сразу — потребитель потеряет доступ, пока вы не передадите новый.',
            okText:'Перевыпустить',
            onOk:async ()=>{
              try {
                const r = await API.call('/api/ext-squads/'+sq.id+'/rotate', { method:'POST' });
                tokenModal('Новый токен · '+sq.name, r.token);
                await draw();
              } catch(e){ toast('Не получилось: '+e.message, 'err'); }
            }})},
          {label:sq.is_active?'Выключить':'Включить', icon:sq.is_active?'ban':'power', onClick:async ()=>{
            try {
              await API.call('/api/ext-squads/'+sq.id, { method:'PATCH', body:{ is_active: !sq.is_active } });
              toast(sq.is_active ? 'Выключен' : 'Включён');
              await draw();
            } catch(e){ toast('Не получилось: '+e.message, 'err'); }
          }},
          '-',
          {label:'Удалить', icon:'trash', danger:true, onClick:()=>confirmModal({
            title:`Удалить «${esc(sq.name)}»?`, text:'Потребитель немедленно потеряет доступ к хостам.', okText:'Удалить',
            onOk:async ()=>{
              try {
                await API.call('/api/ext-squads/'+sq.id, { method:'DELETE' });
                toast('Удалён');
                await draw();
              } catch(e){ toast('Не удалился: '+e.message, 'err'); }
            }})},
        ]);
      }));
    };
    root.querySelector('#newExt').addEventListener('click', ()=>extSquadDrawer(null, draw));
    await draw();
  }
});

/* Токен показывается один раз: в базе только хеш. */
function tokenModal(title, token){
  openModal({
    title, icon:'key', size:'sm',
    body:`
      <div class="notice" style="background:color-mix(in srgb, var(--warn) 10%, transparent);border:1px solid color-mix(in srgb, var(--warn) 30%, transparent);color:var(--warn-ink);border-radius:10px;padding:11px 14px;font-size:12.5px;margin-bottom:14px">
        ${I('alert',13)} Показывается один раз — в базе хранится только хеш. Скопируйте сейчас.
      </div>
      <div class="copy-block"><span class="txt mono">${esc(token)}</span>
        <button data-copy>${I('copy',14)}</button></div>`,
    footer:`<div class="spacer"></div><button class="btn primary" data-close>Я скопировал</button>`,
    onMount(layer){
      layer.querySelector('[data-copy]').addEventListener('click', ()=>copyText(token, 'Токен скопирован'));
    }
  });
}

function extSquadDrawer(sq, onSaved){
  const isNew = !sq;
  openDrawer({
    title: isNew ? 'Новый внешний сквад' : 'Внешний сквад · '+esc(sq.name),
    sub:'Список хостов для стороннего потребителя', icon:'globe', size:'lg', className:'sectioned-dialog',
    body:`
      <div class="field"><label>Название у вас <span class="req">*</span></label>
        <input class="inp" id="exName" value="${isNew?'':esc(sq.name)}" placeholder="Партнёр «Северный»">
        <div class="hint">Как этот партнёр называется в вашей панели.</div></div>

      <div class="form-section"><h4>${I('globe',13)} Витрина партнёра</h4>
        <p class="sub-note" style="margin-bottom:12px">Внешний сквад — это отдельный VPN-проект на вашей инфраструктуре: у партнёра своя страница подписки, своё имя и своя поддержка. Его клиенты видят его марку, а не вашу.</p>
        <div class="two-col">
          <div class="field"><label>Название сервиса</label>
            <input class="inp" id="exBrand" value="${isNew?'':esc(sq.brand_name||'')}" placeholder="NordVPN Lite"></div>
          <div class="field"><label>Логотип</label>
            <input class="inp mono" id="exLogo" value="${isNew?'':esc(sq.logo_url||'')}" placeholder="https://…/logo.png"></div>
          <div class="field"><label>Ссылка поддержки</label>
            <input class="inp mono" id="exSupport" value="${isNew?'':esc(sq.support_url||'')}" placeholder="https://t.me/партнёр_бот"></div>
          <div class="field"><label>Адрес его страницы подписки</label>
            <input class="inp mono" id="exPage" value="${isNew?'':esc(sq.page_url||'')}" placeholder="https://sub.партнёр.com">
            <div class="hint">Для справки: панель туда не ходит.</div></div>
        </div>
      </div>
      <div class="two-col">
        <div class="field"><label>Лимит запросов</label>
          <div class="inp-group"><input class="inp num" id="exRate" type="number" min="1"
            value="${isNew?'':(sq.rate_limit_per_min||'')}" placeholder="∞"><span class="suffix">в минуту</span></div>
          <div class="hint">Утёкший токен иначе будут выкачивать непрерывно.</div></div>
        <div class="field"><label>Разрешённые адреса</label>
          <input class="inp mono" id="exIps" value="${isNew?'':((sq.allowed_ips||[]).join(', '))}" placeholder="1.2.3.4, 5.6.7.0/24">
          <div class="hint">Через запятую. Пусто — с любого адреса.</div></div>
      </div>
      <div class="form-section"><h4>${I('host',13)} Какие хосты отдавать</h4>
        <div class="chips-select">${DB.hosts.map(h=>`
          <span class="chip-opt ${(sq?.host_ids||[]).includes(Number(h.id))?'on':''}" data-host="${h.id}">${esc(h.remark)}</span>`).join('')
          || '<span class="sub-note">Хостов нет.</span>'}</div>
        <div class="hint" style="margin-top:8px">Отмеченные увидит потребитель. Остальные ваши хосты останутся скрытыми.</div></div>`,
    footer:`<div class="spacer"></div><button class="btn" data-close>Отмена</button>
            <button class="btn primary" data-save>${isNew?'Создать':'Сохранить'}</button>`,
    onMount(layer, close){
      layer.querySelectorAll('.chip-opt').forEach(c=>c.addEventListener('click', ()=>c.classList.toggle('on')));
      layer.querySelector('[data-save]').addEventListener('click', async ()=>{
        const name = layer.querySelector('#exName').value.trim();
        if (!name) { toast('Нужно название', 'err'); return; }
        const rate = layer.querySelector('#exRate').value.trim();
        const ips = layer.querySelector('#exIps').value.split(',').map(x=>x.trim()).filter(Boolean);
        const body = {
          name,
          rate_limit_per_min: rate === '' ? null : parseInt(rate, 10),
          allowed_ips: ips,
          host_ids: [...layer.querySelectorAll('.chip-opt.on')].map(c=>+c.dataset.host),
          brand_name: layer.querySelector('#exBrand').value.trim() || null,
          support_url: layer.querySelector('#exSupport').value.trim() || null,
          logo_url: layer.querySelector('#exLogo').value.trim() || null,
          page_url: layer.querySelector('#exPage').value.trim() || null,
        };
        try {
          if (isNew) {
            const r = await API.call('/api/ext-squads', { method:'POST', body });
            close();
            tokenModal('Внешний сквад создан', r.token);
          } else {
            await API.call('/api/ext-squads/'+sq.id, { method:'PATCH', body });
            close();
            toast('Сохранено');
          }
          await onSaved();
        } catch(e){ toast('Не сохранилось: '+e.message, 'err'); }
      });
    }
  });
}
