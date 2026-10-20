# Private synthetic provider. Receipts, IDs, XML and credentials are synthetic only.
import http.server,ssl,json,sys,time,urllib.parse
class Handler(http.server.BaseHTTPRequestHandler):
 def log_message(self,*args): pass
 def reply(self,code,body,headers=None):
  raw=body if isinstance(body,bytes) else json.dumps(body).encode()
  self.send_response(code);self.send_header('content-type','application/json');self.send_header('content-length',str(len(raw)))
  for k,v in (headers or {}).items(): self.send_header(k,v)
  self.end_headers()
  try:self.wfile.write(raw)
  except (OSError,ssl.SSLError):pass
 def run_request(self):
  path=urllib.parse.urlparse(self.path);parts=path.path.split('/');mode=parts[1]
  length=int(self.headers.get('content-length','0'));raw=self.rfile.read(length)
  body=raw.decode() if self.headers.get('content-type')!='application/json' else json.loads(raw)
  with open(sys.argv[3],'a') as f:f.write(json.dumps({'method':self.command,'path':path.path,'query':urllib.parse.parse_qs(path.query),'body':body,'authOk':self.headers.get('authorization')=='synthetic-private-api-key-74','accept':self.headers.get('accept')})+'\n')
  if mode=='redirect':return self.reply(307,b'',{'location':'/normal/ap/api/rest/deliver-document'})
  if mode=='hang':time.sleep(.5)
  if mode=='large':return self.reply(200,b'x'*(1024*1024+1))
  if mode=='malformed':return self.reply(200,b'not JSON synthetic-private-api-key-74')
  if path.path.endswith('/register-company'):
   return self.reply(409,b'already registered') if mode=='invalid' else self.reply(200,{'companyKey':'company_key','message':'registered'})
  if '/register-participant/' in path.path:
   return self.reply(409,b'partly registered') if mode=='invalid' else self.reply(200,{'registeredOnNetwork':True,'webhookRegistered':False,'message':None})
  if path.path.endswith('/validate-document'):
   return self.reply(400,{'statusCode':400,'success':False,'errors':[{'context':'invoice','pattern':'required','description':'synthetic','xpath':None,'type':'error'}]}) if mode=='invalid' else self.reply(200,{'statusCode':200,'success':True,'errors':[]})
  code=200 if self.command=='GET' else 202
  status='delivered' if self.command=='GET' else 'queued-for-delivery'
  identity='wrong-doc' if mode=='wrong-id' else 'doc-1'
  if mode=='no-id':identity=None;code=502;status='temporary-upstream-error'
  if mode=='unknown-status':status='invented-delivered'
  if mode=='invalid':code=400;status='document-not-valid'
  return self.reply(code,{'statusCode':code,'documentStatus':status,'documentId':identity,'message':'synthetic-private-api-key-74 must never be echoed','publicUrl':'https://example.invalid/synthetic-private-api-key-74' if mode=='secret-url' else None,'ksef':None,'verificationUrl':None,'ksefDocumentNumber':None})
 do_POST=run_request
 do_GET=run_request
server=http.server.ThreadingHTTPServer(('127.0.0.1',0),Handler);server.daemon_threads=True
context=ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER);context.load_cert_chain(sys.argv[1],sys.argv[2]);server.socket=context.wrap_socket(server.socket,server_side=True)
print(server.server_port,flush=True);server.serve_forever()
