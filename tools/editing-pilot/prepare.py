# SPDX-License-Identifier: MIT OR Apache-2.0
"""Prepare a read-only Star 4 audition from a saved assembly; media stays outside Git."""
import argparse
import hashlib
import json
import math
import random
import re
import struct
import subprocess
import wave
from pathlib import Path

FPS = 30
RATE = 48000
VOICE = 'A-roll/voiceover/06_star-4.mp3'
START_QUOTE = 'That is the answer to the first question from the opening'


def read(path):
    return json.loads(path.read_text(encoding='utf-8'))


def save(path, data):
    path.write_text(json.dumps(data, indent=2, ensure_ascii=False) + '\n', encoding='utf-8')


def sha(path):
    with path.open('rb') as handle:
        return hashlib.file_digest(handle, 'sha256').hexdigest()


def run(args):
    subprocess.run([str(a) for a in args], check=True)


def probe(path):
    return json.loads(subprocess.check_output(['ffprobe', '-v', 'error', '-show_format', '-show_streams', '-of', 'json', str(path)], text=True))


def normal(text):
    return re.sub(r'[^a-z0-9]', '', text.lower())


def anchor(words, quote):
    wanted = [normal(w) for w in quote.split()]
    matches = [i for i in range(len(words)-len(wanted)+1)
               if [normal(w[0]) for w in words[i:i+len(wanted)]] == wanted]
    if len(matches) != 1:
        raise ValueError('Expected one measured anchor: ' + quote)
    return float(words[matches[0]][1])


def picture_filter(frames, hold):
    return (f'scale=1920:1080:force_original_aspect_ratio=decrease,pad=1920:1080:(ow-iw)/2:(oh-ih)/2,'
            f'setsar=1,fps=30,trim=end_frame={frames-hold},setpts=PTS-STARTPTS,tpad=stop_mode=clone:stop=-1')


def sound(path, kind, seed):
    """Original audition sounds, not approved library assets or model generations."""
    rng = random.Random(seed)
    seconds = {'pop': .18, 'whoosh': .42, 'impact': .48}[kind]
    samples, low = [], 0.0
    for i in range(round(seconds*RATE)):
        t = i/RATE
        noise = rng.uniform(-1, 1)
        low = .85*low + .15*noise
        if kind == 'whoosh':
            value = (noise-low)*math.sin(math.pi*t/seconds)**3
        elif kind == 'pop':
            value = math.sin(2*math.pi*(820*t-1700*t*t))*math.exp(-38*t)*min(1, t/.008)
        else:
            value = (.7*math.sin(2*math.pi*(115*t-65*t*t))+.3*low)*math.exp(-13*t)*min(1,t/.009)
        samples.append(value)
    peak = max(abs(v) for v in samples)
    samples = [v/peak*0.70 for v in samples]
    with wave.open(str(path), 'wb') as out:
        out.setparams((1, 2, RATE, 0, 'NONE', 'not compressed'))
        out.writeframes(b''.join(struct.pack('<h', round(v*32767)) for v in samples))
    transient = max(range(len(samples)), key=lambda i: abs(samples[i]))
    return dict(file=path.name, duration=math.ceil(seconds*FPS), transient=round(transient/RATE*FPS),
                kind=kind, seed=seed, origin='procedural audition; pending creator review', sha256=sha(path))


