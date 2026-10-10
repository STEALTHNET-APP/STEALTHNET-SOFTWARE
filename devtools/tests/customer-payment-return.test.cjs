'use strict';
const {test} = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const root = path.resolve(__dirname, '../..');
const app = fs.readFileSync(path.join(root, 'web/app/app.js'), 'utf8');

// Minimal DOM boundary. Routing, login, storefront and payment rendering
// execute their production functions; all API responses are synthetic.
function element() {
  let markup = '';
  const children = new Map();
  return {
    hidden:false, open:false, value:'', disabled:false,
    set innerHTML(html) {
      markup = html;
      children.clear();
      for (const match of html.matchAll(/\bid="([^"]+)"/g)) children.set('#' + match[1], element());
    },
    get innerHTML() { return markup; },
    querySelector(selector) {
      if (selector === 'button') {
        if (!children.has(selector)) children.set(selector, element());
      }
      return children.get(selector) || null;
    },
    querySelectorAll() { return []; },
    insertAdjacentHTML(_position, html) { markup += html; },
    addEventListener(event, handler) { this['on' + event] = handler; },
    setAttribute() {},
    showModal() { this.open = true; },
    close() { this.open = false; },
  };
}

function screen(hash, authorized = false) {
  const view = element(), dialog = element(), requests = [];
  const desktopNav = element(), header = element();
  const config = {
    brand:'Test service', headline:'Test storefront', description:'Choose your access',
    tariffs_heading:'Plans', faq_heading:'Help', registration_enabled:true,
    language_content_ready:true, shop_enabled:true, steps:[], faq:[], docs_links:[],
    bot:'', support_url:'', logo:'', logo_dark:'', referral_enabled:false,
  };
  const account = {username:'fixture', tariff:'Test plan', status:'active', code_saved:true};
  const api = async (url, options = {}) => {
    const method = options.method || 'GET';
    requests.push({method, url});
    if (method === 'GET' && url === '/config') return config;
    if (method === 'GET' && url === '/auth/session') {
      if (!authorized) throw Object.assign(new Error('No session'), {status:401});
      return {csrf:'fixture-csrf', code_saved:true};
    }
    if (method === 'GET' && url === '/catalog') return {currency:'RUB', tariffs:[]};
    if (method === 'POST' && url === '/auth/login') {
      assert.equal(options.body.code, 'fixture-access-code');
      authorized = true;
      return {csrf:'fixture-csrf', code_saved:true};
    }
    assert(authorized, 'Private customer data requested before login');
    if (method === 'GET' && url === '/me') return account;
    if (method === 'GET' && url === '/payments?offset=0') return {
      items:[{id:101, title:'Existing canceled payment', status:'canceled',
        amount_minor:17500, currency:'RUB', created_at:'2026-10-09T10:00:00Z', url:null}],
      has_more:false,
    };
    throw new Error(`Unexpected request: ${method} ${url}`);
  };
  const context = vm.createContext({
    URL, URLSearchParams, location:{hash, search:''}, localStorage:{getItem:()=>null},
    window:{scrollTo(){}}, navigator:{language:'ru'},
    document:{body:{className:''}, documentElement:{}, addEventListener(){},
      querySelector:selector => selector === '#desktopNav' ? desktopNav : selector === '.app-header' ? header : null},
    root:view, dialog, tabs:element(), me:null, config:{}, shop:null, currency:'',
    section:'home', generation:0, tg:null, ICON:{},
    api, post:(url, body = {}) => api(url, {method:'POST', body}),
    esc:value => String(value ?? ''),
    money:value => `${value / 100} RUB`, date:value => value,
    statusLabel:{canceled:'Canceled'},
    errorBox:error => { throw error; },
  });
  for (const file of ['customer-en.js', 'customer-i18n.js', 'customer-ui.js']) {
    vm.runInContext(fs.readFileSync(path.join(root, 'web/app', file), 'utf8'), context);
  }
  const safeURL = app.match(/const safeURL = \(s\) => \{[^]*?\n\};/);
  assert(safeURL, 'Production URL validator is available');
  vm.runInContext(safeURL[0], context);
  for (const name of ['showDialog', 'navigate', 'draw', 'drawPayments']) {
    const declaration = app.match(new RegExp(`(?:async )?function ${name}\\([^]*?\\n}`));
    assert(declaration, `Production function ${name} is available`);
    vm.runInContext(declaration[0], context);
  }
  // Theme/header/navigation decoration is unrelated to authentication or routing.
  vm.runInContext('applyCustomerBrand=()=>{}; mountCustomerHeader=()=>{}; drawNav=()=>{};', context);
  context.syncBack = () => {};
  return {
    view, dialog, requests, context,
    async boot() {
      await vm.runInContext('cabinetBoot()', context);
      await new Promise(resolve => setImmediate(resolve));
    },
    async login() {
      const form = dialog.querySelector('#codeLogin');
      assert(form, 'Return without a session must offer code login');
      dialog.querySelector('#loginCode').value = 'fixture-access-code';
      await form.onsubmit({preventDefault(){}, target:form});
      await new Promise(resolve => setImmediate(resolve));
    },
  };
}

test('ordinary unauthenticated visit keeps the storefront without forcing login', async () => {
  const s = screen('');
  await s.boot();
  assert(s.view.querySelector('#tariffs'));
  assert.equal(s.dialog.open, false);
  assert.equal(s.context.location.hash, '');
  assert.equal(s.context.document.body.className, 'public-site');
  assert.deepEqual(s.requests.filter(r => r.method !== 'GET'), []);
});

test('explicit login link still opens the code form', async () => {
  const s = screen('#login');
  await s.boot();
  assert.equal(s.dialog.open, true);
  assert(s.dialog.querySelector('#codeLogin'));
  assert.equal(s.context.location.hash, '#login');
  assert(!s.requests.some(r => r.url === '/me'));
});

test('payment return with a session renders existing payment history without creating an invoice', async () => {
  const s = screen('#payments', true);
  await s.boot();
  assert.equal(s.dialog.open, false);
  assert.equal(s.context.section, 'payments');
  assert.match(s.view.querySelector('#history').innerHTML, /Existing canceled payment/);
  assert.equal(s.context.location.hash, '#payments');
  assert.deepEqual(s.requests.filter(r => r.method !== 'GET'), []);
});

test('payment return without a session preserves its destination through code login', async () => {
  const s = screen('#payments');
  await s.boot();
  assert.equal(s.dialog.open, true, 'Payment return must open login instead of leaving the storefront');
  assert(s.dialog.querySelector('#codeLogin'));
  assert.equal(s.context.location.hash, '#payments');
  assert(!s.requests.some(r => r.url === '/me' || r.url.startsWith('/payments')));
  await s.login();
  assert.equal(s.dialog.open, false);
  assert.equal(s.context.section, 'payments');
  assert.equal(s.context.location.hash, '#payments');
  assert.match(s.view.querySelector('#history').innerHTML, /Existing canceled payment/);
  assert.deepEqual(s.requests.filter(r => r.method !== 'GET'), [{method:'POST', url:'/auth/login'}]);
});
