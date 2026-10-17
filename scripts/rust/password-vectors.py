"""Independent Python/OpenSSL scrypt vectors; synthetic credentials only."""
from pathlib import Path
import hashlib,json,unicodedata
salt='000102030405060708090a0b0c0d0e0f'
values=['','Synthetic password 42!',' wrong whitespace ','\x00nul\npassword','Ångström','A\u030angstro\u0308m','ＡＢＣ１２３','\u212b\ufb03','\u1100\u1161\u11a8','\ufdfa','a'+'\u0301'*40,'🔐 secure Ø']
vectors=[]
for i,password in enumerate(values):
    normalized=unicodedata.normalize('NFKC',password)
    key=hashlib.scrypt(normalized.encode(),salt=salt.encode(),n=16384,r=16,p=1,dklen=64,maxmem=67108864)
    vectors.append({'id':f'python-{i}','password':password,'hash':salt+':'+key.hex()})
out={'generator':'Python stdlib hashlib.scrypt; NFKC for stable pre-Unicode14 characters; synthetic only','parameters':{'N':16384,'r':16,'p':1,'dkLen':64,'saltEncoding':'literal lowercase hex string UTF8'},'vectors':vectors}
p=Path(__file__).resolve().parents[2]/'crates/rentemester-runtime/fixtures/password.json';p.write_text(json.dumps(out,ensure_ascii=False,indent=2)+'\n')
