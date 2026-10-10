'use strict';
const {test}=require('node:test'),assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path'),vm=require('node:vm');
const ROOT=process.env.SN_SOURCE_ROOT||path.resolve(__dirname,'../..');
const source=fs.readFileSync(path.join(ROOT,'web/app/app.js'),'utf8');
function deferred(){let resolve,reject;const promise=new Promise((a,b)=>{resolve=a;reject=b});return {promise,resolve,reject};}
// DOM boundary only; actual buySheet, paymentOptions and translation code execute.
function harness(lang,opts={}){
 const nodes={},calls=[],buttons=[];let payGate;
 function el(){return {textContent:'',disabled:false,isConnected:true,classList:{add(){},remove(){}},addEventListener(n,f){this['on'+n]=f},querySelectorAll(){return []}};}
 for(const id of ['#promo','#applyPromo','#quote','#payActions','#payError','.dialog-body'])nodes[id]=el();
 const periods=[3,30,60].map(n=>Object.assign(el(),{value:String(n)}));
 Object.defineProperty(nodes['#payActions'],'innerHTML',{set(html){this.markup=html;buttons.length=0;for(const m of html.matchAll(/<button([^>]*data-pay="([^"]+)"[^>]*)>([^]*?)<\/button>/g))buttons.push(Object.assign(el(),{dataset:{pay:m[2]},textContent:m[3],disabled:/\bdisabled\b/.test(m[1])}));},get(){return this.markup}});
 nodes['#payActions'].querySelectorAll=()=>buttons;
 const dialog={open:true,querySelector:s=>nodes[s]||null,querySelectorAll:s=>s==='input[name="period"]'?periods:s==='input,select,button'?[...periods,...buttons,nodes['#promo'],nodes['#applyPromo']]:[],close(){this.open=false}};
 const prices=[{days:3,currency:'USD',amount_minor:0},{days:30,currency:'USD',amount_minor:100},{days:60,currency:'USD',amount_minor:300},{days:30,currency:'XTR',amount_minor:20},{days:60,currency:'XTR',amount_minor:60}];
 const shop={tariffs:[{id:1,title:'Synthetic',description:'Test',prices}],methods_by_currency:{USD:[{id:'manual',title:'Manual'}],XTR:opts.stars===false?[]:[{id:'stars',title:'Stars'}]}};
 let quoteHandler=async b=>{
  const amount=prices.find(p=>p.days===b.days&&p.currency===b.currency).amount_minor;
  if(b.promo==='INVALID'||b.promo==='FXERR'&&b.currency==='XTR')throw Error('Invalid promo');
  return {amount_minor:b.promo==='ZERO'?0:b.promo==='HALF'?amount/2:b.promo==='FIXED'||b.promo==='FXERR'?b.currency==='USD'?0:amount:b.promo==='STARZERO'?b.currency==='XTR'?0:amount:amount,days:b.days+(b.promo==='BONUS'?5:0)};
 };
 const c=vm.createContext({URL,URLSearchParams,location:{search:'?lang='+lang},localStorage:{getItem:()=>null},window:{},navigator:{language:lang},document:{documentElement:{},addEventListener(){}},dialog,shop,currency:'USD',tg:null,activePlan:null,me:null,pendingPayment:null,esc:String,duration:String,money:(n,cur)=>`${n} ${cur}`,showDialog(){dialog.open=true},navigate(){},notice(){},safeURL:()=>false,api:async()=>({}),post:async(url,body)=>{calls.push({url,body});if(url==='/quote')return quoteHandler(body);if(url==='/pay'){payGate=deferred();return payGate.promise;}throw Error(url)}});
 for(const file of ['customer-en.js','customer-i18n.js'])vm.runInContext(fs.readFileSync(path.join(ROOT,'web/app',file),'utf8'),c);
 vm.runInContext(source.slice(source.indexOf('function paymentOptions('),source.indexOf('\nconst money =')),c);
 vm.runInContext(source.slice(source.indexOf('function buySheet('),source.indexOf('\nfunction openInvoice(')),c);
 vm.runInContext('buySheet(1,30)',c);
 return {c,nodes,calls,buttons,dialog,free:lang==='ru'?'Активировать бесплатно':'Activate for free',button:id=>buttons.find(b=>b.dataset.pay===id),setCode(code){nodes['#promo'].oninput({target:{value:code}})},apply(){return nodes['#applyPromo'].onclick()},async promo(code){this.setCode(code);await this.apply()},period(n){periods.find(p=>p.value===String(n)).onchange({target:{value:String(n)}})},setQuoteHandler(fn){quoteHandler=fn},resolvePay(){payGate.resolve({free:true})}};
}
for(const lang of ['ru','en']){
 test(`${lang}: zero quote changes action without changing provider`,async()=>{const h=harness(lang);await h.promo('ZERO');assert.equal(h.button('manual').textContent,h.free);assert.equal(h.button('stars').textContent,h.free);assert(!h.button('free'));});
 test(`${lang}: amounts are evaluated separately per currency`,async()=>{const h=harness(lang);await h.promo('FIXED');assert.equal(h.button('manual').textContent,h.free);assert.notEqual(h.button('stars').textContent,h.free);await h.promo('STARZERO');assert.notEqual(h.button('manual').textContent,h.free);assert.equal(h.button('stars').textContent,h.free);});
 test(`${lang}: positive discount and bonus days keep paid action`,async()=>{const h=harness(lang);for(const code of ['HALF','BONUS']){await h.promo(code);assert.notEqual(h.button('manual').textContent,h.free);assert.notEqual(h.button('stars').textContent,h.free);}});
 test(`${lang}: invalid alternative quote stays disabled`,async()=>{const h=harness(lang);await h.promo('FXERR');assert.equal(h.button('manual').textContent,h.free);assert(h.button('stars').disabled);assert.notEqual(h.button('stars').textContent,h.free);});
 test(`${lang}: changing code clears free action and invalid code blocks checkout`,async()=>{const h=harness(lang);await h.promo('ZERO');h.setCode('INVALID');assert.notEqual(h.button('manual').textContent,h.free);await h.apply();await h.button('manual').onclick();assert.equal(h.calls.filter(x=>x.url==='/pay').length,0);});
 test(`${lang}: changing period clears quote; original free tariff keeps free provider`,async()=>{const h=harness(lang);await h.promo('ZERO');h.period(60);assert.notEqual(h.button('manual').textContent,h.free);h.period(3);assert(h.button('free'));assert(!h.button('manual'));});
 test(`${lang}: stale quote after code change cannot restore free label`,async()=>{const h=harness(lang,{stars:false}),gate=deferred();h.setQuoteHandler(()=>gate.promise);h.setCode('ZERO');const waiting=h.apply();h.setCode('HALF');gate.resolve({amount_minor:0,days:30});await waiting;assert.notEqual(h.button('manual').textContent,h.free);});
 test(`${lang}: stale quote after period change cannot restore free label`,async()=>{const h=harness(lang,{stars:false}),gate=deferred();h.setQuoteHandler(()=>gate.promise);h.setCode('ZERO');const waiting=h.apply();h.period(60);gate.resolve({amount_minor:0,days:30});await waiting;assert.notEqual(h.button('manual').textContent,h.free);});
 test(`${lang}: double click dispatches one checkout using original provider`,async()=>{const h=harness(lang);await h.promo('ZERO');const b=h.button('manual'),first=b.onclick();const second=b.onclick();assert.equal(h.calls.filter(x=>x.url==='/pay').length,1);assert.equal(h.calls.find(x=>x.url==='/pay').body.provider,'manual');h.resolvePay();await Promise.all([first,second]);});
 test(`${lang}: Stars activation keeps Stars currency and provider`,async()=>{const h=harness(lang);await h.promo('ZERO');const request=h.button('stars').onclick();const call=h.calls.find(x=>x.url==='/pay');assert.equal(call.body.provider,'stars');assert.equal(call.body.currency,'XTR');h.resolvePay();await request;});
 test(`${lang}: stale secondary Stars quote cannot restore a cleared quote`,async()=>{const h=harness(lang),gate=deferred(),started=deferred();h.setQuoteHandler(b=>{if(b.currency==='XTR'){started.resolve();return gate.promise;}return Promise.resolve({amount_minor:0,days:30});});h.setCode('ZERO');const waiting=h.apply();await started.promise;h.period(60);gate.resolve({amount_minor:0,days:30});await waiting;assert.notEqual(h.button('manual').textContent,h.free);assert.notEqual(h.button('stars').textContent,h.free);});
 test(`${lang}: positive quote preserves exact paid labels and Stars amount`,async()=>{const h=harness(lang);await h.promo('HALF');const fork=source.includes('Оформить счёт ·');assert.equal(h.button('manual').textContent,lang==='ru'?(fork?'Оформить счёт · Manual':'Оплатить · Manual'):(fork?'Create invoice · Manual':'Pay · Manual'));assert.equal(h.button('stars').textContent,lang==='ru'?'Оплатить 10 XTR':'Pay 10 XTR');});

}
