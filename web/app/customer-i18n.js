/* Only authored UI strings are translated. Template values (codes, customer data,
   brand content and provider instructions) are never rewritten. */
'use strict';
const CT = (() => {
  const key='sn.customer.language';
  const query=new URLSearchParams(location.search).get('lang');
  let saved;try{saved=localStorage.getItem(key);}catch{}
  const telegram=window.Telegram?.WebApp?.initDataUnsafe?.user?.language_code;
  const locale=['ru','en'].includes(query)?query:['ru','en'].includes(saved)?saved:(telegram||navigator.language||'ru').startsWith('ru')?'ru':'en';
  document.documentElement.lang=locale;
  const cyr=/[А-Яа-яЁё]/;
  function text(value){
    const raw=String(value),phrase=raw.trim();
    if(locale==='ru'||!cyr.test(phrase))return raw;
    if(!Object.hasOwn(CT_EN,phrase))throw new Error('Missing UI translation');
    return raw.replace(phrase,CT_EN[phrase]);
  }
  function chunk(raw){
    if(locale==='ru')return raw;
    const labels=[];
    raw=raw.replace(/(\b(?:aria-label|title|placeholder|alt)\s*=\s*["'])([^"']+)(["'])/g,(_,a,t,b)=>{
      const id=labels.length;labels.push(a+text(t)+b);return `__CT_LABEL_${id}__`;
    });
    raw=raw.split(/(<[^>]*>|>)/g).map(s=>s.startsWith('<')||s==='>'?s:text(s)).join('');
    return raw.replace(/__CT_LABEL_(\d+)__/g,(_,id)=>labels[id]);
  }
  function html(strings,...values){
    if(typeof strings==='string')return chunk(strings);
    return strings.map((s,i)=>chunk(s)+(i<values.length?String(values[i]):'')).join('');
  }
  function error(message){
    const s=String(message||'');
    if(locale==='ru'||!cyr.test(s))return s;
    return CT_EN[s]||s;
  }
  function setLanguage(lang){
    if(!['ru','en'].includes(lang))return;
    try{localStorage.setItem(key,lang);}catch{}
    const url=new URL(location.href);url.searchParams.set('lang',lang);location.assign(url.href);
  }
  function localizeConfig(config){
    if(locale==='ru')return config;
    const copy={...config},translated=config.locales?.en||{};
    for(const field of ['headline','description','tariffs_heading','faq_heading','seo_title','seo_description','support_label','support_text','docs_text'])copy[field]=translated[field]||'';
    for(const field of ['steps','faq','docs_links'])copy[field]=translated[field]||[];
    copy.language_content_ready=['headline','description','tariffs_heading','seo_title','seo_description'].every(k=>typeof translated[k]==='string'&&translated[k].trim());
    return copy;
  }
  function response(path,value){
    if(path==='/config')return localizeConfig(value);
    if(locale!=='en')return value;
    const plan=p=>({...p,title:p.locales?.en?.title||p.title,description:p.locales?.en?.description||'',badge:p.locales?.en?.badge||''});
    const addon=p=>({...p,title:`+${p.quantity} ${p.kind==='traffic'?'GB traffic':p.quantity===1?'device':'devices'}`});
    const method=m=>({...m,title:({manual:'Bank transfer',cryptobot:'Cryptocurrency',custom:'Online payment'})[m.id]||m.title});
    if(value.tariffs)value.tariffs=value.tariffs.map(plan);
    if(path==='/me'&&value.tariff_locales?.en?.title)value.tariff=value.tariff_locales.en.title;
    if(path==='/addons')value.items=value.items.map(addon);
    if(path.startsWith('/payments'))for(const p of value.items||[])p.title=p.addon_kind?addon({kind:p.addon_kind,quantity:p.addon_quantity}).title:p.locales?.en?.title||p.title;
    if(value.methods)value.methods=value.methods.map(method);
    if(value.methods_by_currency)for(const currency of Object.keys(value.methods_by_currency))value.methods_by_currency[currency]=value.methods_by_currency[currency].map(method);
    return value;
  }
  function mount(container){
    if(!container||container.querySelector('.language-trigger'))return;
    const b=document.createElement('button');b.type='button';b.className='icon-btn language-trigger';b.textContent=locale.toUpperCase();b.setAttribute('aria-label',text('Язык'));b.title=locale==='ru'?'Switch to English':'Переключить на русский';b.onclick=()=>setLanguage(locale==='ru'?'en':'ru');container.append(b);
  }
  document.addEventListener('DOMContentLoaded',()=>{
    for(const el of document.querySelectorAll('[data-ct]'))el.textContent=text(el.dataset.ct);
    for(const el of document.querySelectorAll('[data-ct-label]'))el.setAttribute('aria-label',text(el.dataset.ctLabel));
    mount(document.querySelector('.header-actions')||document.querySelector('.app-header'));
  });
  return Object.freeze({locale,text,html,error,setLanguage,localizeConfig,response,mount});
})();