def prepare(project, out):
    project, out = project.resolve(), out.resolve()
    if out == project or project in out.parents or any((p/'.git').exists() for p in [out, *out.parents]):
        raise ValueError('Output must be outside the source project and Git')
    if out.exists():
        raise ValueError('Use a new output folder; existing auditions are never overwritten')
    for source, approved in [('script/script.md', 'sync/approved-script.md'), ('channel.md', 'sync/approved-channel.md')]:
        if (project/source).read_bytes() != (project/approved).read_bytes():
            raise ValueError('Source approval is absent or stale')
    protected = ['project.json', 'links.json', 'script/script.md', 'visuals/visuals.md', 'channel.md']
    hashes = {p: sha(project/p) for p in protected}
    words = read(project/'media/vo-words.json')[VOICE]
    if words['sha256'] != sha(project/VOICE):
        raise ValueError('Word observations are stale for this voice file')
    words = words['words']
    start = anchor(words, START_QUOTE)
    length = 90
    start_frame = round(start*FPS)
    plan_path = max((project/'sync/plans').glob('*.json'), key=lambda p:p.stat().st_mtime)
    plan = read(plan_path)
    if plan['timebase'] != {'num': FPS, 'den': 1}:
        raise ValueError('Pilot currently requires a 30 fps saved plan')
    vo = [x for x in plan['items'] if x['kind']=='clip' and x['track']=='A1' and x['source']['path']==VOICE]
    if len(vo) != 1:
        raise ValueError('Expected one Star 4 voice item')
    lo = vo[0]['start_frame'] + start_frame
    hi = lo + length*FPS
    selected = sorted([x for x in plan['items'] if x['kind']=='clip' and x['track']=='V2'
                       and x['start_frame'] < hi and x['start_frame']+x['duration_frames'] > lo], key=lambda x:x['start_frame'])
    if not selected:
        raise ValueError('Saved plan has no picture for the sample')
    out.mkdir(parents=True)
    assets=out/'assets';assets.mkdir()
    work=out/'work';work.mkdir()
    ledger=[]; pieces=[]; cursor=lo

    def piece(source, source_frame, frames, hold, label):
        dest=work/f'{len(pieces):03d}.mp4'
        args=['ffmpeg','-hide_banner','-loglevel','error','-nostdin','-n']
        if source is None:
            args += ['-f','lavfi','-i','color=c=0x11151e:s=1920x1080:r=30']
        else:
            args += ['-ss',f'{source_frame/FPS:.6f}','-i',source]
        args += ['-an','-vf',picture_filter(frames,hold),
                 '-frames:v',str(frames),'-c:v','libx264','-preset','veryfast','-crf','20','-pix_fmt','yuv420p',dest]
        run(args)
        actual = next(s for s in probe(dest)['streams'] if s['codec_type']=='video')
        if int(actual['nb_frames']) != frames:
            raise ValueError('Prepared segment does not match its requested frame count')
        pieces.append(dest)
        ledger.append(dict(file=str(source) if source else None,source_in_frame=source_frame,frames=frames,planned_hold_frames=hold,cue_id=label))

    for n,item in enumerate(selected):
        a=max(lo,item['start_frame']); b=min(hi,item['start_frame']+item['duration_frames'])
        if a < cursor:
            raise ValueError('Overlapping picture items in saved plan')
        if a > cursor:
            piece(None,0,a-cursor,0,'unresolved picture gap')
        next_start=selected[n+1]['start_frame'] if n+1<len(selected) else hi
        hold=max(0,min(next_start,hi)-b)
        # shortcut: tiny assembly gaps become documented holds, revisit with more source handles for action-critical footage.
        if hold>15: hold=0
        src=project/item['source']['path']
        piece(src,item['source']['in_frame']+a-item['start_frame'],b-a+hold,hold,item['cue_id'])
        cursor=b+hold
    if cursor<hi:piece(None,0,hi-cursor,0,'unresolved picture gap')
    listing=work/'concat.txt'
    listing.write_text('\n'.join("file '"+str(p).replace('\\','/').replace("'","'\\''")+"'" for p in pieces),encoding='utf-8')
    run(['ffmpeg','-hide_banner','-loglevel','error','-nostdin','-n','-f','concat','-safe','0','-i',listing,'-c','copy',assets/'baseline.mp4'])
    baseline = next(s for s in probe(assets/'baseline.mp4')['streams'] if s['codec_type']=='video')
    if int(baseline['nb_frames']) != length*FPS:
        raise ValueError('Prepared baseline does not cover the requested duration')
    run(['ffmpeg','-hide_banner','-loglevel','error','-nostdin','-n','-ss',f'{start_frame/FPS:.6f}','-i',project/VOICE,'-t','90','-ar',str(RATE),'-c:a','pcm_s16le',assets/'voice.wav'])
    run(['ffmpeg','-hide_banner','-loglevel','error','-nostdin','-n','-i',project/'Music/bed-tension-soundride.mp3','-t','90','-af','loudnorm=I=-20:TP=-3:LRA=7,afade=t=in:d=1,afade=t=out:st=88:d=2','-ar',str(RATE),assets/'music.wav'])
    palette=[sound(assets/f'{kind}-{i}.wav',kind,i+1) for kind in ['pop','whoosh','impact'] for i in range(2)]

    def at(quote):return round(anchor(words,quote)*FPS)-start_frame

    scenes=[
      dict(id='fight-vs-pursuit',kind='comparison',start=at('He wins the fight')-15,end=at('Compared with three'),title='WINNING THE FIGHT IS ONE PART',
           items=[dict(text='Fight won',at=at('He wins the fight')),dict(text='Still wanted',at=at('he is still wanted'))]),
      dict(id='cost-of-leaving',kind='progressive',start=at('Now the ground fight'),end=at('In five'),title='WHAT THE NEXT ESCAPE COSTS',
           items=[dict(text='Time',at=at('the time ammunition')),dict(text='Ammunition',at=at('ammunition and strength')),dict(text='Strength',at=at('strength he needed'))]),
      dict(id='context-matters',kind='source-card',start=at('He gets pauses'),end=at("Zero's fight"),title='CONTEXT MATTERS',sourceLabel='John Wick · Narration summary',
           items=[dict(text='Pauses, help and changes of situation',at=at('He gets pauses'))]),
    ]
    sounds=[]
    for scene in scenes:
        cues=[('whoosh',scene['start']+8,-16)]
        cues += [('impact' if scene['kind']=='comparison' and i==1 else 'pop',item['at']+5,-12 if scene['kind']=='comparison' and i==1 else -17) for i,item in enumerate(scene['items'])]
        for kind,target,db in cues:
            s=next(p for p in palette if p['kind']==kind)
            sounds.append(dict(sceneId=scene['id'],file=s['file'],target=target,transient=s['transient'],duration=s['duration'],db=db))
    captions=[];group=[]
    for text,a,b in words:
        a,b=round(float(a)*FPS)-start_frame,round(float(b)*FPS)-start_frame
        if a<0 or b>length*FPS:continue
        if group and (len(' '.join([w[0] for w in group]+[text]))>32 or len(group)>=6 or a-group[-1][2]>12):
            captions.append(dict(start=group[0][1],end=group[-1][2],text=' '.join(w[0] for w in group)));group=[]
        group.append((text,a,b))
    if group:captions.append(dict(start=group[0][1],end=group[-1][2],text=' '.join(w[0] for w in group)))
    pilot=dict(version=1,fps=FPS,duration=length*FPS,baseline='baseline.mp4',voice='voice.wav',music='music.wav',variant='restrained',scenes=scenes,captions=captions,sounds=sounds)
    save(out/'pilot.json',pilot)
    save(out/'sound-palette.json',palette)
    report=dict(source_project=str(project),source_plan=str(plan_path),source_plan_hash=sha(plan_path),source_hashes=hashes,
                voice_file=VOICE,voice_start_frame=start_frame,sequence_start_frame=lo,duration_frames=length*FPS,
                picture=ledger,padding_policy='Trim each shot to its planned end, then repeat its last frame for planned gaps or fractional frame-rate shortages; every output frame count is verified',
                caption_source='cached ASR observations; captions and word boundaries require review',
                sound_source='original procedural audition, not Stable Audio; pending listening approval',
                source_card='Narration context only; not independent film evidence',clef='shadow packets only; no local endpoint was assumed')
    if any(sha(project/p)!=h for p,h in hashes.items()):raise RuntimeError('Source changed during preparation')
    save(out/'prepared.json',report)
    print('Prepared',out,flush=True)


if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--project',required=True,type=Path)
    parser.add_argument('--out',required=True,type=Path)
    args=parser.parse_args();prepare(args.project,args.out)
