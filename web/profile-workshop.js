/* Profile creation, reusable templates and reviewed publication. */
'use strict';
const ProfileWorkshop = (() => {
  const text=(ru,en)=>LANG==='en'?en:ru;
  const tr=s=>LANG==='en'?translatedText(s):s;
  function errorText(value){
    let s=String(value||'');
    if(s.includes(' / ')){const [en,...tail]=s.split(' / '),ru=tail.join(' / ');const detail=ru.match(/: (.*)$/s)?.[1];return LANG==='en'?en+(detail&&!en.includes(detail)?': '+detail:''):ru;}
    if(LANG!=='en')return s;
    const exact={
      'нет секции inbounds — ноде нечего слушать':'Missing inbounds section: the node has no listeners',
      'список inbounds пуст':'The inbounds list is empty',
      'нет outbounds — трафику некуда идти':'Missing outbounds: traffic has no destination',
      'нет api+stats со StatsService — трафик по клиентам собираться не будет':'Missing api/stats with StatsService: per-customer traffic will not be collected'
    };
    if(exact[s])return exact[s];
    const replacements=[
      [/^inbound #(\d+): пустой tag — на него ссылаются хосты и сквады$/,(_,n)=>`Inbound #${n}: tag is required for host and squad bindings`],
      [/^tag «(.+)» повторяется — теги должны быть уникальны$/,(_,tag)=>`Duplicate tag: ${tag}. Tags must be unique`],
      [/^порт (\d+) занят дважды$/,(_,port)=>`Port ${port} is used by conflicting listeners`],
      [/^движок: /,'Xray: '],
      [/не указан protocol/,'protocol is required'],[/недопустимый порт /,'invalid port '],
      [/reality без privateKey — сгенерируйте ключи/,'Reality requires a privateKey; generate keys'],
      [/reality без сайта для маскировки — задайте target/,'Reality requires a destination; specify target'],
      [/reality без serverNames/,'Reality requires serverNames'],
      [/пустой shortIds — допустимо, но лучше задать/,'shortIds is empty; set identifiers for your clients'],
      [/tls без сертификатов — работает только за обратным прокси/,'TLS certificates are missing; check TLS termination at the reverse proxy'],
      [/в конфиге прописаны clients — их подставляет агент, список будет перезаписан/,'The configured clients list will be replaced by customers from the panel'],
      [/hysteria версии (.+) не поддерживается — укажите version: 2/,(_,v)=>`Hysteria version ${v} is unsupported; specify version: 2`],
      [/у hysteria не задан version — клиенты ожидают 2/,'Hysteria version is missing; clients expect version 2'],
      [/hysteria без TLS — клиенты подключаются к нему только по TLS/,'Hysteria requires TLS for client connections'],
      [/у hysteria transport должен быть "network": "hysteria", а не "(.+)" — иначе клиенты отвечают «not hysteria transport»/,(_,v)=>`Hysteria requires network: hysteria, not ${v}`],
      [/shadowsocks без settings.password — это серверный ключ, сгенерируйте его/,'Shadowsocks requires a server key in settings.password; generate it'],
      [/метод (.+) не поддерживает раздельные ключи клиентов — возьмите 2022-blake3-aes-128-gcm/,(_,m)=>`${m} does not support this panel's per-customer keys; use 2022-blake3-aes-128-gcm`],
      [/shadowsocks без settings.method/,'Shadowsocks requires settings.method'],
      [/grpc без serviceName — клиенты подключатся к пустому сервису/,'gRPC serviceName is empty; clients will use an empty service name']
    ];
    for(const [pattern,replacement] of replacements)s=s.replace(pattern,replacement);
    return tr(s);
  }
  const clone=v=>JSON.parse(JSON.stringify(v));
  const blank=()=>({log:{loglevel:'warning'},inbounds:[],outbounds:[{tag:'direct',protocol:'freedom'},{tag:'block',protocol:'blackhole'}],routing:{rules:[]}});
  const call=(path,body,method='POST')=>API.call(path,{method,body});
  const missing=s=>typeof s==='string'&&/#REPLACE|YOUR_|ВАШ_|СЕРВЕРНЫЙ_КЛЮЧ|\/ПУТЬ\/К\/|ВТОРАЯ_НОДА|UUID_СЕРВИСНОГО|ПУБЛИЧНЫЙ_КЛЮЧ|\{\{|<REQUIRED|CHANGE_ME/i.test(s);
  function leaves(v,path='',out=[]){if(v&&typeof v==='object')Object.entries(v).forEach(([k,v])=>leaves(v,path+'/'+k.replace(/~/g,'~0').replace(/\//g,'~1'),out));else if(missing(v))out.push(path);return out;}
  const ptr=p=>p.split('/').slice(1).map(k=>k.replace(/~1/g,'/').replace(/~0/g,'~'));
  function get(v,p){for(const k of ptr(p)){v=v?.[k];}return v;}
  function set(v,p,value){const keys=ptr(p);if(keys.some(k=>['__proto__','constructor','prototype'].includes(k)))throw Error(text('Небезопасный путь JSON','Unsafe JSON path'));let obj=v;for(const k of keys.slice(0,-1)){if(!Object.hasOwn(obj,k))throw Error('Missing JSON path');obj=obj[k];}obj[keys.at(-1)]=value;}
  function parameters(cfg){
    const out=[];
    const add=(path,label,hint,extra={})=>{
      if(!out.some(p=>p.path===path))out.push({path,label,hint,kind:'text',...extra});
    };
    (Array.isArray(cfg.inbounds)?cfg.inbounds:[]).forEach((ib,i)=>{
      if(!ib||ib.protocol==='dokodemo-door')return;
      const base='/inbounds/'+i,group=text('Подключение','Connection')+' '+String(ib.protocol||i+1).toUpperCase()+(ib.streamSettings?.security==='reality'?' · REALITY':'');
      const field=(path,label,hint,extra={})=>add(base+path,label,hint,{group,groupId:base,...extra});
      const r=ib.streamSettings?.realitySettings;
      if(r&&typeof r==='object'){
        const rp='/streamSettings/realitySettings/';
        field(rp+(Object.hasOwn(r,'target')?'target':Object.hasOwn(r,'dest')?'dest':'target'),text('Сайт для маскировки','Camouflage website'),text('Укажите домен публичного HTTPS-сайта. Свой домен покупать не нужно. Панель подготовит адрес назначения и имя сайта для подключения.','Enter the domain of a public HTTPS website. You do not need to buy a domain. The panel will prepare the destination and connection server name.'),{kind:'website',example:'example.com',required:true});
        field(rp+'serverNames',text('Имена сайта для подключения (SNI)','Connection server names (SNI)'),text('Обычно совпадают с сайтом выше и заполняются автоматически. Для особой настройки укажите имена из сертификата этого сайта, по одному на строку.','Usually matches the website above and is filled automatically. For a custom setup, enter names covered by that website’s certificate, one per line.'),{kind:'list',advanced:true,required:true});
        field(rp+'privateKey',text('Приватный ключ сервера','Server private key'),text('Будет создан при проверке. Существующий ключ сохраняется; менять его для обычной настройки не нужно.','Generated when you validate. An existing key is kept; you do not need to change it for a standard setup.'),{kind:'password',advanced:true,generated:true});
        field(rp+'shortIds',text('Идентификаторы подключения','Connection identifiers'),text('Создаются автоматически. Для ручной настройки — по одному на строку, до 16 символов 0–9 и a–f, чётная длина. Пустой идентификатор можно задать во вкладке JSON.','Generated automatically. For manual setup, enter one per line: up to 16 characters, 0–9 and a–f, with an even length. An empty identifier can be set in JSON.'),{kind:'list',advanced:true,generated:true});
        if(Object.hasOwn(r,'target')&&Object.hasOwn(r,'dest'))field(rp+'dest',text('Прежний адрес назначения (dest)','Legacy destination (dest)'),text('В импортированном документе заданы оба адреса. Проверьте их приоритет для своей версии Xray во вкладке JSON.','The imported document contains both destination fields. Check their precedence for your Xray version in JSON.'),{advanced:true});
      }
      const network=ib.streamSettings?.network;
      const portHint=['kcp','hysteria'].includes(network)?text('Откройте этот порт для UDP в сетевом экране VPN-сервера. Разрешения только для TCP недостаточно.','Allow this UDP port through the VPN server firewall. Allowing TCP alone is not enough.'):
        ['ws','xhttp','httpupgrade'].includes(network)&&ib.listen==='127.0.0.1'?text('Внутренний порт Xray. Укажите этот же порт в настройке обратного прокси; клиенты подключаются к внешнему HTTPS-порту прокси.','The internal Xray port. Use the same port in the reverse proxy configuration; clients connect to the proxy’s external HTTPS port.'):
        ib.protocol==='shadowsocks'?text('Свободный порт VPN-сервера. Если используются TCP и UDP, разрешите оба типа трафика на этом порту.','An available VPN server port. If TCP and UDP are used, allow both types of traffic on this port.'):
        text('Обычно 443. Он должен быть свободен на VPN-сервере и открыт в его сетевом экране. Если здесь уже работает сайт, выберите другой порт.','Usually 443. It must be available on the VPN server and allowed through its firewall. If a website already uses it, choose another port.');
      field('/port',text('Порт подключения','Connection port'),portHint,{kind:'number',required:true,example:'443'});
      field('/tag',text('Внутреннее имя подключения','Internal connection name'),text('Уникальное имя, по которому хосты и группы доступа находят это подключение. В готовом сценарии уже заполнено.','A unique name used by hosts and access groups to identify this connection. Ready-made scenarios already fill it in.'),{advanced:true,required:true});
      if(ib.listen!==undefined)field('/listen',text('На каких адресах принимать подключения','Addresses to accept connections on'),text('0.0.0.0 — на всех IPv4-адресах сервера. Меняйте только если нужно ограничить конкретным адресом или использовать IPv6.','0.0.0.0 accepts connections on all server IPv4 addresses. Change it only to bind a specific address or use IPv6.'),{advanced:true});
      for(const [path,key] of [['wsSettings','path'],['xhttpSettings','path'],['grpcSettings','serviceName'],['httpupgradeSettings','path']]){
        const value=ib.streamSettings?.[path]?.[key];if(value===undefined)continue;
        field('/streamSettings/'+path+'/'+key,key==='path'?text('Путь подключения','Connection path'):text('Имя сервиса gRPC','gRPC service name'),text('Должно совпадать с настройками обратного прокси и хоста в подписке.','Must match the reverse proxy and subscription host settings.'),{example:key==='path'?'/vpn':'vpn'});
      }
      const certs=ib.streamSettings?.tlsSettings?.certificates;
      if(Array.isArray(certs))certs.forEach((c,j)=>{for(const k of ['certificateFile','keyFile'])if(c[k]!==undefined)field('/streamSettings/tlsSettings/certificates/'+j+'/'+k,k==='certificateFile'?text('Файл сертификата TLS','TLS certificate file'):text('Файл приватного ключа TLS','TLS private key file'),text('Полный путь к существующему файлу на VPN-сервере. Этот мастер не выпускает и не загружает сертификаты.','Full path to an existing file on the VPN server. This wizard does not issue or upload certificates.'),{required:true});});
      if(ib.protocol==='shadowsocks'){
        field('/settings/method',text('Шифрование Shadowsocks','Shadowsocks encryption'),text('Для отдельных ключей клиентов нужен Shadowsocks 2022. Готовый сценарий уже содержит подходящий метод.','Individual customer keys require Shadowsocks 2022. The ready-made scenario includes a suitable method.'),{advanced:true,required:true});
        field('/settings/password',text('Серверный ключ Shadowsocks','Shadowsocks server key'),text('Создаётся при проверке. Существующий ключ не заменяется.','Generated on validation. An existing key is not replaced.'),{kind:'password',advanced:true,generated:true});
      }
    });
    const names={address:['Адрес следующего сервера','Upstream server address'],id:['UUID на следующем сервере','Upstream server UUID'],serverName:['Имя сайта на следующем сервере (SNI)','Upstream server name (SNI)'],publicKey:['Публичный ключ следующего сервера','Upstream public key'],shortId:['Идентификатор на следующем сервере','Upstream connection identifier'],password:['Пароль подключения','Connection password'],host:['Домен обратного прокси','Reverse proxy domain']};
    for(const path of leaves(cfg)){
      if(out.some(p=>path===p.path||path.startsWith(p.path+'/')))continue;
      const key=ptr(path).at(-1),known=names[key],upstream=path.startsWith('/outbounds/');
      add(path,known?text(...known):text('Параметр шаблона: ','Template parameter: ')+key,upstream?text('Возьмите это значение из настроек следующего сервера. Панель не может создать его вместо владельца того сервера.','Copy this value from the upstream server’s settings. The panel cannot generate it on behalf of that server’s owner.'):text('Это дополнительный параметр импортированного шаблона. Его назначение уточните в инструкции автора; точное место доступно в JSON.','This is an additional imported template parameter. Consult the author’s guide for its purpose; its exact location is available in JSON.'),{group:upstream?text('Следующий сервер','Upstream server'):text('Дополнительные параметры шаблона','Additional template parameters'),groupId:upstream?'/outbounds/'+ptr(path)[1]:'custom',required:true,kind:/privateKey|password|token|auth/i.test(path)?'password':'text'});
    }
    return out;
  }
  function websiteAddress(value){
    const raw=value.trim();
    const fail=()=>{throw Error(text('Введите домен сайта, например example.com. Можно добавить https:// и порт. Путь страницы, логин и пароль не нужны.','Enter a website domain, for example example.com. You may include https:// and a port. A page path, username or password is not needed.'));};
    if(!raw||/[\s\\]/.test(raw)||(!/^https:\/\//i.test(raw)&&raw.includes('://'))||/[?#]/.test(raw))return fail();
    let url;try{url=new URL(/^https:\/\//i.test(raw)?raw:'https://'+raw);}catch{return fail();}
    const host=url.hostname.toLowerCase();
    if(url.protocol!=='https:'||url.username||url.password||(url.pathname!=='/'&&url.pathname!=='')||!host.includes('.')||host.length>253||host.split('.').some(x=>!/^[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?$/.test(x))||/^[\d.]+$/.test(host))return fail();
    const port=Number(url.port||443);if(!Number.isInteger(port)||port<1||port>65535)return fail();
    return {host,target:host+':'+port};
  }
  function displayValue(cfg,p){
    const value=get(cfg,p.path);
    if(value===undefined||missing(value))return '';
    if(p.kind==='list')return Array.isArray(value)?value.filter(v=>!missing(v)).join('\n'):JSON.stringify(value);
    if(p.kind==='website')try{const a=websiteAddress(String(value));return a.target.endsWith(':443')?a.host:a.target;}catch{}
    return String(value);
  }
  // Only changed controls are written back. Imported values (including [""])
  // and unknown JSON fields survive a form → JSON → form round trip verbatim.
  function applyFields(cfg,params,values){
    const next=clone(cfg),sites=[];
    for(const {index,value} of values){
      const p=params[index];if(value===displayValue(cfg,p))continue;
      try{
        let parsed=value;
        if(p.kind==='number'){
          parsed=Number(value);if(!value.trim()||!Number.isInteger(parsed)||parsed<1||parsed>65535)throw Error(text('Укажите целый порт от 1 до 65535.','Enter a whole port number from 1 to 65535.'));
        }else if(p.kind==='list'){
          if(value.trim().startsWith('[')){parsed=JSON.parse(value);if(!Array.isArray(parsed)||parsed.some(v=>typeof v!=='string'))throw Error(text('Введите значения по одному на строку.','Enter one value per line.'));}
          else parsed=value.split(/[\n,]/).map(v=>v.trim()).filter(Boolean);
        }else if(p.kind==='website'){
          sites.push({p,value});continue;
        }
        const parent=p.path.slice(0,p.path.lastIndexOf('/'));
        if(/^\/inbounds\/\d+\/settings$/.test(parent)&&get(next,parent)===undefined)set(next,parent,{});
        set(next,p.path,parsed);
      }catch(e){e.paramPath=p.path;throw e;}
    }
    for(const {p,value} of sites){
      try{
        if(!value.trim()){set(next,p.path,'');continue;}
        const a=websiteAddress(value),parent=p.path.slice(0,p.path.lastIndexOf('/')),names=get(next,parent+'/serverNames');
        let oldHost;try{oldHost=websiteAddress(String(get(cfg,p.path)||'')).host;}catch{}
        set(next,p.path,a.target);
        // Keep manually chosen SNI names, including multi-domain lists.
        if(!Array.isArray(names)||!names.length||names.some(missing)||names.every(v=>!v)||names.length===1&&names[0]===oldHost)set(next,parent+'/serverNames',[a.host]);
      }catch(e){e.paramPath=p.path;throw e;}
    }
    // Also complete imported targets whose SNI has not been supplied yet.
    for(const p of params.filter(p=>p.kind==='website')){
      const parent=p.path.slice(0,p.path.lastIndexOf('/')),names=get(next,parent+'/serverNames');
      if(!Array.isArray(names)||!names.length||names.some(missing)||names.every(v=>!v)){
        try{set(next,parent+'/serverNames',[websiteAddress(String(get(next,p.path)||'')).host]);}catch{}
      }
    }
    return next;
  }
  function fieldIssues(cfg,params){
    const paths=new Set(leaves(cfg));
    for(const p of params){
      const v=get(cfg,p.path);
      if(p.required&&(v===undefined||v===null||typeof v==='string'&&!v.trim()||Array.isArray(v)&&(!v.length||v.every(x=>!x))))paths.add(p.path);
      if(p.kind==='number'&&(!Number.isInteger(v)||v<1||v>65535))paths.add(p.path);
    }
    const issues=new Map();
    for(const path of paths){
      let p=params.find(p=>path===p.path||path.startsWith(p.path+'/'));
      if(!p||p.generated)continue;
      if(p.path.endsWith('/serverNames')){
        const site=params.find(s=>s.kind==='website'&&s.groupId===p.groupId);
        if(site&&(!get(cfg,site.path)||missing(get(cfg,site.path))))p=site;
      }
      issues.set(p.path,p);
    }
    return [...issues.values()];
  }
  function builtins(){return PROFILE_PRESETS.map(p=>{
    const cfg=JSON.parse(p.config);for(const ib of cfg.inbounds||[]){const r=ib.streamSettings?.realitySettings;if(r){delete r.dest;r.target='{{target}}';r.serverNames=['{{serverName}}'];}}
    const group=p.id==='reality-plus-ws'?'multi':/ws|xhttp|httpupgrade/.test(p.id)?'proxy':'direct';return {...p,config:cfg,group,builtin:true,version:'2026.09',description:tr(p.desc)};
  }).concat([{
    id:'chain',name:'VLESS → VLESS Reality',group:'chain',builtin:true,version:'2026.09',
    description:text('Вход на первом сервере, выход через второй. Нужны действующие реквизиты второго сервера.','Inbound on the first server, outbound through a second server. Requires the second server’s real credentials.'),
    config:{...blank(),
      inbounds:[{tag:'entry',listen:'0.0.0.0',port:443,protocol:'vless',settings:{clients:[],decryption:'none'},streamSettings:{network:'raw',security:'reality',realitySettings:{target:'{{target}}',serverNames:['{{serverName}}'],privateKey:'{{privateKey}}',shortIds:['{{shortId}}']}}}],
      outbounds:[{tag:'chain',protocol:'vless',settings:{vnext:[{address:'{{upstreamAddress}}',port:443,users:[{id:'{{upstreamUUID}}',encryption:'none',flow:'xtls-rprx-vision'}]}]},streamSettings:{network:'raw',security:'reality',realitySettings:{serverName:'{{upstreamSNI}}',fingerprint:'chrome',publicKey:'{{upstreamPublicKey}}',shortId:'{{upstreamShortId}}'}}},...blank().outbounds],
      routing:{rules:[{type:'field',inboundTag:['entry'],outboundTag:'chain'}]}
    }
  }]);}

  function download(config,name){const blob=new Blob([JSON.stringify(config,null,2)],{type:'application/json'}),url=URL.createObjectURL(blob),a=document.createElement('a');a.href=url;a.download=name.replace(/[^a-z0-9_.-]/gi,'_')+'.json';a.click();setTimeout(()=>URL.revokeObjectURL(url),1000);}
  const msg=(el,s,kind='')=>{el.className='pw-result '+kind;el.textContent=s;el.setAttribute('role','status');};
  function requirements(cfg){const lines=new Set();for(const i of cfg.inbounds||[]){const s=i.streamSettings||{};
    if(s.security==='reality')lines.add(text('Для маскировки выберите публичный сайт с HTTPS, TLS 1.3 и HTTP/2, доступный с вашего VPN-сервера. Свой сайт и сертификат для REALITY не требуются. Проверка ниже проверяет конфигурацию, но не доступность выбранного сайта: после установки проверьте подключение с ноды и в VPN-приложении.', 'For camouflage, choose a public HTTPS website supporting TLS 1.3 and HTTP/2 that your VPN server can reach. REALITY does not require your own website or certificate. Validation checks the configuration, not the selected website’s reachability: after installation, test from the node and in a VPN app.'));
    if(s.security==='tls')lines.add(text('TLS: сертификат и приватный ключ должны существовать по указанным путям на каждой ноде.','TLS: the certificate and private key must exist at the specified paths on every node.'));
    if(['ws','xhttp','httpupgrade'].includes(s.network)&&s.security!=='tls')lines.add(text('Прокси/CDN: TLS завершает внешний прокси. Порт, путь и Host должны соответствовать его настройкам.','Proxy/CDN: TLS terminates at the external proxy. Port, path and Host must match its settings.'));
    if(i.protocol==='shadowsocks')lines.add(text('Подписки панели поддерживают Shadowsocks 2022 с отдельными ключами клиентов.','Panel subscriptions support Shadowsocks 2022 with individual client keys.'));
    if(!['vless','vmess','trojan','shadowsocks','hysteria','dokodemo-door','socks','http'].includes(i.protocol))lines.add(text('Этот протокол требует ручной проверки совместимости с учётом трафика и клиентскими подписками.','This protocol needs a manual check of traffic accounting and subscription compatibility.'));
  }lines.add(text('Проверка на панели использует её установленный Xray. Тестовая нода проверяет конфиг своей версией. Подключение клиентом проверяется отдельно.','Panel validation uses its installed Xray. A test node validates with its own version. Verify an actual client connection separately.'));return [...lines];}
  async function wizard(options={}){
    let config=options.config?clone(options.config):blank(),library=[],mode=options.config?'import':'preset',view='form',selected=null,dirty=false;
    try{library=await API.call('/api/profile-templates');}catch(e){toast(errorText(e.message),'err');return;}
    const all=[...builtins(),...library.map(p=>({...p,id:'custom-'+p.id,group:'custom',description:LANG==='en'?p.description_en:p.description_ru}))];
    const drafts=new Map();let currentParams=[];
    openModal({title:options.onApply?text('Параметры конфигурации','Configuration parameters'):text('Новый профиль','New profile'),icon:'json',size:'xl',className:'profile-workshop',initialFocus:'title',
      sub:text('Сценарий → параметры → проверка → профиль','Scenario → parameters → validation → profile'),
      body:`<div class="pw-content" data-no-i18n><label class="field"><span>${text('Название профиля','Profile name')}</span><input class="inp" id="pwName" maxlength="160" value="${esc(options.name||'')}"></label>
      <div class="pw-modes" role="group" aria-label="${text('Способ создания','Creation method')}">${[['preset',text('Готовый сценарий','Ready-made scenario'),'layers'],['import',text('Импорт JSON','Import JSON'),'download'],['blank',text('Пустой профиль','Blank profile'),'json']].map(([id,label,icon])=>`<button class="btn" data-mode="${id}" aria-pressed="${mode===id}">${I(icon,16)} ${label}</button>`).join('')}</div>
      <div id="pwSources"></div><details class="pw-requirements"><summary>${text('Что подготовить для этого сценария','What to prepare for this scenario')}</summary><div id="pwRequirements"></div></details>
      <div class="pw-toolbar"><button class="btn sm" id="pwForm">${text('Параметры','Parameters')}</button><button class="btn sm" id="pwJson">JSON</button><button class="btn sm" id="pwTemplate">${text('Сохранить шаблон','Save template')}</button></div>
      <div id="pwFields"><div id="pwBasicFields"></div><details id="pwAdvanced" class="pw-advanced"><summary>${text('Расширенные настройки','Advanced settings')}<small>${text('Имена подключений, ключи и сетевые параметры','Connection names, keys and network settings')}</small></summary><div id="pwAdvancedFields"></div><button class="btn sm" id="pwGenerate">${I('key',14)} ${text('Заполнить недостающие ключи','Generate missing keys')}</button><details class="pw-technical"><summary>${text('Где находятся поля в JSON','Field locations in JSON')}</summary><div id="pwPaths"></div></details></details></div><label id="pwJsonWrap" hidden><span>${text('Полная конфигурация JSON','Full JSON configuration')}</span><textarea class="inp mono pw-json" id="pwJsonArea" spellcheck="false" data-no-i18n></textarea></label>
      <div id="pwInbounds"></div><div id="pwResult" class="pw-result" role="status"></div></div>`,
      footer:`<button class="btn" data-close>${text('Отмена','Cancel')}</button><div class="spacer"></div><button class="btn" id="pwValidate">${text('Проверить','Validate')}</button><button class="btn primary" id="pwCreate">${options.onApply?text('Вернуть в редактор','Use in editor'):text('Создать профиль','Create profile')}</button>`,
      onMount(l,close){
        const q=s=>l.querySelector(s),result=q('#pwResult'),area=q('#pwJsonArea');let changed=false;
        function focusField(path,message){
          const i=currentParams.findIndex(p=>p.path===path),el=q('[data-param="'+i+'"]');
          if(!el)return;
          if(currentParams[i].advanced)q('#pwAdvanced').open=true;
          if(message){el.setAttribute('aria-invalid','true');q('#pwError'+i).textContent=message;}
          if(el.disabled)setTimeout(()=>{if(el.isConnected&&!el.disabled)el.focus();},0);else el.focus();el.scrollIntoView({block:'center',behavior:'smooth'});
        }
        function read(){
          if(view==='form'){
            try{config=applyFields(config,currentParams,[...q('#pwFields').querySelectorAll('[data-param]')].map(el=>({index:+el.dataset.param,value:el.value})));q('#pwFields').querySelectorAll('[data-param]').forEach(el=>el.value=displayValue(config,currentParams[+el.dataset.param]));}
            catch(e){focusField(e.paramPath,e.message);throw e;}
          }else{
            if(area.value.length>524288)throw Error(text('JSON больше 512 КиБ','JSON exceeds 512 KiB'));
            config=JSON.parse(area.value);if(!config||Array.isArray(config)||typeof config!=='object')throw Error(text('Нужен JSON-объект','JSON object required'));
          }
          return config;
        }
        function redraw(){
          currentParams=parameters(config);
          const renderField=(p,i)=>{
            const value=displayValue(config,p),attrs=`id="pwParam${i}" data-param="${i}" aria-labelledby="pwLabel${i}" aria-describedby="pwHint${i} pwError${i}" autocomplete="off" spellcheck="false" ${p.example?`placeholder="${esc(p.example)}"`:''}`;
            const input=p.kind==='list'?`<textarea class="inp pw-list" rows="2" ${attrs}>${esc(value)}</textarea>`:`<input class="inp" type="${p.kind==='password'?'password':p.kind==='number'?'number':'text'}" ${p.kind==='number'?'min="1" max="65535" step="1"':''} ${attrs} value="${esc(value)}">`;
            return `<label class="field" for="pwParam${i}"><span id="pwLabel${i}">${esc(p.label)}</span>${input}<small class="hint" id="pwHint${i}">${esc(p.hint)}</small><small class="pw-field-error" id="pwError${i}"></small></label>`;
          };
          const renderGroups=advanced=>{
            const groups=new Map();currentParams.forEach((p,i)=>{if(Boolean(p.advanced)!==advanced)return;if(!groups.has(p.groupId))groups.set(p.groupId,{name:p.group,html:[]});groups.get(p.groupId).html.push(renderField(p,i));});
            return [...groups.values()].map(g=>`<section class="pw-field-group"><h3>${esc(g.name)}</h3><div class="pw-fields">${g.html.join('')}</div></section>`).join('');
          };
          q('#pwBasicFields').innerHTML=renderGroups(false)||`<p class="hint">${text('Добавьте подключения во вкладке JSON или выберите готовый сценарий.','Add connections in JSON or choose a ready-made scenario.')}</p>`;
          q('#pwAdvancedFields').innerHTML=renderGroups(true);
          q('#pwAdvanced').hidden=!currentParams.length;
          q('#pwPaths').innerHTML=currentParams.map(p=>`<p><b>${esc(p.label)}</b><br><code>${esc(p.path)}</code></p>`).join('');
          area.value=JSON.stringify(config,null,2);q('#pwFields').hidden=view!=='form';q('#pwJsonWrap').hidden=view!=='json';
          q('#pwForm').setAttribute('aria-pressed',view==='form');q('#pwJson').setAttribute('aria-pressed',view==='json');
          q('#pwRequirements').innerHTML=requirements(config).map(x=>`<p>${esc(x)}</p>`).join('');
          q('#pwInbounds').textContent=currentParams.some(p=>p.generated)?text('Ключи будут подготовлены автоматически при проверке. Уже заданные ключи сохранятся.','Keys will be prepared automatically when you validate. Existing keys will be kept.'):'';
          if(busy)q('#pwFields').querySelectorAll('input,textarea').forEach(el=>el.disabled=true);
          q('#pwFields').querySelectorAll('[data-param]').forEach(el=>el.addEventListener('input',()=>{
            el.removeAttribute('aria-invalid');q('#pwError'+el.dataset.param).textContent='';dirty=true;changed=true;msg(result,text('Есть непроверенные изменения','Unvalidated changes'));
          }));
        }
        // Lock the draft while preparing/validating, so a late response cannot
        // overwrite text entered during the request or submit the draft twice.
        let busy=false;
        async function working(fn){
          if(busy)return;busy=true;
          const controls=[...l.querySelectorAll('.pw-content input,.pw-content textarea,.pw-content select,.pw-content button,.m-foot button:not([data-close])')],states=controls.map(el=>el.disabled);
          controls.forEach(el=>el.disabled=true);q('.pw-content').setAttribute('aria-busy','true');
          try{return await fn();}finally{controls.forEach((el,i)=>el.disabled=states[i]);q('#pwFields').querySelectorAll('input,textarea').forEach(el=>el.disabled=false);q('.pw-content').removeAttribute('aria-busy');busy=false;}
        }
        function choose(item){try{read();}catch(e){msg(result,errorText(e.message),'err');return;}if(dirty){confirmModal({title:text('Заменить черновик?','Replace draft?'),text:text('Выбранный шаблон заменит текущий JSON в этом окне. Сохраните его в библиотеку, если он нужен.','The selected template will replace this draft JSON. Save it to the library if you need it.'),okText:text('Заменить','Replace'),onOk:()=>{dirty=false;choose(item);}});return;}config=clone(item.config);selected=item;dirty=false;changed=true;redraw();l.querySelectorAll('[data-preset-id]').forEach(b=>b.classList.toggle('selected',b.dataset.presetId===String(item.id)));msg(result,text('Заполните поля ниже и нажмите «Проверить».','Complete the fields below and select Validate.'));}
        function source(){
          q('#pwSources').innerHTML=mode==='preset'?`<div class="pw-search"><input class="inp" id="pwSearch" placeholder="${text('Поиск сценария или шаблона','Search scenarios or templates')}"><select class="inp" id="pwGroup">${[['',text('Все сценарии','All scenarios')],['direct',text('Прямое подключение','Direct connection')],['proxy',text('Прокси / CDN','Proxy / CDN')],['multi',text('Несколько входов','Multiple inbounds')],['chain',text('Связь серверов','Server chaining')],['custom',text('Мои шаблоны','My templates')]].map(([v,t])=>`<option value="${v}">${t}</option>`).join('')}</select></div><div id="pwPresets" class="pw-presets"></div>`:mode==='import'?`<div class="pw-import"><label class="field"><span>${text('JSON-файл (до 512 КиБ)','JSON file (up to 512 KiB)')}</span><span class="btn pw-file-button" tabindex="0" role="button">${text('Выбрать файл','Choose file')}</span><input type="file" id="pwFile" accept=".json,application/json" class="pw-file-native"></label><label class="field"><span>${text('GitHub · raw-ссылка с полным commit','GitHub · raw URL pinned to a full commit')}</span><input class="inp" id="pwUrl" placeholder="https://raw.githubusercontent.com/owner/repo/commit/config.json"></label><button class="btn" id="pwFetch">${text('Загрузить JSON','Fetch JSON')}</button><p class="hint">${text('Можно также вставить весь документ во вкладке JSON. Загрузка не сохраняет профиль.','You can also paste the entire document in the JSON tab. Fetching does not save a profile.')}</p></div>`:`<p class="hint">${text('Начните с JSON-каркаса. Протоколы и маршрутизацию определяете вы.','Start with a JSON skeleton. You choose protocols and routing.')}</p>`;
          if(mode==='preset'){
            function list(){const search=q('#pwSearch').value.toLowerCase(),group=q('#pwGroup').value;const items=all.filter(p=>(!group||p.group===group)&&(p.name+' '+p.description).toLowerCase().includes(search));q('#pwPresets').innerHTML=items.map(p=>`<button class="pw-preset ${selected?.id===p.id?'selected':''}" data-preset-id="${esc(p.id)}"><b>${esc(tr(p.name))}</b><span>${esc(p.description)}</span><small>${p.builtin?'STEALTHNET':esc(p.author||text('Собственный шаблон','Custom template'))} · v${esc(p.version)}</small></button>`).join('')||`<p>${text('Шаблоны не найдены','No templates found')}</p>`;q('#pwPresets').querySelectorAll('[data-preset-id]').forEach(b=>b.onclick=()=>{choose(all.find(p=>String(p.id)===b.dataset.presetId));list();});}
            q('#pwSearch').oninput=list;q('#pwGroup').onchange=list;list();
          }
          if(mode==='import'){
            q('.pw-file-button').onkeydown=e=>{if(e.key==='Enter'||e.key===' '){e.preventDefault();q('#pwFile').click();}};
            async function load(value,source){if(value.config&&typeof value.config==='object')value=value.config;if(!value||Array.isArray(value)||typeof value!=='object')throw Error(text('Нужен JSON-объект','JSON object required'));config=clone(value);selected=source;view='json';redraw();dirty=true;}
            q('#pwFile').onchange=async e=>{try{const file=e.target.files[0];if(!file)return;if(file.size>524288)throw Error(text('Файл больше 512 КиБ','File exceeds 512 KiB'));await load(JSON.parse(await file.text()),null);msg(result,text('Файл загружен в черновик','File loaded into draft'));}catch(e){msg(result,errorText(e.message),'err');}};
            q('#pwFetch').onclick=async e=>{e.target.disabled=true;try{const r=await call('/api/profile-templates/import',{url:q('#pwUrl').value.trim()});await load(r.config,r);msg(result,'SHA256: '+r.sha256);}catch(e){msg(result,errorText(e.message),'err');}finally{e.target.disabled=false;}};
          }
        }
        l.querySelectorAll('[data-mode]').forEach(b=>b.onclick=()=>{try{read();drafts.set(mode,{config:clone(config),selected,view});mode=b.dataset.mode;const d=drafts.get(mode);config=d?.config||blank();selected=d?.selected||null;view=d?.view||(mode==='preset'?'form':'json');l.querySelectorAll('[data-mode]').forEach(x=>x.setAttribute('aria-pressed',x===b));source();redraw();}catch(e){msg(result,errorText(e.message),'err');}});
        q('#pwForm').onclick=()=>{try{read();view='form';redraw();}catch(e){msg(result,errorText(e.message),'err');}};
        q('#pwJson').onclick=()=>{try{read();view='json';redraw();}catch(e){msg(result,errorText(e.message),'err');}};
        area.oninput=()=>{dirty=true;changed=true;msg(result,text('Есть непроверенные изменения','Unvalidated changes'));};
        q('#pwGenerate').onclick=()=>working(async()=>{try{read();const r=await call('/api/profiles/prepare',{config});config=r.config;changed=true;redraw();msg(result,text('Ключи готовы. Заполните остальные поля и проверьте профиль.','Keys are ready. Complete the other fields and validate the profile.'));}catch(e){msg(result,errorText(e.message),'err');}});
        async function validate(){
          read();
          const params=parameters(config),issues=fieldIssues(config,params);
          if(issues.length){
            view='form';redraw();msg(result,text('Заполните выделенные поля, затем повторите проверку.','Complete the highlighted fields, then validate again.'),'err');
            issues.forEach(p=>{
              const i=currentParams.findIndex(x=>x.path===p.path),el=q('[data-param="'+i+'"]');
              if(el){el.setAttribute('aria-invalid','true');q('#pwError'+i).textContent=text('Заполните это поле.','Complete this field.');}
              const b=document.createElement('button');b.className='btn sm';b.textContent=p.label+(issues.filter(x=>x.label===p.label).length>1?' · '+p.group:'');b.onclick=()=>focusField(p.path);result.appendChild(b);
            });
            focusField(issues[0].path);return null;
          }
          msg(result,text('Подготавливаем ключи и проверяем конфигурацию…','Preparing keys and validating the configuration…'));
          const prepared=await call('/api/profiles/prepare',{config});config=prepared.config;
          // Keep preparation and validation together; do not redraw editable
          // inputs until the asynchronous work has finished.
          let r;try{r=await call('/api/profiles/validate',{config});}finally{redraw();}
          msg(result,[r.valid?text('Проверка пройдена','Validation passed'):text('Есть ошибки в конфигурации','There are configuration errors'),r.engine_checked?text('Проверено Xray панели. После установки проверьте подключение в VPN-приложении.','Checked by panel Xray. After installation, test the connection in a VPN app.'):text('Проверена только структура — Xray панели недоступен. Перед использованием проверьте на тестовой ноде.','Structure only — panel Xray unavailable. Validate on a test node before use.')].join('\n'),r.valid?'ok':'err');
          const diagnostics=[...(r.errors||[]),...(r.warnings||[])];
          if(diagnostics.length){const d=document.createElement('details'),summary=document.createElement('summary'),body=document.createElement('div');d.className='pw-technical';summary.textContent=text('Подробности проверки','Validation details');body.textContent=diagnostics.map(errorText).join('\n');d.append(summary,body);d.open=!r.valid;result.appendChild(d);}
          return r;
        }
        q('#pwValidate').onclick=()=>working(async()=>{try{await validate();}catch(e){msg(result,errorText(e.message),'err');}});
        q('#pwTemplate').onclick=()=>{try{read();saveTemplate(config,{name:q('#pwName').value,...(selected?.source_url?selected:{})});}catch(e){msg(result,errorText(e.message),'err');}};
        q('#pwCreate').onclick=()=>working(async()=>{try{const name=q('#pwName').value.trim();if(!name){q('#pwName').disabled=false;q('#pwName').focus();throw Error(text('Укажите название профиля','Enter a profile name'));}if(mode==='preset'&&!selected&&!options.config)throw Error(text('Выберите сценарий','Choose a scenario'));const r=await validate();if(!r?.valid)return;if(options.onApply){options.onApply(clone(config));close();return;}const created=await call('/api/profiles',{name,config});close();await refreshDB();navigate('config?id='+created.id);toast(text('Профиль создан','Profile created'));}catch(e){msg(result,errorText(e.message),'err');}});
        if(options.config)view='form';source();redraw();
      }
    });
  }
  async function saveTemplate(config,metadata={}){
    try{const clean=await call('/api/profile-templates/sanitize',{config});const original=metadata.config;openModal({title:text('Сохранить шаблон','Save template'),icon:'layers',size:'lg',className:'profile-workshop',body:`<div data-no-i18n><p class="hint">${text('Операторские значения заменены параметрами; списки клиентов удалены. Рабочие профили при обновлении шаблона не меняются.','Operator values are parameterized and client lists removed. Updating a template does not change existing profiles.')}</p>${[['name',text('Название','Name')],['description_ru',text('Описание · RU','Description · RU')],['description_en','Description · EN'],['author',text('Автор','Author')]].map(([k,t])=>`<label class="field"><span>${t}</span><input class="inp" data-meta="${k}" value="${esc(metadata[k]||'')}" maxlength="${k.startsWith('description')?4000:160}"></label>`).join('')}<label class="field"><span>JSON · ${text('предпросмотр без секретов','preview without secrets')}</span><textarea class="inp mono pw-json" id="ptJson" readonly>${esc(JSON.stringify(clean.config,null,2))}</textarea></label>${original?`<p>${text('Изменённые разделы','Changed sections')}: ${esc(Object.keys(clean.config).filter(k=>JSON.stringify(original[k])!==JSON.stringify(clean.config[k])).join(', ')||'—')}</p>`:''}</div>`,footer:`<button class="btn" id="ptDownload">${text('Скачать шаблон','Download template')}</button><div class="spacer"></div><button class="btn" data-close>${text('Отмена','Cancel')}</button><button class="btn primary" id="ptSave">${text('Сохранить','Save')}</button>`,onMount(l,close){l.querySelector('#ptDownload').onclick=()=>download(clean.config,metadata.name||'template');l.querySelector('#ptSave').onclick=async e=>{e.target.disabled=true;try{const body={config:clean.config,source_url:metadata.source_url||null,source_revision:metadata.source_revision||null,version:metadata.version};l.querySelectorAll('[data-meta]').forEach(el=>body[el.dataset.meta]=el.value);await call('/api/profile-templates'+(metadata.id?'/'+metadata.id:''),body,metadata.id?'PATCH':'POST');close();toast(text('Шаблон сохранён','Template saved'));}catch(e){toast(errorText(e.message),'err');}finally{e.target.disabled=false;}};}});}catch(e){toast(errorText(e.message),'err');}
  }
  async function library(){try{const list=await API.call('/api/profile-templates');openModal({title:text('Библиотека профилей','Profile library'),icon:'layers',size:'lg',className:'profile-workshop',body:`<div data-no-i18n><p>${text('Встроенные сценарии доступны в «Новом профиле». Здесь — ваши сохранённые шаблоны.','Built-in scenarios are available in New profile. Your saved templates are listed here.')}</p><div class="pw-library">${list.map(p=>`<section class="pw-library-card"><h4>${esc(p.name)} <small>v${p.version}</small></h4><p>${esc(LANG==='en'?p.description_en:p.description_ru)}</p><p class="hint">${esc(p.author)} ${esc(p.source_revision||'')}</p><div class="pw-toolbar"><button class="btn sm" data-use="${p.id}">${text('Использовать','Use')}</button><button class="btn sm" data-edit-template="${p.id}">${text('Изменить','Edit')}</button><button class="btn sm" data-export="${p.id}">${text('Скачать','Download')}</button><button class="btn sm danger" data-delete="${p.id}">${text('Удалить','Delete')}</button></div></section>`).join('')||`<p>${text('Сохранённых шаблонов пока нет. Создайте шаблон из JSON в редакторе.','No saved templates yet. Save a template from JSON in the editor.')}</p>`}</div></div>`,footer:`<button class="btn primary" id="plImport">${text('Импортировать шаблон','Import template')}</button><div class="spacer"></div><button class="btn" data-close>${text('Закрыть','Close')}</button>`,onMount(l,close){l.querySelector('#plImport').onclick=()=>wizard({config:blank(),name:''});l.querySelectorAll('[data-use]').forEach(b=>b.onclick=()=>{close();const p=list.find(p=>p.id===+b.dataset.use);wizard({config:p.config,name:p.name});});l.querySelectorAll('[data-edit-template]').forEach(b=>b.onclick=()=>{const p=list.find(p=>p.id===+b.dataset.editTemplate);openModal({title:text('Редактирование шаблона','Edit template'),icon:'json',size:'lg',body:`<textarea class="inp mono pw-json" id="plEditJson" spellcheck="false">${esc(JSON.stringify(p.config,null,2))}</textarea>`,footer:`<button class="btn" data-close>${text('Отмена','Cancel')}</button><div class="spacer"></div><button class="btn primary" id="plReview">${text('Предпросмотр изменений','Review changes')}</button>`,onMount(m,c){m.querySelector('#plReview').onclick=()=>{try{const cfg=JSON.parse(m.querySelector('#plEditJson').value);c();saveTemplate(cfg,p);}catch(e){toast(errorText(e.message),'err');}};}});});l.querySelectorAll('[data-export]').forEach(b=>b.onclick=()=>{const p=list.find(p=>p.id===+b.dataset.export);download({name:p.name,description_ru:p.description_ru,description_en:p.description_en,author:p.author,source_url:p.source_url,source_revision:p.source_revision,config:p.config},p.name);});l.querySelectorAll('[data-delete]').forEach(b=>b.onclick=()=>confirmModal({title:text('Удалить шаблон?','Delete template?'),text:text('Созданные по нему профили сохранятся.','Profiles created from it are kept.'),onOk:async()=>{await API.call('/api/profile-templates/'+b.dataset.delete,{method:'DELETE'});close();library();}}));}});}catch(e){toast(errorText(e.message),'err');}}
  function nodeRows(nodes){return `<div class="pw-node-status">${nodes.map(n=>`<div><b>${esc(n.name)}</b><span>Xray ${esc(n.engine_version||'—')}</span><span>${n.applied?text('Применено','Applied'):n.error?text('Ошибка','Error'):text('Ожидаем подтверждения','Waiting for confirmation')} · ${esc(n.reported_version??'—')} / ${esc(n.target_version)}</span>${n.error?`<p class="pw-result err">${esc(errorText(n.error))}</p>`:''}</div>`).join('')||`<p>${text('Нет назначенных нод','No assigned nodes')}</p>`}</div>`;}
  async function status(p){try{const nodes=await API.call('/api/profiles/'+p.id+'/status');openModal({title:text('Применение профиля','Profile deployment'),sub:esc(p.name),icon:'server',size:'lg',className:'profile-workshop',body:nodeRows(nodes)+`<p class="hint">${text('Подтверждение агента означает запуск конфигурации. Доступность VPN проверьте клиентским приложением.','Agent confirmation means the configuration started. Verify VPN connectivity with a client application.')}</p>`,footer:`<button class="btn" id="psRefresh">${text('Обновить','Refresh')}</button><div class="spacer"></div><button class="btn" data-close>${text('Закрыть','Close')}</button>`,onMount(l,c){l.querySelector('#psRefresh').onclick=()=>{c();status(p);};}});}catch(e){toast(errorText(e.message),'err');}}
  async function review(p,config,onSaved){
    try{const impact=await call('/api/profiles/'+p.id+'/impact',{config});if(Number(p.version)!==impact.version)throw Error(text('Профиль уже изменён. Откройте его заново.','The profile has changed. Reopen it.'));
      openModal({title:text('Проверка перед применением','Review before applying'),sub:esc(p.name),icon:'shieldCheck',size:'lg',className:'profile-workshop',body:`<div data-no-i18n><p>${text('Затронутые ноды','Affected nodes')}: <b>${impact.nodes.map(n=>esc(n.name)).join(', ')||'—'}</b></p>${impact.sensitive?`<div class="pw-result warn">${text('Меняются параметры подключения. После применения клиентам может потребоваться обновить подписку.','Connection settings are changing. Clients may need to refresh their subscriptions after applying.')}</div>`:''}<div class="pw-impact"><section><h4>${text('Хосты','Hosts')}</h4>${impact.hosts.map(h=>`<p>${esc(h.name)} · ${esc(h.inbound)} ${h.removed?'⚠ '+text('будет выключен и отвязан','will be disabled and unlinked'):''}</p>`).join('')||'—'}</section><section><h4>${text('Сквады','Squads')}</h4>${impact.squads.map(s=>`<p>${esc(s.name)} · ${esc(s.inbound)} ${s.removed?'⚠ '+text('связь будет удалена','link will be removed'):''}</p>`).join('')||'—'}</section></div><details><summary>${text('Изменённые поля','Changed fields')} · ${impact.changed_paths.length}</summary><pre class="pw-diff">${esc(impact.changed_paths.join('\n'))}</pre></details>${impact.removed.length?`<label class="check"><input id="prAcknowledge" type="checkbox">${text('Понимаю последствия удаления инбаундов','I understand the consequences of removing inbounds')}: ${impact.removed.map(esc).join(', ')}</label>`:''}<p class="hint">${text('Можно сначала проверить эту конфигурацию на отдельной свободной ноде. Рабочий профиль до применения не меняется.','You can rehearse this configuration on a separate free node first. The production profile remains unchanged until applied.')}</p><div id="prResult" class="pw-result" role="status"></div></div>`,footer:`<button class="btn" id="prTrial">${text('Тестовая нода','Test node')}</button><div class="spacer"></div><button class="btn" data-close>${text('Отмена','Cancel')}</button><button class="btn primary" id="prApply">${text('Применить к профилю','Apply to profile')}</button>`,onMount(l,close){l.querySelector('#prTrial').onclick=()=>trial(p,config,onSaved);l.querySelector('#prApply').onclick=async e=>{e.target.disabled=true;try{if(impact.removed.length&&!l.querySelector('#prAcknowledge').checked)throw Error(text('Подтвердите удаление связей','Acknowledge removing the links'));const res=await call('/api/profiles/'+p.id,{config,expected_version:impact.version,acknowledge_impact:true},'PATCH');close();await onSaved(res);status(p);}catch(e){msg(l.querySelector('#prResult'),errorText(e.message),'err');}finally{e.target.disabled=false;}};}});
    }catch(e){toast(errorText(e.message),'err');}
  }
  async function trial(p,config,onSaved){try{const current=await API.call('/api/profiles/'+p.id+'/trial');const nodes=await API.call('/api/nodes');const eligible=nodes.filter(n=>!n.profile_id&&n.status==='online');openModal({title:text('Проверка на тестовой ноде','Test-node rehearsal'),sub:esc(p.name),icon:'server',size:'lg',className:'profile-workshop',body:`<div data-no-i18n><p>${text('Используется свободная нода без профиля. Создаётся отдельный тестовый профиль; рабочие хосты и сквады не меняются. Для проверки VPN добавьте к нему тестовый хост и тестового клиента.','Uses a free node without a profile. Creates a separate test profile; production hosts and squads remain unchanged. Add a test host and test client to verify VPN connectivity.')}</p>${current?nodeRows(current.nodes)+`<p><a href="#/config?id=${current.candidate_id}" id="ptCandidate">${text('Открыть тестовый профиль','Open test profile')}</a></p>`:`<label class="field"><span>${text('Тестовая нода','Test node')}</span><select class="inp" id="ptNode"><option value="">${text('Выберите ноду','Choose a node')}</option>${eligible.map(n=>`<option value="${n.id}">${esc(n.name)} · Xray ${esc(n.engine_version||'—')}</option>`).join('')}</select></label>${!eligible.length?`<p class="hint">${text('Свободных нод нет. Подключите отдельную ноду без профиля в разделе «Ноды».','No free nodes. Connect a separate node without a profile in Nodes.')}</p>`:''}`}<div class="pw-result" id="ptResult" role="status"></div></div>`,footer:current?`<button class="btn" id="ptFinish">${text('Завершить тест','Finish trial')}</button><button class="btn" id="ptRefresh">${text('Обновить','Refresh')}</button><div class="spacer"></div><button class="btn primary" id="ptPublish">${text('Проверить и применить к рабочему профилю','Review and apply to production profile')}</button>`:`<button class="btn" data-close>${text('Отмена','Cancel')}</button><div class="spacer"></div><button class="btn primary" id="ptStart" ${eligible.length?'':'disabled'}>${text('Запустить тест','Start rehearsal')}</button>`,onMount(l,close){if(current){l.querySelector('#ptCandidate').onclick=()=>closeAllLayers();l.querySelector('#ptRefresh').onclick=()=>{close();trial(p,config,onSaved);};l.querySelector('#ptFinish').onclick=async()=>{try{await API.call('/api/profiles/'+p.id+'/trial',{method:'DELETE'});close();await refreshDB();toast(text('Тест завершён; нода освобождена, тестовый профиль сохранён','Trial finished; node released, test profile retained'));}catch(e){msg(l.querySelector('#ptResult'),errorText(e.message),'err');}};l.querySelector('#ptPublish').onclick=()=>{const ready=current.nodes.some(n=>n.id===current.node_id&&n.applied&&n.reported_version===current.current_version);if(!ready){msg(l.querySelector('#ptResult'),text('Дождитесь подтверждения работающей конфигурации от агента.','Wait for the agent to confirm the configuration is running.'),'err');return;}if(current.base_version!==Number(p.version)){msg(l.querySelector('#ptResult'),text('Рабочий профиль изменён после начала теста. Сравните правки заново.','Production profile changed after the trial started. Review the changes again.'),'err');return;}close();review(p,current.config,onSaved);};}else l.querySelector('#ptStart').onclick=async e=>{e.target.disabled=true;try{await call('/api/profiles/'+p.id+'/trial',{node_id:Number(l.querySelector('#ptNode').value),config,expected_version:Number(p.version)});close();await refreshDB();trial(p,config,onSaved);}catch(e){msg(l.querySelector('#ptResult'),errorText(e.message),'err');}finally{e.target.disabled=false;}};}});}catch(e){toast(errorText(e.message),'err');}}
  async function history(p,onLoad){try{const rows=await API.call('/api/profiles/'+p.id+'/history');openModal({title:text('История конфигурации','Configuration history'),icon:'history',size:'lg',body:`<p>${text('Загрузка версии меняет только черновик редактора. Проверьте отличия и связи перед применением.','Loading a revision only changes the editor draft. Review differences and bindings before applying.')}</p>${rows.map((r,i)=>`<div class="info-row"><span>v${r.version} · ${esc(fmtDT(r.created_at))}</span><button class="btn" data-revision="${i}">${text('Загрузить в редактор','Load in editor')}</button></div>`).join('')||text('Истории пока нет','No history yet')}`,footer:`<div class="spacer"></div><button class="btn" data-close>${text('Закрыть','Close')}</button>`,onMount(l,c){l.querySelectorAll('[data-revision]').forEach(b=>b.onclick=()=>{onLoad(rows[+b.dataset.revision].config);c();});}});}catch(e){toast(errorText(e.message),'err');}}
  return {errorText,wizard,library,saveTemplate,review,status,trial,history,download,parameters,websiteAddress,displayValue,applyFields,fieldIssues,missing,leaves,get,set,builtins};
})();
newProfileModal=()=>ProfileWorkshop.wizard();
