# SPDX-License-Identifier: MIT OR Apache-2.0
"""Service adapter process. JSON events on stdout; coordinator owns durable state.

Uses supported VoiceStudio HTTP and ComfyUI routes, without importing their code.
Never resubmits a request in recovery mode. No model keys are stored in requests.
"""
import json
import os
import sys
import time
import urllib.error
import urllib.parse
import urllib.request
import uuid
from pathlib import Path

def emit(kind, **data):
    print(json.dumps({"event": kind, **data}), flush=True)

def request(base, path, data=None, headers=None, raw=False, timeout=30):
    if data is not None and not isinstance(data, bytes):
        data=json.dumps(data).encode(); headers={"Content-Type":"application/json", **(headers or {})}
    token=os.environ.get("SPEARFIA_SERVICE_TOKEN", "")
    if token: headers={"Authorization":"Bearer "+token, **(headers or {})}
    req=urllib.request.Request(base.rstrip('/')+path,data=data,headers=headers or {})
    response=urllib.request.urlopen(req,timeout=timeout)
    if raw:return response
    with response:return json.load(response)

def download(base,path,target):
    target=Path(target);target.parent.mkdir(parents=True,exist_ok=True)
    partial=target.with_name(target.name+'.part')
    try:
        with request(base,path,raw=True,timeout=120) as response, partial.open('wb') as out:
            total=0
            while chunk:=response.read(1024*1024):
                total+=len(chunk)
                if total>2*1024**3:raise RuntimeError('Service artifact exceeds 2 GiB.')
                out.write(chunk)
            out.flush();os.fsync(out.fileno())
        os.replace(partial,target)
    finally:
        if partial.exists():partial.unlink()

def cancelled(req):return Path(req['stop_file']).exists()

def comfy_recover(base,key):
    found=set()
    queue=request(base,'/queue')
    for row in queue.get('queue_running',[])+queue.get('queue_pending',[]):
        if any(isinstance(x,dict) and x.get('spearfia_key')==key for x in row):found.add(str(row[1]))
    history=request(base,'/history?max_items=1000')
    for pid,entry in history.items():
        if any(isinstance(x,dict) and x.get('spearfia_key')==key for x in entry.get('prompt',[])):found.add(str(pid))
    if len(found)>1:raise RuntimeError('Multiple ComfyUI prompts match this request; reconcile them before retrying.')
    return next(iter(found),None)

def comfy(req):
    task=req['task'];base=task['url'];pid=req.get('provider_id')
    if not pid and req.get('recover_only'):
        pid=comfy_recover(base,req['key'])
        if not pid:
            emit('error',uncertain=True,message='Submission outcome is unknown; no matching prompt was found. Inspect ComfyUI before explicitly retrying.');return
    if not pid:
        emit('submitting',message='Submitting ComfyUI workflow')
        sent=request(base,'/prompt',{'prompt':task['workflow'],'client_id':str(uuid.uuid4()),'extra_data':{'spearfia_key':req['key']}})
        if sent.get('node_errors') or not sent.get('prompt_id'):raise RuntimeError('ComfyUI rejected the workflow: '+str(sent.get('node_errors',sent)))
        pid=str(sent['prompt_id'])
    emit('submitted',provider_id=pid,message='ComfyUI prompt recorded')
    deadline=time.monotonic()+req.get('timeout_seconds',3600)
    while time.monotonic()<deadline:
        if cancelled(req):
            request(base,'/queue',{'delete':[pid]})
            emit('cancelled',provider_id=pid,message='Queued prompt removed. If already running it remains recoverable; no global interrupt was sent.');return
        history=request(base,'/history/'+urllib.parse.quote(pid,safe='')).get(pid)
        if not history:
            queue=request(base,'/queue')
            if not any(str(row[1])==pid for row in queue.get('queue_running',[])+queue.get('queue_pending',[])):
                emit('error',uncertain=True,provider_id=pid,message='Saved prompt is absent from queue and retained history. Inspect the service before resolving this request; no resubmission was made.');return
        if history:
            status=history.get('status',{})
            if status.get('status_str')=='error':raise RuntimeError('ComfyUI failed: '+str(status.get('messages',''))[:2000])
            groups=history.get('outputs',{})
            if status.get('completed') is True:
                for output in task['outputs']:
                    keys={'audio':['audio'],'image':['images'],'video':['gifs','videos']}[output['kind']]
                    nodes=[output['node']] if output.get('node') else sorted(groups)
                    candidates=[f for node in nodes for kind in keys for f in groups.get(node,{}).get(kind,[])]
                    index=output.get('index',0)
                    if index>=len(candidates):raise RuntimeError('Workflow completed without the requested output selector.')
                    file=candidates[index]
                    name=str(file['filename'])
                    if '/' in name or '\\' in name or name in ('.','..'):raise RuntimeError('Unsafe service filename.')
                    query=urllib.parse.urlencode({'filename':name,'subfolder':file.get('subfolder',''),'type':file.get('type','output')})
                    download(base,'/view?'+query,output['path'])
                emit('complete',provider_id=pid,message='ComfyUI outputs downloaded');return
        emit('progress',provider_id=pid,message='Waiting for ComfyUI prompt')
        time.sleep(1)
    emit('error',uncertain=True,provider_id=pid,message='Prompt is still pending/running at the local timeout. Resume this ID; do not submit another prompt.')

