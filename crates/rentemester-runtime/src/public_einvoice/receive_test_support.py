# Standalone receive-only synthetic TLS fixture. No production network or data.
import http.server, ssl, json, sys, time, urllib.parse
XML = b'\xef\xbb\xbf<?xml version="1.0" encoding="UTF-8"?>\r\n<Invoice>synthetic \xc3\xa6</Invoice>\r\n'
class Handler(http.server.BaseHTTPRequestHandler):
 def log_message(self, *args): pass
 def reply(self, code, body, headers=None, stream=False):
  raw = body if isinstance(body, bytes) else json.dumps(body).encode()
  self.send_response(code)
  if not stream: self.send_header('Content-Length', str(len(raw)))
  for k,v in (headers or {}).items(): self.send_header(k,v)
  self.end_headers()
  try: self.wfile.write(raw)
  except (OSError,ssl.SSLError): pass
 def do_GET(self):
  parsed = urllib.parse.urlparse(self.path); mode = parsed.path.split('/')[1]
  with open(sys.argv[3], 'a') as f:
   f.write(json.dumps({'method':'GET','path':parsed.path,'query':urllib.parse.parse_qs(parsed.query),'authOk':self.headers.get('Authorization')=='synthetic-private-api-key-74','authorizationPresent':self.headers.get('Authorization') is not None,'accept':self.headers.get('Accept')})+'\n')
  download = parsed.path.endswith('/download')
  if mode in ('hang', 'xml-hang') and (download or mode=='hang'): time.sleep(.6)
  if mode=='redirect' or (download and mode=='xml-redirect'): return self.reply(307,b'',{'Location':'/normal/download?signature=synthetic-url-secret'})
  if mode=='refused' or (download and mode=='xml-refused'): return self.reply(403,b'synthetic-url-secret synthetic-private-api-key-74')
  if download:
   if mode=='xml-encoded': return self.reply(200,XML,{'Content-Encoding':'gzip'})
   if mode=='xml-large': return self.reply(200,b'x'*(10*1024*1024+1))
   if mode=='xml-stream-large': return self.reply(200,b'x'*(10*1024*1024+1),stream=True)
   if mode=='xml-exact-bound': return self.reply(200,b'x'*(10*1024*1024))
   if mode=='xml-empty': return self.reply(200,b'')
   return self.reply(200,XML)
  if mode=='malformed': return self.reply(200,b'synthetic-url-secret synthetic-private-api-key-74')
  if mode=='large': return self.reply(200,b'x'*(1024*1024+1))
  if mode=='stream-large': return self.reply(200,b'x'*(1024*1024+1),stream=True)
  if mode=='encoded': return self.reply(200,b'{}',{'Content-Encoding':'gzip'})
  url = 'https://127.0.0.1:%d/%s/download?signature=synthetic-url-secret' % (self.server.server_port,mode)
  row={'internalId':'stable-1','documentId':'doc-1','documentType':'Invoice','documentTypeUrn':'urn:synthetic','profileId':'profile','customizationId':'customization','receivedAt':'2026-01-01T00:00:00Z','senderParticipantType':'DK:CVR','senderParticipantId':'12345678','senderName':'Synthetic sender','destinationParticipantType':'DK:CVR','destinationParticipantId':'87654321','destinationName':'Synthetic receiver','sourceNetwork':'Nemhandel','downloadUrlExpiresAt':'2026-01-02T00:00:00Z','downloadUrl':url,'licenseKey':'synthetic-private-api-key-74','searchParams':{'private':url}}
  if mode=='missing': row={'internalId':'stable-1'}
  if mode=='bad-type': row['senderName']={}
  if mode=='secret-field': row['senderName']='synthetic-private-api-key-74'
  if mode=='secret-encoded': row['senderName']='%73ynthetic-private-api-key-74'
  if mode=='bad-url': row['downloadUrl']='http://example.invalid/secret'
  if mode=='no-id': row.pop('internalId')
  rows=[row]
  if mode=='too-many': rows=rows*101
  if mode=='empty': rows=[]
  if mode=='second' and urllib.parse.parse_qs(parsed.query).get('offset')==['1']: rows=[]
  return self.reply(200,{'documents':rows,'nextPageUrl':None if mode in ('empty','last') else 'https://untrusted.invalid/never-fetch?secret=synthetic-url-secret','licenseKey':'synthetic-private-api-key-74','participants':[{'secret':url}],'searchParams':{'url':url}})
server=http.server.ThreadingHTTPServer(('127.0.0.1',0),Handler); server.daemon_threads=True
context=ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER); context.load_cert_chain(sys.argv[1],sys.argv[2]); server.socket=context.wrap_socket(server.socket,server_side=True)
print(server.server_port,flush=True); server.serve_forever()
