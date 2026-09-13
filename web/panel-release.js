/* Server checks GitHub; browsers never receive repository tokens or run shell commands. */
'use strict';
const PanelRelease = (() => {
  let state=null, pending=null, nextCheck=0;
  const repository='https://github.com/STEALTHNET-APP/STEALTHNET-SOFTWARE';
  const brandMark='<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" aria-hidden="true"><path d="M18.5 4H9L4 9l5 5h6l3 3-3 3H5.5M5.5 20H15l5-5-5-5H9L6 7l3-3" stroke-linejoin="round"/></svg>';
  const labels={update_available:'Доступно обновление',current:'Установлена последняя версия',ahead:'Ваша сборка новее опубликованной',no_releases:'Релизы ещё не опубликованы',unavailable:'Не удалось проверить GitHub'};
  function badge(){
    const b=document.getElementById('panelVersion');if(!b)return;
    const version=state?.installed?.version||PAGES._health?.version;
    b.innerHTML=brandMark+`<span>${version?'v'+esc(version):'Версия'}</span>`+(state?.status==='update_available'?'<i aria-hidden="true"></i>':'');
    b.classList.toggle('has-update',state?.status==='update_available');
    b.title='Версия панели'+(state?' · '+(labels[state.status]||'Не удалось проверить GitHub'):'');
    b.setAttribute('aria-label',b.title);b.onclick=open;
  }
  async function check(force=false){
    if(pending)return pending;
    if(!force&&state&&Date.now()<nextCheck){badge();return state;}
    pending=API.call('/api/system/release'+(force?'?force=true':'')).then(data=>{
      state=data;nextCheck=Date.now()+Math.max(1,Math.min(Number(data.check_interval_seconds)||900,900))*1000;badge();return state;
    }).finally(()=>{pending=null;});
    return pending;
  }
  function description(status){
    return ({update_available:'Перед обновлением будет создана резервная копия базы и настроек.',current:'Новых стабильных релизов с готовыми установочными файлами нет.',ahead:'На GitHub пока опубликована более ранняя версия.',no_releases:'Проверка работает. В репозитории пока нет опубликованного стабильного релиза.',unavailable:'GitHub не ответил или данные релиза неполные. Попробуйте повторить проверку позже.'})[status]||'Не удалось получить информацию о релизе.';
  }
  function content(data){
    const current=data.installed||{}, update=data.status==='update_available';
    const metadata=[['Версия',current.version?'v'+current.version:null],['Ветка',current.branch],['Собрано',current.built_at],['Номер сборки',current.build_number],['Архитектура',current.target],['Коммит',current.commit]].filter(([,v])=>v);
    const releaseURL=data.latest?.tag&&/^v?\d+\.\d+\.\d+$/.test(data.latest.tag)?repository+'/releases/tag/'+data.latest.tag:null;
    return `<div class="release-notice ${update?'is-update':''}" role="status"><span class="release-mark">${brandMark}</span><div><strong>${esc(labels[data.status]||labels.unavailable)}</strong><p>${esc(description(data.status))}</p>${update?`<b class="release-upgrade">v${esc(current.version)} ${I('arrowRight',14)} v${esc(data.latest.version)}</b>`:''}</div></div>
    <div class="release-facts">${metadata.map(([name,value])=>`<div><span>${esc(name)}</span><b>${esc(value)}</b></div>`).join('')}</div>
    ${!current.commit?'<p class="release-note">Эта сборка не содержит метаданных GitHub Actions.</p>':''}
    ${data.latest?.notes?`<details class="release-notes"><summary>Что изменилось в v${esc(data.latest.version)}</summary><pre>${esc(data.latest.notes)}</pre></details>`:''}
    <div class="release-command"><label>Обновление на сервере панели</label><div><code>cd /opt/stealthnet-software &amp;&amp; make update</code><button class="icon-btn" data-copy-update title="Скопировать команду" aria-label="Скопировать команду обновления">${I('copy',17)}</button></div><small>Запустите от root по SSH. Команда также доступна как <code>stealthnet update</code>.</small></div>
    <div class="release-links"><a class="btn" href="https://t.me/stealthnet_admin_panel" target="_blank" rel="noopener noreferrer">${I('external',16)} ${LANG==='en'?'Community':'Сообщество'}</a>${releaseURL?`<a class="btn primary" href="${esc(releaseURL)}" target="_blank" rel="noopener noreferrer">${I('external',16)} Открыть релиз</a>`:''}<a class="btn" href="${repository}" target="_blank" rel="noopener noreferrer">${I('github',16)} GitHub</a></div>
    <p class="release-note">${data.checked_at?'Проверено: '+esc(new Date(data.checked_at).toLocaleString(currentLang()))+'. ':''}Результат сохраняется на ${data.status==='unavailable'?'1 минуту':'15 минут'}.</p>`;
  }
  function open(){
    openModal({title:'Версия и обновления',sub:'STEALTHNET SOFTWARE',icon:'shieldCheck',size:'md',body:'<div class="release-panel" aria-live="polite"><div class="empty">Проверяем версию…</div></div>',
      footer:'<button class="btn" data-release-refresh>Проверить снова</button><div class="spacer"></div><button class="btn" data-close>Закрыть</button>',
      onMount(layer){
        const box=layer.querySelector('.release-panel'),refresh=layer.querySelector('[data-release-refresh]');
        async function render(force=false){
          refresh.disabled=true;
          try{const data=await check(force);if(!layer.isConnected)return;if(force&&data.cached)toast('GitHub проверялся менее минуты назад. Повторите чуть позже.');box.innerHTML=content(data);box.querySelector('[data-copy-update]').onclick=()=>copyText('cd /opt/stealthnet-software && make update','Команда обновления скопирована');}
          catch(error){if(layer.isConnected)box.innerHTML=`<div class="empty"><b>Не удалось проверить версию</b><span>${esc(error.message)}</span></div>`;}
          finally{if(layer.isConnected)refresh.disabled=false;}
        }
        refresh.onclick=()=>render(true);render();
      }});
  }
  function mount(){badge();check().catch(()=>{});}
  // Refresh after returning to a tab, without a timer in hidden admin pages.
  document.addEventListener('visibilitychange',()=>{if(!document.hidden&&document.getElementById('panelVersion'))mount();});
  return {mount,open};
})();