def voice(req):
    task=req['task'];base=task['url'];pid=req.get('provider_id')
    if req.get('recover_only') or pid:
        history=request(base,'/history')
        matches=[h for h in history if (str(h.get('id'))==pid if pid else
                 h.get('seed')==req['seed'] and h.get('text')==task['text'][:200]
                 and h.get('profile_id')==task['profile_id']
                 and float(h.get('created_at',0))>=req['started']-2)]
        if len(matches)!=1:
            emit('error',uncertain=True,message='VoiceStudio exposes no resumable TTS request ID. No unique completed take matches this request; inspect its history before allowing another generation.');return
        take=matches[0];pid=str(take['id']);filename=take['audio_path']
    else:
        emit('submitting',message='VoiceStudio generation is synchronous; request seed is saved for recovery')
        form={k:task[k] for k in ['text','profile_id','instruct','language','speed']}
        form.update(num_step=task['steps'],stream='true',seed=req['seed'])
        with request(base,'/generate',urllib.parse.urlencode(form).encode(),{'Content-Type':'application/x-www-form-urlencoded','X-VoiceStudio-CSRF':'1'},raw=True,timeout=req.get('timeout_seconds',3600)) as response:
            take=None
            for line in response:
                if cancelled(req):
                    emit('error',uncertain=True,message='VoiceStudio request stopped locally; its server may still finish. Reconcile the saved seed before retrying.');return
                event=json.loads(line)
                if event.get('type')=='error':raise RuntimeError(str(event.get('message',event.get('error',event)))[:2000])
                if event.get('dropped_chunks',0):raise RuntimeError('VoiceStudio dropped narration chunks; the take is not accepted.')
                if event.get('type')=='done':take=event
                elif event.get('type') in ['start','chunk']:emit('progress',message='VoiceStudio '+event['type'])
            if not take:raise RuntimeError('VoiceStudio returned no completed take.')
        pid=str(take['id']);filename=take['audio_path']
    if '/' in filename or '\\' in filename:raise RuntimeError('Unsafe VoiceStudio output filename.')
    emit('submitted',provider_id=pid,message='VoiceStudio completed take recorded')
    download(base,'/audio/'+urllib.parse.quote(filename,safe=''),task['outputs'][0]['path'])
    emit('complete',provider_id=pid,message='VoiceStudio narration downloaded')

def multipart(fields,file):
    boundary='spearfia-'+uuid.uuid4().hex;parts=[]
    for key,value in fields.items():parts.append((f'--{boundary}\r\nContent-Disposition: form-data; name="{key}"\r\n\r\n{value}\r\n').encode())
    path=Path(file)
    if path.stat().st_size>200*1024*1024:raise RuntimeError('Align narration in sections smaller than 200 MiB.')
    parts.extend([(f'--{boundary}\r\nContent-Disposition: form-data; name="file"; filename="audio.wav"\r\nContent-Type: application/octet-stream\r\n\r\n').encode(),path.read_bytes(),f'\r\n--{boundary}--\r\n'.encode()])
    return b''.join(parts),{'Content-Type':'multipart/form-data; boundary='+boundary}

def alignment(req):
    task=req['task'];fields={'model':'whisper-1','response_format':'verbose_json','timestamp_granularities[]':'word'}
    if task.get('language') and task['language'].lower()!='auto':fields['language']=task['language']
    body,headers=multipart(fields,task['audio'])
    data=request(task['url'],'/v1/audio/transcriptions',body,headers,timeout=req.get('timeout_seconds',3600))
    if not data.get('words'):raise RuntimeError('Transcription service returned no measured word timestamps.')
    Path(task['outputs'][0]['path']).parent.mkdir(parents=True,exist_ok=True)
    Path(task['outputs'][0]['path']).write_text(json.dumps(data),encoding='utf-8')
    emit('complete',message='Measured word transcription downloaded')

def main():
    req=json.loads(Path(sys.argv[1]).read_text(encoding='utf-8'))
    try:{'comfyui':comfy,'voiceover':voice,'alignment':alignment}[req['task']['adapter']](req)
    except urllib.error.HTTPError as e:emit('error',uncertain=e.code>=500,message=f'Service returned HTTP {e.code}: '+e.read(2000).decode(errors='replace'))
    except (OSError,ValueError,RuntimeError,KeyError) as e:
        emit('error',uncertain=isinstance(e,OSError),message=str(e)[:3000])
if __name__=='__main__':main()
