/* Графики панели.
 *
 * Своё, на SVG, без внешних библиотек: панель ставят к себе на сервер,
 * и тянуть чужой код с чужого домена ради пары диаграмм — лишняя
 * зависимость и лишний способ узнать, кто её использует.
 *
 * Правило у всех функций одно: рисуем только то, что есть в данных.
 * Пустой ряд возвращает честную врезку «данных нет», а не гладкую
 * линию на нулях — иначе график сообщает о работе системы то, чего не
 * было. Ноль при этом остаётся нулём и рисуется нулём.
 *
 * Цвета берутся из темы (var(--…)), поэтому графики переключаются
 * вместе с ней и не требуют перерисовки.
 */
'use strict';

/* Уникальный номер для градиентов и масок: два графика на одной
   странице с одинаковым id склеили бы свои заливки. */
let _cid = 0;
const nextId = () => 'c' + (++_cid);

const esc2 = (s) => String(s).replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/"/g, '&quot;');

/* Пустая врезка нужного размера — чтобы блок не «схлопывался» и
   страница не прыгала, когда данные появятся. */
function chartEmpty(h, text = 'Данных за период нет') {
  return `<div class="chart-empty" style="height:${h}px">
    <svg viewBox="0 0 24 24" width="22" height="22" fill="none" stroke-width="1.6"><path d="M3 3v18h18"/><path d="m7 14 3-3 3 3 5-6"/></svg>
    <span>${esc2(text)}</span></div>`;
}

const niceMax = (v) => {
  if (!(v > 0)) return 1;
  const p = Math.pow(10, Math.floor(Math.log10(v)));
  return Math.ceil(v / p * 2) / 2 * p;   // до ближайшей «круглой» половины
};

/* ── площадь с градиентом ──
   Основной график панели: видно и уровень, и форму — где провал, где
   всплеск.

   Устройство важное: тянущийся SVG масштабирует всё внутри себя, включая
   текст и толщину линий. Из-за этого подписи осей раздувались в полтора
   раза и выглядели жирными кляксами, а зоны наведения проступали
   чёрными полосами — глобальное правило `svg{stroke:currentColor}`
   пририсовывало им обводку, и растяжение делало её заметной.

   Поэтому: в SVG остаются только заливка и линия (обводка помечена
   non-scaling-stroke, чтобы не толстеть), а сетка, подписи и зоны
   наведения — обычные элементы разметки поверх. Они не растягиваются,
   читаются одинаково при любой ширине и не требуют пересчёта. */
function areaChart(points, opts = {}) {
  const {
    h = 190, color = 'var(--accent)', fmt = (v) => v, labels = null,
  } = opts;

  const vals = points.map((p) => (typeof p === 'object' ? p.v : p) || 0);
  if (!vals.length) return chartEmpty(h);
  // Всё по нулям — это «ничего не происходило», и линия на дне об этом
  // не говорит, а изображает ровную работу. Показываем словами.
  if (!vals.some((v) => v > 0)) return chartEmpty(h, 'За период ничего не начислялось');

  const id = nextId();
  const max = niceMax(Math.max(...vals));
  const W = 1000, H = 100;            // условные единицы: SVG растянется
  const n = vals.length;
  const x = (i) => (n === 1 ? W / 2 : (i * W) / (n - 1));
  const y = (v) => H - (v / max) * H;

  /* Сглаживание. Ломаная из тридцати отрезков выглядит дёргано, а
     обычные кубические кривые «промахиваются» мимо точек и рисуют
     провалы там, где их не было. Берём монотонную интерполяцию: она
     проходит ровно через значения и не выдумывает лишних колебаний. */
  const path = (() => {
    if (n < 3) return vals.map((v, i) => `${i ? 'L' : 'M'}${x(i)},${y(v).toFixed(2)}`).join('');
    let d = `M${x(0)},${y(vals[0]).toFixed(2)}`;
    for (let i = 0; i < n - 1; i++) {
      const x0 = x(i), x1 = x(i + 1);
      const y0 = y(vals[i]), y1 = y(vals[i + 1]);
      const dx = (x1 - x0) / 3;
      d += `C${(x0 + dx).toFixed(2)},${y0.toFixed(2)} ${(x1 - dx).toFixed(2)},${y1.toFixed(2)} ${x1.toFixed(2)},${y1.toFixed(2)}`;
    }
    return d;
  })();

  // Три линии сетки: больше — рябит, меньше — не с чем соотнести высоту.
  const ylabs = [1, 0.5, 0].map((k) => String(fmt(max * k)));
  const grid = [1, 0.5, 0].map((k, i) => `
    <div class="ch-row" style="top:${((1 - k) * 100).toFixed(2)}%">
      <span class="ch-ylab">${esc2(ylabs[i])}</span>
    </div>`).join('');

  /* Ширина полосы под подписи оси.
     Раньше она была фиксированной, и «$300.00» не помещался — подпись
     вылезала за край карточки. Считаем по самой длинной строке: шрифт
     моноширинный, поэтому длина в символах даёт точную ширину в ch. */
  const pad = Math.max(...ylabs.map((t) => t.length)) + 1;

  // Подписи дат: максимум пять, иначе на месяце они сливаются в кашу.
  // Крайние прижимаем к краям: по центру они наполовину уходят за поле.
  const step = labels ? Math.max(1, Math.ceil(labels.length / 5)) : 1;
  const xlab = labels ? labels.map((t, i) => {
    const last = i === labels.length - 1;
    if (i % step && !last) return '';
    const edge = i === 0 ? ' ch-xlab-first' : last ? ' ch-xlab-last' : '';
    return `<span class="ch-xlab${edge}" style="left:${((x(i) / W) * 100).toFixed(2)}%">${esc2(t)}</span>`;
  }).join('') : '';

  // Зоны наведения — разметкой, а не в SVG: попасть мышью в тонкую
  // линию невозможно, а в полосу во всю высоту — всегда.
  const hit = vals.map((v, i) => `
    <i class="ch-hit" style="left:${((x(i) / W) * 100).toFixed(2)}%;width:${(100 / n).toFixed(3)}%"
       data-t="${esc2((labels && labels[i] ? labels[i] + ' · ' : '') + fmt(v))}"
       data-y="${((y(v) / H) * 100).toFixed(2)}"></i>`).join('');

  return `<div class="chart" style="--ch:${color};height:${h}px;--ch-pad:${pad}ch">
    <div class="ch-plot">
      ${grid}
      <svg class="ch-svg" viewBox="0 0 ${W} ${H}" preserveAspectRatio="none" aria-hidden="true">
        <defs><linearGradient id="${id}" x1="0" y1="0" x2="0" y2="1">
          <stop offset="0%"   stop-color="${color}" stop-opacity=".30"/>
          <stop offset="100%" stop-color="${color}" stop-opacity="0"/>
        </linearGradient></defs>
        <path d="${path}L${W},${H}L0,${H}Z" fill="url(#${id})" stroke="none"/>
        <path d="${path}" fill="none" stroke="${color}" stroke-width="2"
              stroke-linejoin="round" stroke-linecap="round" vector-effect="non-scaling-stroke"/>
      </svg>
      <div class="ch-hits">${hit}</div>
      <i class="ch-dot"></i>
    </div>
    <div class="ch-axis">${xlab}</div>
    <div class="ch-tip"></div>
  </div>`;
}

