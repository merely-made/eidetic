#!/usr/bin/env python3
"""M5 draft loopback fixture; never writes Turnstone state."""
import argparse, json, os, time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

class Handler(BaseHTTPRequestHandler):
    def do_GET(self):
        self.server.requests.append({"path": self.path, "time_ns": time.time_ns()})
        self.server.persist()
        if self.path == '/swap-a':
            self.server.a2 = True
            body = b'swapped'
        else:
            names = {'/A': 'a2.html' if self.server.a2 else 'a1.html',
                     '/B': 'b.html', '/Foreign': 'foreign.html', '/selection': 'selection.html'}
            name = names.get(self.path)
            if name is None:
                self.send_error(404); return
            try: body = (self.server.source_dir / name).read_bytes()
            except OSError:
                self.send_error(404); return
        self.send_response(200); self.send_header('Content-Type', 'text/html; charset=utf-8'); self.end_headers()
        self.wfile.write(body)
    def log_message(self, *_): pass
class Server(ThreadingHTTPServer):
    def persist(self):
        tmp = self.log.with_suffix('.tmp')
        tmp.write_text(json.dumps(self.requests, ensure_ascii=False, indent=2) + '\n')
        os.replace(tmp, self.log)

def main():
    p = argparse.ArgumentParser(); p.add_argument('--ready', type=Path, required=True); p.add_argument('--log', type=Path, required=True); p.add_argument('--port', type=int, default=0); p.add_argument('--source-dir', type=Path, required=True); a = p.parse_args()
    s = Server(('127.0.0.1', a.port), Handler); s.log = a.log; s.source_dir = a.source_dir; s.requests = []; s.a2 = False; s.persist()
    a.ready.write_text(json.dumps({'pid': os.getpid(), 'host': '127.0.0.1', 'port': s.server_port}) + '\n')
    try: s.serve_forever()
    finally: s.server_close()
if __name__ == '__main__': main()
