"""Independent stdlib HMAC wire corpus. Run from any cwd; no product imports."""
import base64, hashlib, hmac, json
from pathlib import Path
from urllib.parse import quote
key=base64.urlsafe_b64encode(bytes(range(32))).decode().rstrip('=')
name='__Secure-better-auth.session_token'
def signed(token,key=key):
    return token+'.'+base64.b64encode(hmac.new(key.encode(),token.encode(),hashlib.sha256).digest()).decode()
def header(token,key=key): return name+'='+quote(signed(token,key),safe="~()*!.'-")
v=[]
def case(id,header,ok=False,**kw): v.append(dict(id=id,header=header,ok=ok,**kw))
for token in ['synthetic-token','with.dots','with+plus','with space','æøå','x'*512]:case('valid-'+str(len(v)),header(token),True,token=token)
good=header('synthetic-token');raw=name+'='+signed('synthetic-token')
case('raw-base64-plus',raw,True,token='synthetic-token')
case('quoted',name+'="'+good.split('=',1)[1]+'"',True,token='synthetic-token')
case('optional-spacing','other=x; \t'+good+' ; cache=ignored',True,token='synthetic-token')
case('dev',good.replace('__Secure-',''),True,profile='development',token='synthetic-token')
for id,value in [('missing',None),('empty',''),('cache-only','better-auth.session_data=anything'),('wrong-profile',good.replace('__Secure-','')),('duplicate',good+'; '+good),('secure-plain',good+'; '+good.replace('__Secure-','')),('bad-first',name+'=bad; '+good),('bad-last',good+'; '+name+'=bad'),('case-sensitive',good.replace('session_token','SESSION_TOKEN')),('wrong-key',header('synthetic-token',base64.urlsafe_b64encode(bytes([2])*32).decode().rstrip('='))),('tampered',good.replace('synthetic','synthetix')),('too-long-header','a='+'x'*8193+';'+good),('too-long-token',header('x'*513)),('empty-token',header('')),('control-token',header('x\ny')),('control-header',good+'\r\nx=y'),('nonascii-header',good+';x=ø'),('bad-percent',raw+'%'),('bad-utf8',name+'=%FF'+good.split('=',1)[1]),('quote-open',name+'="'+good.split('=',1)[1]),('quote-end',good+'"'),('truncated',raw[:-1]),('double-encoding',good.replace('%3D','%253D'))]:case(id,value)
for val in ['', 'invalid', '   ']: case('service-'+str(len(v)),good,service=val)
sig=signed('synthetic-token').rsplit('.',1)[1];alphabet='ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/'
noncanonical=sig[:-2]+alphabet[alphabet.index(sig[-2])^1]+'='
case('noncanonical-padbits',name+'=synthetic-token.'+noncanonical)
case('no-padding',raw.rstrip('='))
case('base64url',raw.replace('+','-').replace('/','_'))
# Include deterministic token whose tag actually exercises plus/slash.
for i in range(100):
    t='tag-'+str(i)
    if '+' in signed(t) and '/' in signed(t):
        case('literal-plus-slash',name+'='+signed(t),True,token=t)
        case('base64url-tag',name+'='+signed(t).replace('+','-').replace('/','_'))
        break
out={'generator':'Python stdlib hmac/hashlib/base64; synthetic fixtures only','key':key,'vectors':v}
path=Path(__file__).resolve().parents[2]/'crates/rentemester-runtime/fixtures/session-cookie.json'
path.write_text(json.dumps(out,ensure_ascii=False,indent=2)+'\n')
