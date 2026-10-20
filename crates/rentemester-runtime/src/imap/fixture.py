# Private synthetic protocol server for Rust unit tests; no external mailbox.
import json, socket, ssl, sys, threading, time
cert, key, trace, security, mode = sys.argv[1:]
lock = threading.Lock()
def log(value):
    with lock:
        with open(trace, 'a') as f: f.write(value + '\n')
def serve(raw):
    log('CONNECT')
    try:
        s = context.wrap_socket(raw, server_side=True) if context else raw
        with s:
            if mode == 'hang': time.sleep(1); return
            if mode == 'long-line': s.sendall(b'* ' + b'x'*65536 + b'\r\n'); return
            if mode == 'malformed': s.sendall(b'not-imap\r\n'); return
            s.sendall(b'* OK synthetic\r\n')
            f = s.makefile('rb')
            while True:
                line = f.readline(16384)
                if not line: break
                tag, cmd = line.decode().rstrip('\r\n').split(' ', 1)
                log('LOGIN' if cmd.startswith('LOGIN ') else cmd)
                if cmd.startswith('LOGIN '):
                    s.sendall((tag + (' NO refused\r\n' if mode == 'auth' else ' OK authenticated\r\n')).encode())
                elif cmd.startswith('SELECT '):
                    s.sendall(b'* OK [UIDVALIDITY 7] stable\r\n' + (tag+' OK selected\r\n').encode())
                elif cmd.startswith('UID SEARCH '):
                    first = int(cmd.split()[-1].split(':')[0]); count = 33 if mode == 'many' else 2
                    ids = ' '.join(str(i) for i in range(first, count+1))
                    s.sendall(('* SEARCH '+ids+'\r\n'+tag+' OK search\r\n').encode())
                elif cmd.startswith('UID FETCH '):
                    uid = int(cmd.split()[2]); actual = 999 if mode == 'wrong-uid' else uid
                    body = b'Message-ID: <synthetic>\r\n\r\nraw\x00\xff\r\n'
                    size = 25*1024*1024+1 if mode in ['oversize','wrong-uid'] else len(body)
                    s.sendall(('* 1 FETCH (UID '+str(actual)+' BODY[] {'+str(size)+'}\r\n').encode())
                    if mode in ['oversize','wrong-uid']: f.read(1); return
                    # Split both literal and closing response across actual socket writes.
                    s.sendall(body[:7]); s.sendall(body[7:]); s.sendall((')\r\n'+tag+' OK fetched\r\n').encode())
                elif cmd == 'LOGOUT':
                    s.sendall(('* BYE done\r\n'+tag+' OK logout\r\n').encode());return
                else: raise AssertionError(cmd)
    except (OSError, ssl.SSLError): pass
    finally: raw.close()
context = None
if security == 'tls':
    context = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER); context.load_cert_chain(cert,key)
listener = socket.socket(); listener.bind(('127.0.0.1',0)); listener.listen()
print(listener.getsockname()[1],flush=True)
while True:
    s,_=listener.accept();threading.Thread(target=serve,args=(s,),daemon=True).start()
