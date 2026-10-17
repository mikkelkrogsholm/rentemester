"""Independent exact staged arithmetic fixtures, Python Decimal only, no product imports."""
from pathlib import Path
from decimal import Decimal as D, localcontext, ROUND_HALF_UP
import hashlib
import json
import re
import sys
ROOT = Path(__file__).resolve().parents[2]
OUT = ROOT/'docs/architecture/rust-target/implementation/exact-arithmetic'
rows = []
class Refusal(Exception): pass

def raw(value):
    if not isinstance(value,str) or not value.isascii() or not 1 <= len(value) <= 2048: raise Refusal('invalid_input')
    if re.fullmatch(r'[+-]?(?:[0-9]+(?:\.[0-9]*)?|\.[0-9]+)(?:[eE][+-]?[0-9]+)?',value) is None: raise Refusal('invalid_input')
    parts = re.split('[eE]',value)
    if len(parts)>1 and not -400 <= int(parts[1]) <= 400: raise Refusal('invalid_input')
    return D(value)

def quant(value,scale): return value.quantize(D(1).scaleb(-scale),rounding=ROUND_HALF_UP)
def fixed(value,scale): return format(abs(value) if value==0 else value,f'.{scale}f')
def rate(value):
    try: result=quant(raw(value),6)
    except Refusal: raise Refusal('invalid_rate')
    if result<=0: raise Refusal('invalid_rate')
    return result

def currency(value):
    if not isinstance(value,str) or re.fullmatch('[A-Z]{3}',value) is None: raise Refusal('invalid_currency')
    return value

def canonical(value,scale,code,positive=False):
    if not isinstance(value,str) or re.fullmatch(r'-?(?:0|[1-9][0-9]*)\.[0-9]{'+str(scale)+'}',value) is None: raise Refusal(code)
    number=D(value)
    if (value.startswith('-') and number==0) or (positive and number<=0): raise Refusal(code)
    return value

def evaluate(v):
    try:
        op=v['op']
        if op=='currency': return currency(v['input'])
        if op=='currency-wire': return currency(v['input'])
        if op=='money-wire': return canonical(v['input'],2,'invalid_input')
        if op=='rate-wire': return canonical(v['input'],6,'invalid_rate',True)
        if op=='money': return fixed(quant(raw(v['input']),2),2)
        if op=='rate': return fixed(rate(v['input']),6)
        if op=='multiply': return fixed(quant(raw(v['left'])*raw(v['right']),2),2)
        if op=='percent': return fixed(quant(quant(raw(v['left']),2)*quant(raw(v['right']),2)/100,2),2)
        if op=='add': return fixed(quant(raw(v['left']),2)+quant(raw(v['right']),2),2)
        if op=='subtract': return fixed(quant(raw(v['left']),2)-quant(raw(v['right']),2),2)
        if op=='fx':
            currency(v['currency']);r=rate(v['right'])
            return fixed(quant(quant(raw(v['left']),2)*r,2),2)
        raise AssertionError(op)
    except Refusal as e: return {'error':str(e)}

def add(op,**inputs):
    v=dict(id=f'arithmetic-{len(rows)+1:05}',op=op,**inputs)
    v['expected']=evaluate(v)
    rows.append(v)

with localcontext() as context:
    context.prec=20000
    values=['0','-0','+0','.5','-.5','1.','1.005','-1.005','2.675','-2.675','1.0049','0.0049999999999999995','0.0050000000000000001','9007199254740993.015','1e-400','1e400','1e-6','1e3','+01.00','9'*2042+'e400','9'*2048]
    for value in values:
        add('money',input=value);add('rate',input=value)
    for whole in range(-8,9):
        for fraction in ['004999','005','005001','994999','995','995001']:
            value=('-' if whole<0 else '')+str(abs(whole))+'.'+fraction
            add('money',input=value)
    amounts=['0','0.01','-0.01','0.005','-0.005','1.005','-1.005','1.15','-1.15','2.675','-2.675','100','100.005','9999999999999999.99','1e-12','1e300']
    factors=['0','1','-1','0.1','1.5','-1.5','7.4603855','0.0000005','1e-6','1e300']
    for left in amounts:
        for right in factors:
            for op in ['multiply','percent','add','subtract']:add(op,left=left,right=right)
            add('fx',left=left,right=right,currency='USD')
    for value in ['','.', '+','-', 'NaN','Infinity','1e401','1e-401','1e','1e2e3','1.2.3',' 1','1 ','１.00','1'*2049]:
        add('money',input=value);add('rate',input=value)
        add('multiply',left=value,right='1.5');add('percent',left='1.00',right=value)
    for value in ['DKK','USD','EUR','XYZ','','dkk',' USD ','DK','DKKK','123','D€K','ＤＫＫ','DK\0']:
        add('currency',input=value);add('currency-wire',input=value)
    for value in ['0','-1','0.0000004','0.0000005','-0.0000005','7.4603854','7.4603855','7.4603856']:
        add('rate',input=value);add('fx',left='1.15',right=value,currency='USD')
    for value in ['0.00','1.00','-1.00','1','+1.00','01.00','-0.00','1.005','1e0',' 1.00',1,None,'9'*5000+'.99']:
        add('money-wire',input=value)
    for value in ['0.000001','1.000000','7.460380','0.000000','-1.000000','1','01.000000','+1.000000','1.0000001',1,None,'9'*5000+'.123456']:
        add('rate-wire',input=value)
    for value in [fixed(quant(raw('9'*2042+'e400'),2),2),fixed(quant(raw('9'*2042+'e400')**2,2),2)]:add('money-wire',input=value)
    add('rate-wire',input=fixed(rate('9'*2042+'e400'),6))
payload=''.join(json.dumps(v,ensure_ascii=False,separators=(',',':'))+'\n' for v in rows)
manifest=dict(format='rentemester-exact-arithmetic-vectors/1',generator='Python Decimal/ROUND_HALF_UP; no product imports',generator_sha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),vectors_sha256=hashlib.sha256(payload.encode()).hexdigest(),checks=len(rows),operations={op:sum(v['op']==op for v in rows) for op in sorted({v['op'] for v in rows})})
for name,text in {'vectors.jsonl':payload,'vectors-manifest.json':json.dumps(manifest,indent=2)+'\n'}.items():
    if '--check' in sys.argv: assert (OUT/name).read_text(encoding='utf-8')==text,name
    else: (OUT/name).write_text(text,encoding='utf-8',newline='\n')
print(json.dumps(manifest))
