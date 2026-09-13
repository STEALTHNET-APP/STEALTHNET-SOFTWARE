'use strict';
const {test}=require('node:test'),assert=require('node:assert/strict'),fs=require('node:fs'),vm=require('node:vm'),path=require('node:path');
const root=path.resolve(__dirname,'../..');
const context=vm.createContext({LANG:'ru',URL,translatedText:s=>s,newProfileModal:null});
for(const file of ['presets.js','selfsteal-ui.js','profile-workshop.js'])vm.runInContext(fs.readFileSync(path.join(root,'web',file),'utf8'),context);
const w=vm.runInContext('ProfileWorkshop',context),plain=v=>JSON.parse(JSON.stringify(v));
const preset=()=>plain(w.builtins().find(p=>p.id==='reality-tcp').config);
const fields=(cfg,entries)=>{const params=w.parameters(cfg);return plain(w.applyFields(cfg,params,entries.map(([suffix,value])=>{const index=params.findIndex(p=>p.path.endsWith(suffix));assert(index>=0,suffix);return {index,value};})));};
const reality=cfg=>cfg.inbounds.find(i=>i.protocol==='vless').streamSettings.realitySettings;
test('a domain or HTTPS origin generates a destination and SNI; no default website is inserted',()=>{
  const cfg=preset();assert.equal(reality(cfg).target,'{{target}}');
  for(const value of ['example.com','https://EXAMPLE.com/','https://example.com:443']){
    const r=reality(fields(cfg,[['/target',value]]));assert.equal(r.target,'example.com:443');assert.deepEqual(r.serverNames,['example.com']);
  }
  assert.equal(reality(fields(cfg,[['/target','example.com:8443']])).target,'example.com:8443');
});
test('rejects malformed domains, URL paths, credentials, non-HTTPS schemes and invalid ports',()=>{
  for(const value of ['','http://example.com','example.com/path','https://user:pass@example.com','example.com?x=1','https://example.com#x','localhost','127.0.0.1','example.com:0','example.com:65536','example.com:wat','exa mple.com','-example.com'])assert.throws(()=>w.websiteAddress(value),undefined,value);
});
test('changing the website updates a linked SNI but preserves custom names and keys',()=>{
  const cfg=fields(preset(),[['/target','old.example.com']]);reality(cfg).privateKey='existing-private-key';reality(cfg).shortIds=['0123456789abcdef'];
  const changed=fields(cfg,[['/target','new.example.com']]);assert.deepEqual(reality(changed).serverNames,['new.example.com']);assert.equal(reality(changed).privateKey,'existing-private-key');assert.deepEqual(reality(changed).shortIds,['0123456789abcdef']);
  reality(cfg).serverNames=['one.example.com','two.example.com'];assert.deepEqual(reality(fields(cfg,[['/target','new.example.com']])).serverNames,reality(cfg).serverNames);
  assert.deepEqual(reality(fields(cfg,[['/target','new.example.com'],['/serverNames','custom.example.com']])).serverNames,['custom.example.com']);
});
test('an imported dest alias is retained and its missing SNI is completed',()=>{
  const cfg=preset(),r=reality(cfg);delete r.target;r.dest='origin.example.com:8443';delete r.serverNames;
  const filled=fields(cfg,[]);assert.equal(reality(filled).dest,'origin.example.com:8443');assert(!Object.hasOwn(reality(filled),'target'));assert.deepEqual(reality(filled).serverNames,['origin.example.com']);
});
test('custom raw destinations, empty short IDs, service listener and unknown JSON survive untouched round trips',()=>{
  const cfg=preset(),r=reality(cfg);r.target='/run/reality.sock';r.serverNames=['custom.example.com'];r.shortIds=[''];r.privateKey='existing';cfg.extension={future:['keep',{flag:true}]};
  const params=w.parameters(cfg),values=params.map((p,index)=>({index,value:w.displayValue(cfg,p)}));
  assert.deepEqual(plain(w.applyFields(cfg,params,values)),cfg);
  assert(!params.some(p=>p.path.startsWith('/inbounds/0/')));
});
test('advanced list controls accept plain lines, commas and previous JSON notation',()=>{
  const cfg=fields(preset(),[['/target','example.com']]);
  for(const value of ['a.example.com\nb.example.com','a.example.com, b.example.com','["a.example.com","b.example.com"]'])assert.deepEqual(reality(fields(cfg,[['/serverNames',value]])).serverNames,['a.example.com','b.example.com']);
  assert.deepEqual(reality(fields(cfg,[['/shortIds','[""]']])).shortIds,['']);
  assert.throws(()=>fields(cfg,[['/serverNames','[1]']]));
});
test('missing REALITY parameters collapse to the one website prompt; key generation is automatic',()=>{
  const cfg=preset(),issues=plain(w.fieldIssues(cfg,w.parameters(cfg)));assert.equal(issues.length,1);assert.equal(issues[0].label,'Сайт для маскировки');
  const ready=fields(cfg,[['/target','example.com']]);assert.equal(w.fieldIssues(ready,w.parameters(ready)).length,0);
  const cleared=fields(ready,[['/target','']]);assert(w.fieldIssues(cleared,w.parameters(cleared)).some(p=>p.kind==='website'));
});
test('ports reject empty, fractional, zero and oversized values without changing the draft',()=>{
  const cfg=preset(),before=JSON.stringify(cfg);for(const value of ['','0','65536','3.5','abc'])assert.throws(()=>fields(cfg,[['/port',value]]));assert.equal(JSON.stringify(cfg),before);
  assert.equal(fields(cfg,[['/port','8443']]).inbounds[1].port,8443);
});
test('multiple REALITY inbounds are independent and generated SNI never crosses listeners',()=>{
  const cfg=preset();cfg.inbounds.push({...structuredClone(cfg.inbounds[1]),tag:'second',port:8443});const params=w.parameters(cfg),sites=params.map((p,index)=>({p,index})).filter(({p})=>p.kind==='website');
  const result=plain(w.applyFields(cfg,params,sites.map(({index},i)=>({index,value:`site${i}.example.com`}))));assert.deepEqual(result.inbounds[1].streamSettings.realitySettings.serverNames,['site0.example.com']);assert.deepEqual(result.inbounds[2].streamSettings.realitySettings.serverNames,['site1.example.com']);
});
test('all built-in scenarios expose their required inputs; upstream credentials are never generated',()=>{
  for(const template of w.builtins()){
    const cfg=plain(template.config),params=w.parameters(cfg);for(const missing of w.leaves(cfg))assert(params.some(p=>missing===p.path||missing.startsWith(p.path+'/')),template.id+': '+missing);
  }
  const cfg=plain(w.builtins().find(p=>p.id==='chain').config),params=w.parameters(cfg);assert(params.filter(p=>p.path.startsWith('/outbounds/')).length>=5);assert(params.filter(p=>p.path.startsWith('/outbounds/')).every(p=>!p.generated));
});
test('English labels and explanations do not contain untranslated Russian or raw JSON paths',()=>{
  context.LANG='en';for(const template of w.builtins())for(const p of w.parameters(template.config)){assert(!/[А-Яа-яЁё]/.test(p.label+' '+p.hint));assert(!p.label.startsWith('/'));}context.LANG='ru';
});
test('prototype paths are rejected',()=>{assert.throws(()=>w.set({},'/__proto__/polluted',true));assert.equal({}.polluted,undefined);});
test('port hints distinguish UDP, internal reverse-proxy ports and Shadowsocks',()=>{
  for(const [id,word] of [['hysteria2','UDP'],['vless-mkcp','UDP'],['vless-ws-tls','Внутренний'],['shadowsocks-2022','TCP и UDP']]){
    const p=w.parameters(w.builtins().find(p=>p.id===id).config).find(p=>p.path.endsWith('/port'));assert(p.hint.includes(word));
  }
});
test('a missing Shadowsocks settings object can be completed using the form',()=>{
  const cfg={inbounds:[{tag:'ss',port:8388,protocol:'shadowsocks'}],outbounds:[]};const changed=fields(cfg,[['/method','2022-blake3-aes-128-gcm']]);assert.equal(changed.inbounds[0].settings.method,'2022-blake3-aes-128-gcm');
});
test('Selfsteal asks for a domain, keeps title text, and binds SNI and local target together',()=>{
 const cfg=plain(w.builtins().find(p=>p.id==='selfsteal').config);
 const params=w.parameters(cfg);assert(!params.some(p=>/\/(target|serverNames)/.test(p.path)));
 const result=fields(cfg,[['/_selfsteal/domain','https://NL.example.com'],['/_selfsteal/title','Design {{studio}}'],['/_selfsteal/description','Line one\nLine two']]);
 assert.equal(result._selfsteal.domain,'nl.example.com');assert.equal(result._selfsteal.title,'Design {{studio}}');assert.equal(result._selfsteal.description,'Line one\nLine two');
 assert.equal(reality(result).target,'127.0.0.1:9443');assert.deepEqual(reality(result).serverNames,['nl.example.com']);assert.equal(reality(result).xver,0);
 assert.throws(()=>fields(cfg,[['/_selfsteal/domain','example.com:8443']]));
 const p=w.parameters(result);assert.deepEqual(plain(w.applyFields(result,p,p.map((f,index)=>({index,value:w.displayValue(result,f)})))),result);
});
test('a parameterized managed website exposes one domain field instead of raw target or SNI',()=>{
 const cfg=plain(w.builtins().find(p=>p.id==='selfsteal').config);cfg._selfsteal.domain='{{selfstealDomain}}';reality(cfg).serverNames=['{{serverName}}'];
 const params=w.parameters(cfg);assert(!params.some(p=>/\/(target|serverNames)/.test(p.path)));
 const result=fields(cfg,[['/_selfsteal/domain','node.example.com']]);assert.deepEqual(reality(result).serverNames,['node.example.com']);
});