/* ── спарклайн ──
   Крохотный график в карточке показателя: само число говорит «сколько»,
   спарклайн — «куда идёт». Без осей и подписей: на сорока пикселях они
   не читаются и только пачкают карточку. */
function sparkline(values, opts = {}) {
  const { w = 108, h = 30, color = 'var(--accent)' } = opts;
  const vals = (values || []).map((v) => v || 0);
  if (vals.length < 2 || !vals.some((v) => v > 0)) return '';
  const id = nextId();
  const max = Math.max(...vals), min = Math.min(...vals);
  const span = max - min || 1;
  // Отступ по краям: без него крайняя точка обрезается пополам краем
  // карточки, а линия наезжает на рамку.
  const px = 3;
  const x = (i) => px + (i * (w - px * 2)) / (vals.length - 1);
  const y = (v) => h - 4 - ((v - min) / span) * (h - 8);
  const line = vals.map((v, i) => `${i ? 'L' : 'M'}${x(i).toFixed(1)},${y(v).toFixed(1)}`).join('');
  return `<svg class="spark" viewBox="0 0 ${w} ${h}" preserveAspectRatio="none">
    <defs><linearGradient id="${id}" x1="0" y1="0" x2="0" y2="1">
      <stop offset="0%" stop-color="${color}" stop-opacity=".22"/>
      <stop offset="100%" stop-color="${color}" stop-opacity="0"/></linearGradient></defs>
    <path d="${line}L${(w - px).toFixed(1)},${h}L${px},${h}Z" fill="url(#${id})" stroke="none"/>
    <path d="${line}" fill="none" stroke="${color}" stroke-width="1.6" stroke-linejoin="round"/>
    <circle cx="${x(vals.length - 1).toFixed(1)}" cy="${y(vals[vals.length - 1]).toFixed(1)}" r="2.2" fill="${color}" stroke="none"/>
  </svg>`;
}

/* ── кольцо ──
   Для состава целого: сколько клиентов активны, сколько истекли. Доли
   лучше читаются кольцом, чем четырьмя карточками с числами, между
   которыми глазу приходится считать проценты самому. */
