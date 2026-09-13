'use strict';
const SelfstealUI = (() => {
  const text=(ru,en)=>LANG==='en'?en:ru, cache=new Map();
  const clone=v=>JSON.parse(JSON.stringify(v));
  const candidates=cfg=>(cfg.inbounds||[]).filter(i=>i.protocol==='vless'&&i.streamSettings?.security==='reality'&&['raw','tcp'].includes(i.streamSettings.network||'raw'));
  const defaults=(tag)=>({domain:'',inbound_tag:tag,template:'studio',language:LANG==='en'?'en':'ru',title:'',description:''});
  function bind(config){
    const site=config._selfsteal;if(!site)return config;
    const ib=(config.inbounds||[]).find(i=>i.tag===site.inbound_tag);
    if(!ib?.streamSettings?.realitySettings)return config;
    ib.port=443;ib.listen='0.0.0.0';const r=ib.streamSettings.realitySettings;
    r.target='127.0.0.1:9443';delete r.dest;r.serverNames=[site.domain||''];r.xver=0;
    return config;
  }
  async function templates(language){
    if(!cache.has(language))cache.set(language,API.call('/api/profiles/selfsteal/templates?lang='+language).catch(e=>{cache.delete(language);throw e;}));
    return cache.get(language);
  }
  function thumbnail(frame,html){frame.srcdoc=html;const box=frame.parentElement;const observer=new ResizeObserver(()=>{if(!box.isConnected){observer.disconnect();return;}frame.style.transform='scale('+(box.clientWidth/1100)+')';});observer.observe(box);}
  function mount(root,config,{read,change,isBusy=()=>false}){
    const eligible=candidates(config),site=config._selfsteal;
    root.hidden=!eligible.length&&!site;if(root.hidden)return;
    root.innerHTML=`<section class="ss-settings" data-no-i18n><h3>${text('Сайт для REALITY','Website for REALITY')}</h3><div class="ss-modes" role="group" aria-label="${text('Где находится сайт','Website location')}"><button type="button" class="btn" data-site-mode="external" aria-pressed="${!site}">${text('Сторонний сайт','External website')}</button><button type="button" class="btn" data-site-mode="own" aria-pressed="${!!site}">${text('Свой сайт на ноде · Selfsteal','Own website on the node · Selfsteal')}</button></div>${site?`<p class="hint">${text('Выберите один из девяти мини-сайтов. Агент установит его на ноду, получит сертификат и свяжет с REALITY. Сначала направьте домен на IP этой ноды и откройте TCP 80 и 443. В Cloudflare используйте «Только DNS».','Choose one of nine mini-sites. The agent installs it on the node, obtains a certificate and connects it to REALITY. First point the domain to this node’s IP and allow TCP 80 and 443. In Cloudflare, use DNS only.')}</p><p class="hint">${text('Один домен и отдельный профиль на ноду. Нужна обычная установка ноды с systemd; контейнер Docker для этого сценария не подходит. Уже работающий сайт на сервере автоматически не заменяется.','Use a separate domain and profile for each node. This scenario requires a native node installation with systemd; it does not run inside Docker. An existing website on the server is not replaced automatically.')}</p>${eligible.length>1?`<label class="field"><span>${text('Подключение для сайта','Website connection')}</span><select class="inp" id="ssInbound">${eligible.map(i=>`<option value="${esc(i.tag)}" ${i.tag===site.inbound_tag?'selected':''}>${esc(i.tag)}</option>`).join('')}</select></label>`:''}<div class="ss-gallery-head"><h4>${text('Оформление сайта','Website design')}</h4><label>${text('Язык сайта','Website language')}<select class="inp" id="ssLanguage"><option value="ru" ${site.language==='ru'?'selected':''}>Русский</option><option value="en" ${site.language==='en'?'selected':''}>English</option></select></label></div><div class="ss-gallery" id="ssGallery" aria-busy="true"><p>${text('Загружаем превью сайтов…','Loading website previews…')}</p></div><div class="ss-summary"><span>${text('Внешний HTTPS и VPN: 443','Public HTTPS and VPN: 443')}</span><span>${text('Сайт внутри ноды: 127.0.0.1:9443','Website inside the node: 127.0.0.1:9443')}</span><span>${text('SNI заполняется по домену ниже','SNI follows the domain below')}</span></div>`:''}</section>`;
    root.querySelectorAll('[data-site-mode]').forEach(b=>b.onclick=()=>{
      if(isBusy())return;try{const next=clone(read());if(b.dataset.siteMode==='own'){
        if(next._selfsteal)return;const ib=candidates(next)[0];if(!ib)return;
        next._selfsteal=defaults(ib.tag);bind(next);
      }else{if(!next._selfsteal)return;const ib=(next.inbounds||[]).find(i=>i.tag===next._selfsteal.inbound_tag);if(ib?.streamSettings?.realitySettings){const r=ib.streamSettings.realitySettings;r.target='{{target}}';r.serverNames=['{{serverName}}'];delete r.dest;r.xver=0;}delete next._selfsteal;}
      change(next);
      }catch(e){toast(ProfileWorkshop.errorText(e.message),'err');}
    });
    if(!site)return;
    const update=(key,value)=>{if(isBusy())return;try{const next=clone(read());if(key==='inbound_tag'){const old=next.inbounds.find(i=>i.tag===next._selfsteal.inbound_tag);const chosen=next.inbounds.find(i=>i.tag===value);if(old){old.port=chosen?.port===443?8443:chosen?.port||8443;old.streamSettings.realitySettings.target='{{target}}';old.streamSettings.realitySettings.serverNames=['{{serverName}}'];}}next._selfsteal[key]=value;change(bind(next));}catch(e){toast(ProfileWorkshop.errorText(e.message),'err');}};
    root.querySelector('#ssLanguage').onchange=e=>update('language',e.target.value);
    const inbound=root.querySelector('#ssInbound');if(inbound)inbound.onchange=e=>update('inbound_tag',e.target.value);
    const gallery=root.querySelector('#ssGallery');
    async function load(){try{
      const list=await templates(site.language);if(!gallery.isConnected)return;
      gallery.setAttribute('aria-busy','false');gallery.innerHTML=list.map(item=>`<article class="ss-design ${site.template===item.id?'selected':''}"><div class="ss-thumb" aria-hidden="true"><iframe title="${esc(item.name)}" tabindex="-1" sandbox="allow-scripts" loading="lazy"></iframe></div><div class="ss-design-text"><h5>${esc(item.name)}</h5><p>${esc(item.description)}</p><div class="ss-design-actions"><button type="button" class="btn sm" data-select-site="${esc(item.id)}" aria-pressed="${site.template===item.id}" aria-label="${text('Выбрать','Select')} ${esc(item.name)}">${site.template===item.id?I('check',14)+' '+text('Выбран','Selected'):text('Выбрать','Select')}</button><button type="button" class="btn sm" data-preview-site="${esc(item.id)}" aria-label="${text('Посмотреть','Preview')} ${esc(item.name)}">${text('Посмотреть','Preview')}</button></div></div></article>`).join('');
      [...gallery.querySelectorAll('iframe')].forEach((frame,i)=>thumbnail(frame,list[i].html));
      gallery.querySelectorAll('[data-select-site]').forEach(b=>b.onclick=()=>update('template',b.dataset.selectSite));
      gallery.querySelectorAll('[data-preview-site]').forEach(b=>b.onclick=()=>{if(isBusy())return;try{preview({...clone(read()._selfsteal),template:b.dataset.previewSite});}catch(e){toast(ProfileWorkshop.errorText(e.message),'err');}});
    }catch(e){if(!gallery.isConnected)return;gallery.setAttribute('aria-busy','false');gallery.innerHTML=`<p>${text('Не удалось загрузить превью.','Could not load previews.')}</p><button type="button" class="btn" id="ssRetry">${text('Повторить','Retry')}</button>`;gallery.querySelector('#ssRetry').onclick=load;}}
    load();
  }
  async function preview(site){
    try{const result=await API.call('/api/profiles/selfsteal/preview',{method:'POST',body:{...site,domain:site.domain||'example.com'}});
      openModal({title:text('Предпросмотр сайта','Website preview'),sub:text('Такой сайт увидит посетитель домена ноды','The website shown to visitors of the node domain'),icon:'globe',size:'xl',className:'ss-preview-modal',body:`<div class="ss-preview-controls" data-no-i18n><button class="btn sm" data-width="desktop" aria-pressed="true">${text('Компьютер','Desktop')}</button><button class="btn sm" data-width="mobile" aria-pressed="false">${text('Телефон','Phone')}</button></div><div class="ss-preview-stage"><iframe id="ssPreviewFrame" title="${text('Предпросмотр выбранного сайта','Selected website preview')}" sandbox="allow-scripts"></iframe></div>`,footer:`<div class="spacer"></div><button class="btn" data-close>${text('Закрыть','Close')}</button>`,onMount(l){
        const frame=l.querySelector('iframe'),stage=l.querySelector('.ss-preview-stage'),buttons=[...l.querySelectorAll('[data-width]')];
        frame.srcdoc=result.html;let mode=window.innerWidth<700?'mobile':'desktop';
        const resize=()=>{if(!l.isConnected){observer.disconnect();return;}const width=mode==='mobile'?390:1100,scale=Math.min(1,stage.clientWidth/width);if(!scale)return;frame.style.width=width+'px';frame.style.height=stage.clientHeight/scale+'px';frame.style.transform='scale('+scale+')';frame.style.left=(stage.clientWidth-width*scale)/2+'px';buttons.forEach(b=>b.setAttribute('aria-pressed',String(b.dataset.width===mode)));};
        const observer=new ResizeObserver(resize);observer.observe(stage);buttons.forEach(b=>b.onclick=()=>{mode=b.dataset.width;resize();});resize();
      }});
    }catch(e){toast(ProfileWorkshop.errorText(e.message),'err');}
  }
  function status(value){
    if(!value||value.phase==='disabled')return '';
    const phases={preparing:text('Подготовка сайта','Preparing website'),downloading:text('Загрузка веб-сервера','Downloading web server'),certificate:text('Получение сертификата','Obtaining certificate'),ready:text('Сайт и HTTPS готовы','Website and HTTPS ready'),error:text('Ошибка сайта','Website error')};
    return `<p class="ss-node-status"><b>Selfsteal · ${esc(phases[value.phase]||text('Ожидание','Waiting'))}</b>${value.domain?`<span>${esc(value.domain)}</span>`:''}${value.certificate_expires_at?`<span>${text('Сертификат до','Certificate expires')} ${esc(new Date(value.certificate_expires_at*1000).toLocaleDateString(LANG==='en'?'en-GB':'ru-RU'))}</span>`:''}${value.error?`<span class="pw-result err">${esc(ProfileWorkshop.errorText(value.error))}</span>`:''}</p>`;
  }
  return {mount,bind,defaults,preview,status};
})();
