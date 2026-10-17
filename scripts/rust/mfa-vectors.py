"""Synthetic independent libsodium XChaCha20-Poly1305 and stdlib HMAC OTP corpus.
Requires host libsodium only to regenerate AEAD vectors; runtime uses no Python/library.
"""
import base64,ctypes,ctypes.util,hashlib,hmac,json,struct
from pathlib import Path
library=ctypes.util.find_library('sodium')
if not library: raise SystemExit('libsodium required to regenerate independent AEAD fixtures')
sodium=ctypes.CDLL(library)
assert sodium.sodium_init()>=0
sodium.sodium_version_string.restype=ctypes.c_char_p
enc=sodium.crypto_aead_xchacha20poly1305_ietf_encrypt
enc.argtypes=[ctypes.c_void_p,ctypes.POINTER(ctypes.c_ulonglong),ctypes.c_void_p,ctypes.c_ulonglong,ctypes.c_void_p,ctypes.c_ulonglong,ctypes.c_void_p,ctypes.c_void_p,ctypes.c_void_p];enc.restype=ctypes.c_int
key=base64.urlsafe_b64encode(bytes(range(32))).decode().rstrip('=')
values=[b'',b'synthetic-mfa-secret','æ雪💡\0'.encode(),b'["abcde-FG123","67890-abcDE"]',b'abcdefghijklmnopqrstuvwxyz012345','\ufefftext'.encode(),'\ufeff\ufefftext'.encode(),'prefix\ufefftext'.encode(),b'\xff\xfe\x00']
vectors=[]
for i,plain in enumerate(values):
 nonce=bytes((n+i)%256 for n in range(24));out=ctypes.create_string_buffer(len(plain)+16);length=ctypes.c_ulonglong()
 assert enc(out,ctypes.byref(length),plain,len(plain),None,0,None,nonce,hashlib.sha256(key.encode()).digest())==0
 raw=(nonce+out.raw[:length.value]).hex()
 try:
  plaintext=plain.decode('utf8');valid=True
 except UnicodeDecodeError:
  plaintext=None;valid=False
 decoded=plaintext[1:] if valid and plaintext.startswith('\ufeff') else plaintext
 vectors.append({'id':f'libsodium-{i}','key':key,'version':7,'nonce':nonce.hex(),'plaintext':plaintext,'decoded':decoded,'plaintextHex':plain.hex(),'raw':raw,'envelope':'$ba$7$'+raw,'validUtf8':valid})
def otp(secret,counter,digits=6):
 digest=hmac.new(secret,struct.pack('>Q',counter),hashlib.sha1).digest();offset=digest[-1]&15
 return str((int.from_bytes(digest[offset:offset+4],'big')&0x7fffffff)%10**digits).zfill(digits)
secret='12345678901234567890';hotp=[{'counter':i,'code':otp(secret.encode(),i)} for i in range(10)]
assert [v['code'] for v in hotp]==['755224','287082','359152','969429','338314','254676','287922','162583','399871','520489']
times=[59,1111111109,1111111111,1234567890,2000000000,20000000000]
rfc8=['94287082','07081804','14050471','89005924','69279037','65353130'];totp=[]
for sec,expected in zip(times,rfc8):
 assert otp(secret.encode(),sec//30,8)==expected
 totp.append({'nowMs':sec*1000,'counter':sec//30,'code':expected[-6:]})
result={'generator':'Python stdlib HMAC-SHA1 + libsodium crypto_aead_xchacha20poly1305_ietf_encrypt','libsodiumVersion':sodium.sodium_version_string().decode(),'sources':['https://www.rfc-editor.org/rfc/rfc4226#appendix-D','https://www.rfc-editor.org/rfc/rfc6238#appendix-B','https://doc.libsodium.org/secret-key_cryptography/aead/chacha20-poly1305/xchacha20-poly1305_construction'],'aead':vectors,'rfcSecret':secret,'hotp':hotp,'totp':totp}
import argparse
parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('--output',type=Path,default=Path(__file__).resolve().parents[2].joinpath('crates/rentemester-runtime/fixtures/mfa.json'));args=parser.parse_args()
args.output.write_text(json.dumps(result,ensure_ascii=False,indent=2)+'\n')
