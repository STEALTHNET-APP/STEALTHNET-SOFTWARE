/* Страны для выбора локации ноды.
 *
 * Раньше код набирали руками в поле на два символа. Ошибиться было
 * нечем помешать: «GE» — это Грузия, а не Германия, и вместо флага
 * Германии в списке нод появлялся грузинский. Здесь список, по которому
 * ищут словом на русском или английском, а код проставляется сам.
 *
 * Формат: [код ISO 3166-1 alpha-2, название по-русски, по-английски].
 * Флаг не хранится — он выводится из кода функцией flag() в core.js.
 */
const COUNTRIES = [
  ['AE','ОАЭ','United Arab Emirates'],
  ['AL','Албания','Albania'],
  ['AM','Армения','Armenia'],
  ['AR','Аргентина','Argentina'],
  ['AT','Австрия','Austria'],
  ['AU','Австралия','Australia'],
  ['AZ','Азербайджан','Azerbaijan'],
  ['BA','Босния и Герцеговина','Bosnia and Herzegovina'],
  ['BD','Бангладеш','Bangladesh'],
  ['BE','Бельгия','Belgium'],
  ['BG','Болгария','Bulgaria'],
  ['BH','Бахрейн','Bahrain'],
  ['BR','Бразилия','Brazil'],
  ['BY','Беларусь','Belarus'],
  ['CA','Канада','Canada'],
  ['CH','Швейцария','Switzerland'],
  ['CL','Чили','Chile'],
  ['CN','Китай','China'],
  ['CO','Колумбия','Colombia'],
  ['CR','Коста-Рика','Costa Rica'],
  ['CY','Кипр','Cyprus'],
  ['CZ','Чехия','Czechia'],
  ['DE','Германия','Germany'],
  ['DK','Дания','Denmark'],
  ['DO','Доминикана','Dominican Republic'],
  ['DZ','Алжир','Algeria'],
  ['EC','Эквадор','Ecuador'],
  ['EE','Эстония','Estonia'],
  ['EG','Египет','Egypt'],
  ['ES','Испания','Spain'],
  ['FI','Финляндия','Finland'],
  ['FR','Франция','France'],
  ['GB','Великобритания','United Kingdom'],
  ['GE','Грузия','Georgia'],
  ['GR','Греция','Greece'],
  ['HK','Гонконг','Hong Kong'],
  ['HR','Хорватия','Croatia'],
  ['HU','Венгрия','Hungary'],
  ['ID','Индонезия','Indonesia'],
  ['IE','Ирландия','Ireland'],
  ['IL','Израиль','Israel'],
  ['IN','Индия','India'],
  ['IQ','Ирак','Iraq'],
  ['IR','Иран','Iran'],
  ['IS','Исландия','Iceland'],
  ['IT','Италия','Italy'],
  ['JO','Иордания','Jordan'],
  ['JP','Япония','Japan'],
  ['KE','Кения','Kenya'],
  ['KG','Киргизия','Kyrgyzstan'],
  ['KH','Камбоджа','Cambodia'],
  ['KR','Южная Корея','South Korea'],
  ['KW','Кувейт','Kuwait'],
  ['KZ','Казахстан','Kazakhstan'],
  ['LB','Ливан','Lebanon'],
  ['LT','Литва','Lithuania'],
  ['LU','Люксембург','Luxembourg'],
  ['LV','Латвия','Latvia'],
  ['MA','Марокко','Morocco'],
  ['MD','Молдова','Moldova'],
  ['ME','Черногория','Montenegro'],
  ['MK','Северная Македония','North Macedonia'],
  ['MN','Монголия','Mongolia'],
  ['MX','Мексика','Mexico'],
  ['MY','Малайзия','Malaysia'],
  ['NG','Нигерия','Nigeria'],
  ['NL','Нидерланды','Netherlands'],
  ['NO','Норвегия','Norway'],
  ['NP','Непал','Nepal'],
  ['NZ','Новая Зеландия','New Zealand'],
  ['OM','Оман','Oman'],
  ['PA','Панама','Panama'],
  ['PE','Перу','Peru'],
  ['PH','Филиппины','Philippines'],
  ['PK','Пакистан','Pakistan'],
  ['PL','Польша','Poland'],
  ['PT','Португалия','Portugal'],
  ['PY','Парагвай','Paraguay'],
  ['QA','Катар','Qatar'],
  ['RO','Румыния','Romania'],
  ['RS','Сербия','Serbia'],
  ['RU','Россия','Russia'],
  ['SA','Саудовская Аравия','Saudi Arabia'],
  ['SE','Швеция','Sweden'],
  ['SG','Сингапур','Singapore'],
  ['SI','Словения','Slovenia'],
  ['SK','Словакия','Slovakia'],
  ['TH','Таиланд','Thailand'],
  ['TJ','Таджикистан','Tajikistan'],
  ['TM','Туркменистан','Turkmenistan'],
  ['TN','Тунис','Tunisia'],
  ['TR','Турция','Türkiye'],
  ['TW','Тайвань','Taiwan'],
  ['UA','Украина','Ukraine'],
  ['US','США','United States'],
  ['UY','Уругвай','Uruguay'],
  ['UZ','Узбекистан','Uzbekistan'],
  ['VE','Венесуэла','Venezuela'],
  ['VN','Вьетнам','Vietnam'],
  ['ZA','ЮАР','South Africa'],
];

