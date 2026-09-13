#!/usr/bin/env python3
"""Check public documentation links, language parity and section coverage."""
from pathlib import Path
import json,re,sys,urllib.parse,xml.etree.ElementTree as ET
ROOT=Path(__file__).resolve().parent.parent
errors=[]
paths=[ROOT/x for x in ('README.md','README.ru.md','CONTRIBUTING.md','CONTRIBUTING.ru.md','LICENSE.md','SECURITY.md','THIRD_PARTY_NOTICES.md','docs/README.md','docs/compatibility.md','docs/installation.md','docs/installation-releases.md','docs/media/README.md')]
paths += sorted((ROOT/'docs/en').rglob('*.md'))+sorted((ROOT/'docs/ru').rglob('*.md'))
ru={str(p.relative_to(ROOT/'docs/ru')) for p in (ROOT/'docs/ru').rglob('*.md')}
en={str(p.relative_to(ROOT/'docs/en')) for p in (ROOT/'docs/en').rglob('*.md')}
if ru!=en:errors.append('Language page mismatch: '+str(ru^en))
raw=(ROOT/'web/help.js').read_text().split('const SECTION_HELP = ',1)[1].split('\n};',1)[0]+'\n}'
section_ids=set(json.loads(raw))|{'getting-started','cabinet'}
for lang in ('ru','en'):
 actual={p.stem for p in (ROOT/f'docs/{lang}/sections').glob('*.md')}
 if actual!=section_ids:errors.append(f'{lang}: missing/extra sections {actual^section_ids}')
 for id in section_ids:
  if f'(sections/{id}.md)' not in (ROOT/f'docs/{lang}/README.md').read_text():errors.append(f'{lang}: section not indexed: {id}')
for path in paths:
 if not path.is_file():errors.append('Missing file: '+str(path.relative_to(ROOT)));continue
 text=path.read_text()
 # Ignore fenced examples; validate Markdown destinations and HTML image/link URLs.
 text=re.sub(r'```.*?```','',text,flags=re.S)
 links=re.findall(r'\]\(([^)]+)\)',text)+re.findall(r'(?:src|href)="([^"]+)"',text)
 for href in links:
  href=href.split(' "',1)[0].strip('<>')
  if href.startswith(('https://','http://','mailto:','#')):continue
  target=urllib.parse.unquote(href.split('#',1)[0])
  if target and not (path.parent/target).exists():errors.append(f'{path.relative_to(ROOT)}: broken link {href}')
 if '/.qa-runtime' in text or '/Users/mac' in text:errors.append(f'{path.relative_to(ROOT)}: local/private path')
for svg in (ROOT/'docs/media').rglob('*.svg'):
 try:ET.parse(svg)
 except ET.ParseError as e:errors.append(f'{svg.name}: invalid SVG: {e}')
for png in (ROOT/'docs/media').glob('*.png'):
 if not png.read_bytes().startswith(b'\x89PNG\r\n\x1a\n'):errors.append(f'{png.name}: invalid PNG')
if 'GNU AFFERO GENERAL PUBLIC LICENSE' not in (ROOT/'LICENSE').read_text():errors.append('Full license text missing')
if errors:
 print('\n'.join(errors));sys.exit(1)
print(f'Documentation checked: {len(paths)} pages, {len(section_ids)} sections in RU/EN, local links and media valid.')
