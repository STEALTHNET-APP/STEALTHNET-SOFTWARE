const {test}=require('node:test'),assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path'),vm=require('node:vm');
const dir=process.env.SN_SOURCE_ROOT||path.resolve(__dirname,'../..');
const app=fs.readFileSync(path.join(dir,'web/app/app.js'),'utf8');
function harness(lang){
 const nodes={}; const element=()=>({value:'',textContent:'',disabled:false,addEventListener(k,f){this['on'+k]=f},querySelectorAll(){return []},insertAdjacentHTML(_,h){this.innerHTML=(this.innerHTML||'')+h}});
 const root={markup:'',set innerHTML(h){this.markup=h;for(const k of Object.keys(nodes))delete nodes[k];for(const [,id] of h.matchAll(/id="([^"]+)"/g))nodes['#'+id]=element();if(nodes['#message'])nodes['#message'].value=(h.match(/<textarea[^>]*>([^]*?)<\/textarea>/)||[])[1]||''},get innerHTML(){return this.markup},querySelector(s){return nodes[s]||null},querySelectorAll(){return []},insertAdjacentHTML(_,h){this.innerHTML=h+this.markup}};
 const c=vm.createContext({URL,URLSearchParams,location:{search:'?lang='+lang},window:{},navigator:{language:lang},document:{documentElement:{},addEventListener(){}},localStorage:{getItem:()=>null},root,generation:1,currentTicket:null,drafts:new Map(),config:{tickets_enabled:true,docs_links:[]},statusLabel:{active:'Active',expired:'Expired'},ticketStatus:{},esc:v=>String(v??'').replaceAll('&','&amp;').replaceAll('<','&lt;').replaceAll('"','&quot;'),safeURL:v=>{try {const u=new URL(v);return ['https:','http:'].includes(u.protocol)?u.href:null}catch{return null}},richText:()=>'',date:String,money:String,syncBack(){},navigate(){c.generation++;c.currentTicket=null;root.innerHTML='new screen'},notice(){c.notices++},notices:0,errorBox(e){root.innerHTML='<div>'+e.message+'</div>'},api:async()=>({items:[]}),post:async()=>({id:2}),openURL(){},openInvoice(){},checkPayment(){},uiIcon:()=>'',bytes:String,canChoosePlan:()=>true,drawDevices(){},showAccess(){},mountCustomerHeader(){}});
 for(const f of ['customer-en.js','customer-i18n.js'])vm.runInContext(fs.readFileSync(path.join(dir,'web/app',f),'utf8'),c);
 vm.runInContext(app.slice(app.indexOf('async function drawPayments('),app.indexOf('\ntg?.ready')),c);
 return {c,root,nodes};
}
function gate(){let resolve,reject;const promise=new Promise((a,b)=>{resolve=a;reject=b});return {promise,resolve,reject}}
for(const lang of ['ru','en']){
 test(`${lang} B16 document titles are escaped, unsafe URLs omitted and empty list works`,async()=>{
  const h=harness(lang);h.c.config.docs_links=[{title:'Правила <script> & '+ 'Long title '.repeat(50),url:'https://example.test/rules'},{title:'Privacy policy',url:'https://example.test/privacy'},{title:'Unsafe',url:'javascript:alert(1)'}];
  await h.c.drawHelp(1);assert.match(h.root.innerHTML,/Правила &lt;script> &amp;/);assert.match(h.root.innerHTML,/Privacy policy/);assert.doesNotMatch(h.root.innerHTML,/javascript:|>Unsafe</);assert.equal((h.root.innerHTML.match(/class="document-link"/g)||[]).length,2);
  h.c.config.docs_links=[];await h.c.drawHelp(1);assert.doesNotMatch(h.root.innerHTML,/class="document-link"/);
 });
 test(`${lang} B18 invoice and check actions only on pending invoices`,async()=>{
  for(const status of ['pending','success','canceled','failed','refunded'])for(const url of ['https://example.test/pay',null,'javascript:alert(1)']){
   const h=harness(lang);h.c.api=async()=>({items:[{id:1,status,url}]});await h.c.drawPayments(1);const html=h.nodes['#history'].innerHTML;
   assert.equal(html.includes('data-invoice='),status==='pending'&&!!h.c.safeURL(url),status+' '+url);assert.equal(html.includes('data-check='),status==='pending',status);
  }
 });
 for(const outcome of ['resolve','reject'])for(const replaced of [false,true])test(`${lang} B17 stale ${outcome}, replaced=${replaced}, preserves draft and new UI`,async()=>{
  const h=harness(lang),g=gate();h.c.currentTicket={new:true};h.c.post=()=>g.promise;await h.c.drawTicket(1);
  h.nodes['#message'].value='old draft';h.nodes['#message'].oninput();const send=h.nodes['#sendMessage'];const pending=send.onclick({target:send});
  if(!replaced)h.c.generation++;h.c.currentTicket={new:true};await h.c.drawTicket(h.c.generation);h.nodes['#message'].value='replacement draft';h.nodes['#message'].oninput();
  g[outcome](outcome==='resolve'?{id:42}:new Error('late failure'));await pending;
  assert.equal(h.c.drafts.get('new'),'replacement draft');assert.equal(h.c.currentTicket.new,true);assert.equal(h.nodes['#ticketError'].textContent,'');assert.equal(h.c.notices,0);
 });
 test(`${lang} B17 current failure retains draft and load failure offers back`,async()=>{
  const h=harness(lang);h.c.currentTicket={new:true};h.c.post=async()=>{throw Error('failed')};await h.c.drawTicket(1);h.nodes['#message'].value='keep';h.nodes['#message'].oninput();await h.nodes['#sendMessage'].onclick({target:h.nodes['#sendMessage']});assert.equal(h.c.drafts.get('new'),'keep');assert.equal(h.nodes['#ticketError'].textContent,'failed');assert.equal(h.nodes['#sendMessage'].disabled,false);
  h.c.currentTicket={id:4};h.c.api=async()=>{throw Error('load failure')};await h.c.drawTicket(1);assert.ok(h.nodes['#ticketBack']);h.nodes['#ticketBack'].onclick();assert.equal(h.c.currentTicket,null);
 });
 test(`${lang} B23 active and expired badges include year; null and invalid dates safe`,()=>{
  const h=harness(lang),ui=fs.readFileSync(path.join(dir,'web/app/customer-ui.js'),'utf8');vm.runInContext(ui.slice(ui.indexOf('function drawHome()'),ui.indexOf('async function mountGuidedAddons')),h.c);
  for(const status of ['active','expired'])for(const expiry of ['2026-12-31T12:00:00Z','2027-01-01T12:00:00Z',null,'invalid']){
   h.c.me={status,tariff:'Plan',expires_at:expiry};h.c.drawHome();const badge=h.root.innerHTML.split('<section class="connect-card">')[0];
   if(expiry&&expiry!=='invalid')assert.match(badge,new RegExp(expiry.slice(0,4)));assert.doesNotMatch(h.root.innerHTML,/Invalid Date|NaN|1970/);
  }
 });
}

for(const outcome of ['resolve','reject'])test(`B17 ${outcome} after leaving screen safely ignores missing error node`,async()=>{
 const h=harness('en'),g=gate();h.c.currentTicket={new:true};h.c.post=()=>g.promise;await h.c.drawTicket(1);h.nodes['#message'].value='draft';h.nodes['#message'].oninput();const p=h.nodes['#sendMessage'].onclick({target:h.nodes['#sendMessage']});h.c.navigate();g[outcome](outcome==='resolve'?{id:3}:Error('late'));await p;assert.equal(h.root.innerHTML,'new screen');assert.equal(h.c.drafts.get('new'),'draft');
});
for(const lang of ['ru','en'])for(const existing of [false,true])test(`${lang} B17 edited draft follows confirmed ticket and next send replies to same ticket; existing=${existing}`,async()=>{
 const h=harness(lang),g=gate(),calls=[];h.c.currentTicket=existing?{id:3,messages:[]}:{new:true};
 h.c.api=async()=>({id:3,subject:'Confirmed ticket',messages:[{id:10,who:'customer',body:'sent',at:''}]});
 h.c.post=(url,body)=>{calls.push({url,body});return g.promise};await h.c.drawTicket(1);
 h.nodes['#message'].value='sent';h.nodes['#message'].oninput();const p=h.nodes['#sendMessage'].onclick({target:h.nodes['#sendMessage']});
 h.nodes['#message'].value='new words';h.nodes['#message'].oninput();g.resolve({id:3});await p;
 assert.equal(h.c.currentTicket.id,3);assert.equal(h.c.drafts.get(3),'new words');assert.equal(h.c.drafts.has('new'),false);
 assert.equal(h.nodes['#message'].value,'new words');assert.match(h.root.innerHTML,/<p>sent<\/p>/);assert.equal(h.nodes['#sendMessage'].disabled,false);
 await h.nodes['#sendMessage'].onclick({target:h.nodes['#sendMessage']});
 assert.equal(calls[0].url,existing?'/tickets/3':'/tickets');assert.equal(calls[1].url,'/tickets/3');assert.equal(calls[1].body.body,'new words');
});
