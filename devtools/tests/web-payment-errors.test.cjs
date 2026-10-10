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
for(const lang of ['ru','en']) for(const detached of [false,true]) test(`${lang} B15 masks provider errors; detached=${detached}`,async()=>{
 const h=harness(lang),notices=[];h.c.notice=m=>notices.push(m);h.c.post=async()=>{throw Error('provider API secret=example_123 currency merchant key')};
 if(detached)h.nodes['.dialog-body'].isConnected=false;
 await h.button('manual').onclick();const text=detached?notices.join(' '):h.nodes['#payError'].textContent;
 assert.doesNotMatch(text,/example_123|merchant|API|secret|currency/);assert.match(text,lang==='ru'?/историю платежей/:/payment history/);assert.match(text,lang==='ru'?/поддержку перед новой оплатой/:/support before making another payment/);
});