/* Название страны по коду. Незнакомый код возвращаем как есть: панель
   могла быть заведена раньше, чем страна попала в список, и подменять
   её на «—» значило бы потерять то, что человек уже указал. */
function countryName(cc) {
  const code = String(cc || '').trim().toUpperCase();
  const row = COUNTRIES.find((c) => c[0] === code);
  if (!row) return code || '';
  return LANG === 'en' ? row[2] : row[1];
}

/* Разметка поля выбора страны.

   Это обычный input с выпадающим списком, а не <select>: в select нет
   поиска, а стран под сотню. Значение хранится в скрытом поле — форма
   читает код, а человек видит название. */
function countryField(id, value) {
  const code = String(value || '').trim().toUpperCase();
  return `
    <div class="cpick" data-cpick="${id}">
      <input type="hidden" id="${id}" value="${esc(code)}">
      <div class="cpick-in">
        <span class="cpick-flag">${flag(code)}</span>
        <input class="inp cpick-q" autocomplete="off" spellcheck="false"
               placeholder="${LANG === 'en' ? 'Start typing a country' : 'Начните вводить страну'}"
               value="${esc(code ? countryName(code) : '')}">
        <span class="cpick-cc mono">${esc(code)}</span>
      </div>
      <div class="cpick-list" hidden></div>
    </div>`;
}

/* Оживление поля: поиск, клавиши, выбор.

   Ищем и по названию, и по коду, без учёта регистра и буквы «ё».
   Совпадение с начала слова идёт выше — «ру» должно давать Румынию и
   Россию раньше, чем Беларусь. */
function wireCountryField(root, id, onChange) {
  const box = root.querySelector(`[data-cpick="${id}"]`);
  if (!box) return;
  const hidden = box.querySelector('input[type="hidden"]');
  const q = box.querySelector('.cpick-q');
  const list = box.querySelector('.cpick-list');
  const flagEl = box.querySelector('.cpick-flag');
  const ccEl = box.querySelector('.cpick-cc');
  let cursor = -1;

  const norm = (s) => String(s).toLowerCase().replace(/ё/g, 'е');

  const matches = () => {
    const s = norm(q.value.trim());
    if (!s) return COUNTRIES.slice();
    const scored = [];
    for (const c of COUNTRIES) {
      const hay = [norm(c[1]), norm(c[2]), norm(c[0])];
      let best = -1;
      hay.forEach((h) => {
        const at = h.indexOf(s);
        if (at < 0) return;
        // 0 — совпало с начала, 1 — с начала слова, 2 — где-то внутри.
        const rank = at === 0 ? 0 : (h[at - 1] === ' ' || h[at - 1] === '-' ? 1 : 2);
        if (best < 0 || rank < best) best = rank;
      });
      if (best >= 0) scored.push([best, c]);
    }
    return scored.sort((a, b) => a[0] - b[0] || a[1][1].localeCompare(b[1][1]))
                 .map((x) => x[1]);
  };

  const render = () => {
    const rows = matches();
    cursor = rows.length ? 0 : -1;
    list.innerHTML = rows.length
      ? rows.map((c, i) => `
          <button type="button" class="cpick-row${i === 0 ? ' on' : ''}" data-cc="${c[0]}">
            <span class="f">${flag(c[0])}</span>
            <span class="n">${esc(LANG === 'en' ? c[2] : c[1])}</span>
            <span class="c mono">${c[0]}</span>
          </button>`).join('')
      : `<div class="cpick-empty">${LANG === 'en' ? 'no such country' : 'такой страны нет'}</div>`;
    list.hidden = false;
  };

  const pick = (cc) => {
    hidden.value = cc;
    q.value = countryName(cc);
    flagEl.textContent = flag(cc);
    ccEl.textContent = cc;
    list.hidden = true;
    if (onChange) onChange(cc);
  };

  const move = (d) => {
    const rows = [...list.querySelectorAll('.cpick-row')];
    if (!rows.length) return;
    rows[cursor]?.classList.remove('on');
    cursor = (cursor + d + rows.length) % rows.length;
    rows[cursor].classList.add('on');
    rows[cursor].scrollIntoView({ block: 'nearest' });
  };

  q.addEventListener('focus', render);
  q.addEventListener('input', render);
  q.addEventListener('keydown', (e) => {
    if (e.key === 'ArrowDown') { e.preventDefault(); list.hidden ? render() : move(1); }
    else if (e.key === 'ArrowUp') { e.preventDefault(); move(-1); }
    else if (e.key === 'Enter') {
      const row = list.querySelectorAll('.cpick-row')[cursor];
      if (row && !list.hidden) { e.preventDefault(); pick(row.dataset.cc); }
    } else if (e.key === 'Escape' && !list.hidden) {
      e.preventDefault(); e.stopPropagation();   // Esc закрывает список, не окно
      list.hidden = true;
    }
  });
  // Потеря фокуса возвращает подпись к выбранному: набранный, но не
  // выбранный текст иначе выглядел бы как значение поля.
  q.addEventListener('blur', () => setTimeout(() => {
    list.hidden = true;
    q.value = hidden.value ? countryName(hidden.value) : '';
  }, 150));
  list.addEventListener('mousedown', (e) => {
    const row = e.target.closest('.cpick-row');
    if (row) { e.preventDefault(); pick(row.dataset.cc); }
  });
}
