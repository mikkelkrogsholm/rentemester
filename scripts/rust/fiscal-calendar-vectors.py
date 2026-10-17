"""Independent fixed calendar corpus; Python stdlib only, no product imports."""
from pathlib import Path
import calendar
import hashlib
import json
import re
import sys

ROOT = Path(__file__).resolve().parents[2]
OUT = ROOT / 'docs/architecture/rust-target/implementation/fiscal-calendar'
cases = []

def error(code):
    return {'error': code}

def iso(year, month, day):
    return f'{year:04}-{month:02}-{day:02}'

def valid(text):
    if re.fullmatch(r'[0-9]{4}-[0-9]{2}-[0-9]{2}', text) is None:
        return False
    y, m, d = map(int, text.split('-'))
    return 1 <= m <= 12 and 1 <= d <= calendar.monthrange(y, m)[1]

def fiscal(text, start, strategy):
    if not valid(text): return error('invalid_date')
    if start not in range(1, 13): return error('invalid_start_month')
    if strategy not in ('start-year', 'end-year', 'span'): return error('invalid_label_strategy')
    y, m, _ = map(int, text.split('-'))
    sy = y - int(start != 1 and m < start)
    ey = sy + int(start != 1)
    if sy < 0 or ey > 9999: return error('date_out_of_range')
    em = 12 if start == 1 else start - 1
    label = f'{sy:04}' if strategy == 'start-year' else f'{sy:04}/{ey % 100:02}' if strategy == 'span' and start != 1 else f'{ey:04}'
    return dict(startYear=sy, endYear=ey, start=iso(sy, start, 1), end=iso(ey, em, calendar.monthrange(ey, em)[1]), displayLabel=label, identifierLabel=label.replace('/', '-'))

def deadline(text):
    if not valid(text): return error('invalid_date')
    y, m, d = map(int, text.split('-'))
    absolute = y * 12 + m - 1 + 6
    ty, zero_month = divmod(absolute, 12)
    tm = zero_month + 1
    if ty > 9999: return error('date_out_of_range')
    last = calendar.monthrange(ty, tm)[1]
    target_day = last if d == calendar.monthrange(y, m)[1] else min(d, last)
    return iso(ty, tm, target_day)

def add(op, text, policy, start=None, strategy=None):
    row = dict(id=f'vector-{len(cases)+1:05}', op=op, date=text)
    if op == 'fiscal': row.update(startMonth=start, strategy=strategy)
    row['expected'] = fiscal(text, start, strategy) if op == 'fiscal' else deadline(text) if op == 'deadline' else text if valid(text) else error('invalid_date')
    row['legacy'] = policy
    cases.append(row)

for y in [0, 1, 99, 100, 999, 1000, 1900, 1999, 2000, 2024, 2100, 9999]:
    for m in range(1, 13):
        last = calendar.monthrange(y, m)[1]
        text = iso(y, m, last)
        for start in range(1, 13):
            for strategy in ['start-year', 'end-year', 'span']:
                result = fiscal(text, start, strategy)
                add('fiscal', text, 'difference' if 'error' in result or result['startYear'] < 1000 else 'match', start, strategy)
        for d in [1, 15, last]:
            text = iso(y, m, d)
            target_year = y + int(m > 6)
            legacy_matches = target_year <= 9999 and (y >= 1000 or (y >= 100 and (d == last or target_year >= 1000)))
            add('deadline', text, 'match' if legacy_matches else 'difference')
for text in ['0000-02-29','0001-01-01','0099-12-31','1900-02-28','2000-02-29','2100-02-28','9999-12-31']:
    add('date', text, 'match')
for text in ['', '2023-02-29','1900-02-29','2100-02-29','2026-04-31','2026-00-01','2026-01-00','2026-13-01','2026-01-32','1-01-01','10000-01-01','2026-1-01','２０２６-01-01','2026-01-01T00:00:00Z']:
    for op in ['date','fiscal','deadline']: add(op, text, 'refusal', 7, 'span')
for text in [' 2026-01-01','2026-01-01 ', '\ufeff2026-01-01']:
    for op in ['date','fiscal','deadline']: add(op, text, 'typed_input', 7, 'span')
for start in [-1,0,13,100]: add('fiscal','2026-07-01','typed_input',start,'span')
for strategy in ['unknown','SPAN','']: add('fiscal','2026-07-01','typed_input',7,strategy)
for text in ['2026-06-30','2026-07-01','0099-07-01']:
    for strategy in ['start-year','end-year','span']:add('fiscal',text,'difference' if text.startswith('0099') else 'match',7,strategy)

payload = ''.join(json.dumps(c, ensure_ascii=False, separators=(',', ':')) + '\n' for c in cases)
manifest = dict(format='rentemester-fiscal-calendar-vectors/1', generator='Python standard-library calendar.monthrange; no product imports', generator_sha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest(), vectors_sha256=hashlib.sha256(payload.encode()).hexdigest(), checks=len(cases), operations={op:sum(c['op']==op for c in cases) for op in ['date','fiscal','deadline']}, legacy_policies={p:sum(c['legacy']==p for c in cases) for p in ['match','difference','refusal','typed_input']})
files = {'vectors.jsonl':payload,'vectors-manifest.json':json.dumps(manifest, indent=2)+'\n'}
for name, text in files.items():
    if '--check' in sys.argv: assert (OUT/name).read_text() == text, name
    else: (OUT/name).write_text(text)
print(json.dumps(manifest))
