#!/usr/bin/env python3
"""Local browser fixture bridge. Binds loopback; only accepts local test targets."""
import http.server,json,subprocess
from pathlib import Path
ROOT=Path(__file__).resolve().parents[1];CLI=ROOT/'target/debug/hopkit-cli'
class Handler(http.server.BaseHTTPRequestHandler):
    def log_message(self,*args): pass
    def do_OPTIONS(self): self.send_response(204);self.send_header('Access-Control-Allow-Origin','http://127.0.0.1:1420');self.send_header('Access-Control-Allow-Headers','Content-Type');self.end_headers()
    def do_POST(self):
        try:
            obj=json.loads(self.rfile.read(int(self.headers['Content-Length'])))
            if obj['command']=='parse_target': args=['target',obj['input'],'--format','json']
            elif obj['command']=='network_overview':args=['network','--format','json']
            else:raise ValueError('Unsupported fixture command')
            p=subprocess.run([str(CLI),*args],capture_output=True,text=True,timeout=25)
            value=json.loads(p.stdout)
            self.send_response(200);self.send_header('Content-Type','application/json');self.send_header('Access-Control-Allow-Origin','http://127.0.0.1:1420');self.end_headers();self.wfile.write(json.dumps(value).encode())
        except Exception as e:self.send_error(400,str(e))
http.server.ThreadingHTTPServer(('127.0.0.1',18432),Handler).serve_forever()
