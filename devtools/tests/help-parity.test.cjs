'use strict';
const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');

const root = path.resolve(__dirname, '../..');

function extractKeys(text) {
  const match = text.match(/SECTION_HELP(?:_EN)?\s*=\s*\{([\s\S]*?)\};/);
  if (!match) return [];
  const keys = [];
  const re = /["']([a-zA-Z0-9_-]+)["']\s*:/g;
  let m;
  while ((m = re.exec(match[1])) !== null) {
    keys.push(m[1]);
  }
  return keys;
}

test('bilingual parity between RU and EN section help entries', () => {
  const helpRu = fs.readFileSync(path.join(root, 'web', 'help.js'), 'utf8');
  const helpEn = fs.readFileSync(path.join(root, 'web', 'help-en.js'), 'utf8');

  const ruKeys = extractKeys(helpRu);
  const enKeys = extractKeys(helpEn);

  // 'getting-started' is registered in help.js dynamically
  const ruSet = new Set([...ruKeys, 'getting-started']);
  const enSet = new Set(enKeys);

  const missingInEn = [...ruSet].filter(k => !enSet.has(k));
  const missingInRu = [...enSet].filter(k => !ruSet.has(k));

  assert.deepEqual(missingInEn, [], 'Help entries present in RU but missing in EN');
  assert.deepEqual(missingInRu, [], 'Help entries present in EN but missing in RU');
});

test('all registered pages have a help drawer entry', () => {
  const helpRu = fs.readFileSync(path.join(root, 'web', 'help.js'), 'utf8');
  const ruKeys = extractKeys(helpRu);
  const allHelpKeys = new Set([...ruKeys, 'getting-started']);

  const webFiles = fs.readdirSync(path.join(root, 'web')).filter(f => f.endsWith('.js'));
  const registeredPages = new Set();
  for (const file of webFiles) {
    const content = fs.readFileSync(path.join(root, 'web', file), 'utf8');
    const re = /registerPage\(\{\s*id:\s*['"]([a-zA-Z0-9_-]+)['"]/g;
    let pm;
    while ((pm = re.exec(content)) !== null) {
      registeredPages.add(pm[1]);
    }
  }

  // 'login' and '404' are system screens without top navigation or drawer
  const expectedWithoutHelp = new Set(['login', '404']);
  const missingHelp = [...registeredPages].filter(p => !allHelpKeys.has(p) && !expectedWithoutHelp.has(p));

  assert.deepEqual(missingHelp, [], 'Registered pages lacking a help drawer entry');
});