function donut(parts, opts = {}) {
  const { size = 132, thickness = 14, center = null } = opts;
  const items = (parts || []).filter((p) => p.value > 0);
  const total = items.reduce((a, p) => a + p.value, 0);
  if (!total) return chartEmpty(size, 'Пока никого');

  const r = (size - thickness) / 2;
  const c = size / 2;
  const circ = 2 * Math.PI * r;
  let off = 0;
  const rings = items.map((p) => {
    const len = (p.value / total) * circ;
    const seg = `<circle cx="${c}" cy="${c}" r="${r}" fill="none" stroke="${p.color}"
        stroke-width="${thickness}" stroke-dasharray="${len.toFixed(2)} ${(circ - len).toFixed(2)}"
        stroke-dashoffset="${(-off).toFixed(2)}" transform="rotate(-90 ${c} ${c})"
        class="ch-seg"><title>${esc2(p.label)}: ${p.value}</title></circle>`;
    off += len;
    return seg;
  }).join('');

  const mid = center || { top: total, sub: '' };
  return `<div class="donut" style="width:${size}px;height:${size}px">
    <svg viewBox="0 0 ${size} ${size}">
      <circle cx="${c}" cy="${c}" r="${r}" fill="none" stroke="var(--surface-3)" stroke-width="${thickness}"/>
      ${rings}
    </svg>
    <div class="donut-mid"><b class="num">${esc2(mid.top)}</b>${mid.sub ? `<span>${esc2(mid.sub)}</span>` : ''}</div>
  </div>`;
}

/* ── горизонтальные столбцы ──
   Сравнение именованных величин: трафик по нодам, выручка по тарифам.
   Горизонтальные, потому что подписи — слова, а вертикально их
   пришлось бы наклонять. */
function rankBars(rows, opts = {}) {
  const { fmt = (v) => v, color = 'var(--accent)', max: forced = null, height = 0 } = opts;
  const items = (rows || []).filter((r) => r && r.value != null);
  if (!items.length) return chartEmpty(height || 120);
  const max = forced || Math.max(...items.map((r) => r.value)) || 1;
  return `<div class="rank">${items.map((r) => `
    <div class="rank-row">
      <div class="rank-lbl">${r.icon || ''}<span>${esc2(r.label)}</span></div>
      <div class="rank-bar"><i style="width:${Math.max(1.5, (r.value / max) * 100).toFixed(1)}%;background:${r.color || color}"></i></div>
      <div class="rank-val num">${esc2(fmt(r.value))}</div>
    </div>`).join('')}</div>`;
}

/* ── тепловая карта по часам ──
   Сутки × дни недели. Показывает не величину, а распорядок: когда
   нагрузка, когда затишье — по одной таблице цифр это не видно. */
function heatmap(cells, opts = {}) {
  const { fmt = (v) => v, color = 'var(--accent)' } = opts;
  const vals = (cells || []).map((c) => c.value || 0);
  if (!vals.length || !vals.some((v) => v > 0)) return chartEmpty(150);
  const max = Math.max(...vals);
  const days = ['Пн', 'Вт', 'Ср', 'Чт', 'Пт', 'Сб', 'Вс'];
  const byKey = {};
  cells.forEach((c) => { byKey[`${c.day}:${c.hour}`] = c.value; });

  const rows = days.map((d, di) => {
    const tds = Array.from({ length: 24 }, (_, hh) => {
      const v = byKey[`${di}:${hh}`] || 0;
      // Прозрачность вместо палитры: одна и та же величина всегда даёт
      // один и тот же оттенок, и легенда не нужна.
      const a = v ? (0.12 + 0.88 * (v / max)) : 0;
      return `<i title="${days[di]} ${String(hh).padStart(2, '0')}:00 — ${esc2(fmt(v))}"
        style="background:${v ? `color-mix(in srgb, ${color} ${(a * 100).toFixed(0)}%, transparent)` : 'var(--surface-3)'}"></i>`;
    }).join('');
    return `<div class="hm-row"><span class="hm-day">${d}</span>${tds}</div>`;
  }).join('');

  return `<div class="heat">${rows}
    <div class="hm-axis"><span>00</span><span>06</span><span>12</span><span>18</span><span>23</span></div>
  </div>`;
}

/* Подсказка при наведении. Один обработчик на всю страницу: вешать
   слушатель на каждую из тридцати полос — тридцать лишних подписок,
   которые ещё и надо снимать при перерисовке. */
document.addEventListener('mouseover', (e) => {
  const hit = e.target.closest && e.target.closest('.ch-hit');
  if (!hit) return;
  const box = hit.closest('.chart');
  if (!box) return;
  const tip = box.querySelector('.ch-tip');
  const dot = box.querySelector('.ch-dot');
  const plot = box.querySelector('.ch-plot');
  if (!tip || !plot) return;

  const r = hit.getBoundingClientRect(), b = box.getBoundingClientRect();
  const cx = r.left - b.left + r.width / 2;

  tip.textContent = hit.dataset.t;
  tip.classList.add('on');
  tip.style.left = Math.round(cx) + 'px';

  if (dot) {
    dot.style.left = Math.round(cx) + 'px';
    dot.style.top = hit.dataset.y + '%';
    dot.classList.add('on');
  }
  // Подсказку у самого края разворачиваем внутрь, иначе она вылезает
  // за карточку и обрезается.
  tip.classList.toggle('edge-l', cx < 60);
  tip.classList.toggle('edge-r', b.width - cx < 60);
});
document.addEventListener('mouseout', (e) => {
  const hit = e.target.closest && e.target.closest('.ch-hit');
  if (!hit) return;
  const box = hit.closest('.chart');
  if (!box) return;
  box.querySelector('.ch-tip')?.classList.remove('on');
  box.querySelector('.ch-dot')?.classList.remove('on');
});
