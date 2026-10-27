# SPDX-License-Identifier: MIT OR Apache-2.0
"""Deterministic local service contracts; never calls a model or external service."""
import io
import json
import sys
import time
import urllib.parse
import wave
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

root=Path(sys.argv[1]);history={};takes=[];counts={'prompt':0,'generate':0,'align':0}
buffer=io.BytesIO()
with wave.open(buffer,'wb') as audio:
    audio.setparams((1,2,8000,8000,'NONE','NONE'));audio.writeframes(bytes(16000))
wav=buffer.getvalue()

class Handler(BaseHTTPRequestHandler):
    def log_message(self,*_):pass
    def send(self,data,kind='application/json'):
        self.send_response(200);self.send_header('Content-Type',kind);self.end_headers()
        self.wfile.write(data if isinstance(data,bytes) else json.dumps(data).encode())
    def do_GET(self):
        path=urllib.parse.urlparse(self.path).path
        if path=='/queue':return self.send({'queue_running':[],'queue_pending':[]})
        if path=='/history':return self.send(takes if self.headers.get('X-Test-Voice') else history)
        if path=='/voice/history':return self.send(takes)
        if path.startswith('/history/'):return self.send({k:v for k,v in history.items() if k==path.rsplit('/',1)[1]})
        if path=='/counts':return self.send(counts)
        if path=='/view' or '/audio/' in path:return self.send(wav,'audio/wav')
        self.send({})
    def do_POST(self):
        body=self.rfile.read(int(self.headers.get('Content-Length',0)))
        if self.path=='/prompt':
            counts['prompt']+=1;data=json.loads(body);pid='prompt-'+str(counts['prompt'])
            history[pid]={'prompt':[1,pid,data['prompt'],data.get('extra_data',{})], 'status':{'completed':True,'status_str':'success'},'outputs':{'7':{'audio':[{'filename':'out.wav','type':'output'}]}}}
            self.send({'prompt_id':pid,'node_errors':{}})
        elif self.path=='/voice/generate':
            counts['generate']+=1;form=urllib.parse.parse_qs(body.decode());pid='take-'+str(counts['generate'])
            take={'id':pid,'text':form['text'][0][:200],'profile_id':form['profile_id'][0],'seed':int(form['seed'][0]),'created_at':time.time(),'audio_path':'voice.wav','duration_seconds':1}
            takes.append(take)
            self.send((json.dumps({'type':'start'})+'\n'+json.dumps(dict(take,type='done',dropped_chunks=0))+'\n').encode(),'application/x-ndjson')
        elif self.path=='/voice/v1/audio/transcriptions':
            counts['align']+=1
            self.send({'words':[{'word':'One','start':0.1,'end':0.3},{'word':'two','start':0.4,'end':0.6},{'word':'three.','start':0.7,'end':0.9}]})
        else:self.send({})
        (root/'service-counts.json').write_text(json.dumps(counts))

server=ThreadingHTTPServer(('127.0.0.1',0),Handler)
(root/'service-port').write_text(str(server.server_port));server.serve_forever()
