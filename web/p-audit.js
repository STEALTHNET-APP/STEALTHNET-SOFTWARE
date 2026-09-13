/* ═══════════ PAGE: Журнал действий ═══════════ */
'use strict';

/* Человеческие названия действий.
 *
 * В базе они машинные — «node.update». Показывать их как есть значит
 * заставлять читателя переводить в уме; а когда администраторов
 * несколько, в журнал смотрят как раз в спешке. */
const AUDIT_ACTIONS = {
  'client.create':   ['Создал клиента', 'ok'],
  'client.update':   ['Изменил клиента', ''],
  'client.delete':   ['Удалил клиента', 'err'],
  'client.grant':    ['Выдал доступ', 'ok'],
  'client.revoke':   ['Отозвал подписку', 'err'],
  'client.reset_traffic': ['Обнулил трафик клиента', 'warn'],
  'node.create':     ['Подключил ноду', 'ok'],
  'node.update':     ['Изменил ноду', ''],
  'node.delete':     ['Удалил ноду', 'err'],
  'node.restart_engine': ['Перезапустил движок', 'warn'],
  'node.reset_traffic':  ['Обнулил трафик ноды', 'warn'],
  'nodes.update_agents': ['Обновил агентов', 'warn'],
  'profile.save':    ['Сохранил профиль', ''],
  'profile.create':  ['Создал профиль', 'ok'],
  'profile.delete':  ['Удалил профиль', 'err'],
  'squad.create':    ['Создал сквад', 'ok'],
  'squad.update':    ['Изменил сквад', ''],
  'tariff.create':   ['Создал тариф', 'ok'],
  'tariff.update':   ['Изменил тариф', ''],
  'payment.refund':  ['Оформил возврат', 'warn'],
  'payment_provider.update': ['Настроил платёжку', ''],
  'token.create':    ['Выпустил токен', 'warn'],
  'token.revoke':    ['Отозвал токен', 'warn'],
};

const auditLabel = (a) => (AUDIT_ACTIONS[a] || [a, ''])[0];
const auditTone  = (a) => (AUDIT_ACTIONS[a] || [a, ''])[1];

registerPage({
  id: 'audit', title: 'Журнал действий', group: 'Система', icon: 'clock',
  render() {
    return `
    <div class="page-head">
      <div><h1>Журнал действий</h1>
        <div class="desc">Кто и что менял в панели. Пишется сам, правке не подлежит —
          на то он и журнал.</div></div>
      <div class="actions">
        <select class="inp" id="auFilter" style="width:auto">
          <option value="">Все действия</option>
          <option value="client">Клиенты</option>
          <option value="node">Ноды</option>
          <option value="nodes">Парк нод</option>
          <option value="profile">Профили</option>
          <option value="tariff">Тарифы</option>
          <option value="squad">Сквады</option>
          <option value="payment">Платежи</option>
          <option value="token">Токены</option>
        </select>
        <button class="btn" id="auReload">${I('refresh',14)} Обновить</button>
      </div>
    </div>
    <div class="card" style="padding:0"><div id="auBody" class="tbl-wrap" tabindex="0" aria-label="Журнал действий"></div></div>`;
  },

  async bind(root) {
    const box = root.querySelector('#auBody');
    const filter = root.querySelector('#auFilter');

    const load = async () => {
      box.innerHTML = `<div class="empty" style="padding:40px">${I('clock',28)}<b>Читаем журнал…</b></div>`;
      let items;
      try {
        const q = filter.value ? `?action=${encodeURIComponent(filter.value)}` : '';
        ({ items } = await API.call('/api/audit' + q));
      } catch (e) {
        box.innerHTML = `<div class="empty" style="padding:40px">${I('x',28)}
          <b>Не прочитался</b><span>${esc(e.message)}</span></div>`;
        return;
      }

      if (!items.length) {
        box.innerHTML = `<div class="empty" style="padding:40px">${I('clock',28)}
          <b>Записей нет</b><span>По этому отбору в журнале пусто.</span></div>`;
        return;
      }

      box.innerHTML = `
        <table class="tbl readonly">
          <thead><tr>
            <th>Когда</th><th>Кто</th><th>Что сделал</th><th>Над чем</th><th>Откуда</th>
          </tr></thead>
          <tbody>${items.map(row).join('')}</tbody>
        </table>`;
    };

    const row = (r) => {
      // Название объекта показываем, если оно нашлось. Удалённый объект
      // имени уже не имеет — тогда честно остаётся только номер.
      const what = r.entity_name
        ? `<b>${esc(r.entity_name)}</b>`
        : r.entity_id ? `<span class="sub-note">#${r.entity_id}</span>` : '—';
      const tone = auditTone(r.action);
      // Деятель бывает не только администратором: бот и служебный токен
      // тоже пишут в журнал, и путать их с людьми не стоит.
      const who = r.actor
        ? esc(r.actor)
        : `<span class="sub-note">${esc(r.actor_kind || 'система')}</span>`;
      return `
        <tr>
          <td class="num sub-note">${fmtDT(r.created_at)}</td>
          <td>${who}</td>
          <td><span class="bdg ${tone}">${esc(auditLabel(r.action))}</span></td>
          <td>${what}</td>
          <td class="mono sub-note">${esc(r.ip || '—')}</td>
        </tr>`;
    };

    filter.addEventListener('change', load);
    root.querySelector('#auReload').addEventListener('click', load);
    await load();
  },
});
