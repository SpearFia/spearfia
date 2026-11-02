# SPDX-License-Identifier: MIT OR Apache-2.0
"""Build a separate FilmCraft audition; never open or save the source project."""
import argparse
import hashlib
import json
import math
import subprocess
from pathlib import Path
from prepare import FPS, probe, read, run, save, sha

ROOT = Path(__file__).resolve().parent
CLI = Path(r'C:\Program Files\FilmCraft\filmcraft-cli.exe')
TICKS_PER_FRAME = 254016000000 // FPS


def command(command_id, **params):
    return dict(id=command_id, params=params)


def native_caption_groups(p):
    if not p.get('wordCaptions'):
        return p['captions']
    if not p.get('captionPages'):
        raise ValueError('Script words require a shared caption page plan')
    return [dict(start=math.floor(c['startMs']/1000*FPS+.5),
                 end=min(p['duration'],math.floor(c['endMs']/1000*FPS+.5)),text=c['text']) for c in p['captionPages']]


def sound_tracks(sounds):
    ends, scheduled = [], []
    for s in sorted(sounds, key=lambda s:s['target']-s['transient']):
        start = s['target']-s['transient']
        lane = next((i for i, end in enumerate(ends) if end <= start), len(ends))
        if lane == len(ends):
            ends.append(0)
        ends[lane] = start+s['duration']
        scheduled.append((s, f'A{lane+3}'))
    return scheduled


def cli(commands, destination, source=None):
    if destination.exists():
        raise ValueError('Existing FilmCraft files are never overwritten')
    args = [str(CLI), '--compact', '--save-as', str(destination)]
    if source:
        args += ['--project', str(source)]
    result = subprocess.run(args + ['run', '-'], input=''.join(json.dumps(c)+'\n' for c in commands),
                            capture_output=True, text=True, encoding='utf-8', timeout=180)
    destination.with_suffix('.commands.jsonl').write_text(result.stdout, encoding='utf-8')
    if result.returncode:
        raise RuntimeError(result.stderr + result.stdout)
    rows = [json.loads(line) for line in result.stdout.splitlines() if line.strip()]
    if len(rows) != len(commands) or any(not r.get('ok') for r in rows):
        raise RuntimeError('FilmCraft did not confirm every command')
    return [r['result'] for r in rows]


def export(folder):
    folder = folder.resolve()
    if any((p/'.git').exists() for p in [folder, *folder.parents]):
        raise ValueError('Audition must stay outside Git')
    p, prepared = read(folder/'pilot.json'), read(folder/'prepared.json')
    p['captions']=native_caption_groups(p)
    if p['fps'] != FPS or p['duration'] != prepared['duration_frames']:
        raise ValueError('Pilot and preparation differ')
    assets = (folder/'assets').resolve()

    def asset(name):
        path = (assets/name).resolve()
        if not path.is_relative_to(assets) or not path.is_file():
            raise ValueError('Asset is absent or escapes the prepared assets directory')
        return path

    voice, music = asset(p['voice']), asset(p['music']) if p.get('music') else None
    for sound in p['sounds']:
        asset(sound['file'])
    scheduled = sound_tracks(p['sounds'])
    picture = [folder/'work'/f'{i:03d}.mp4' for i in range(len(prepared['picture']))]
    if sum(s['frames'] for s in prepared['picture']) != p['duration'] or not all(path.is_file() for path in picture):
        raise ValueError('Prepared picture segments do not cover the audition')
    fingerprint = hashlib.sha256(json.dumps(p, sort_keys=True).encode())
    for path in [Path(__file__), *sorted(ROOT.glob('src/*.ts')), *sorted(ROOT.glob('src/*.tsx')), ROOT/'package-lock.json',
                 folder/'prepared.json', *picture, asset(p['baseline']), voice, *([music] if music else []), *sorted({asset(s['file']) for s in p['sounds']})]:
        fingerprint.update(sha(path).encode())
    work = folder/('filmcraft-'+fingerprint.hexdigest()[:16])
    verified = work/'verified.json'
    if verified.exists():
        record = read(verified)
        if sha(work/'editing-audition.fcproj') != record['sha256'] or any(sha(work/name) != digest for name, digest in record['inserts'].items()):
            raise ValueError('Saved audition changed; preserve it and prepare a fresh review folder')
        print('Cached verified FilmCraft audition:', record['project'], flush=True)
        return
    work.mkdir()  # Preserve any interrupted audition for inspection.
    props = work/'insert-props.json'
    save(props, dict(p, variant='restrained', captions=[], wordCaptions=[], captionPages=[], sounds=[]))
    inserts = []
    for scene in p['scenes']:
        path = work/(scene['id']+'.mp4')
        run(['node', ROOT/'node_modules/@remotion/cli/remotion-cli.js', 'render', ROOT/'src/index.ts', 'EditingPilot', path,
             '--props', props, '--public-dir', assets, '--frames', f"{scene['start']}-{scene['end']-1}",
             '--muted', '--codec', 'h264', '--crf', '19', '--concurrency', '2', '--log', 'error'])
        actual = probe(path)
        if abs(float(actual['format']['duration'])-(scene['end']-scene['start'])/FPS) > .08:
            raise ValueError('Insert duration differs from its frame range')
        if any(s['codec_type']=='audio' for s in actual['streams']):
            raise ValueError('Insert unexpectedly contains audio')
        inserts.append(path)
    paths = list(dict.fromkeys([*picture, *inserts, voice, *([music] if music else []), *[asset(s['file']) for s in p['sounds']]]))
    imported = work/'01-imported.fcproj'
    results = cli([
        command('file.newSequence', name='Spear Fia - editing treatment audition', width=1920, height=1080, fps=FPS, video=3,
                audio=max([3, *[int(track[1:]) for _, track in scheduled]])),
        command('file.import', paths=[str(path) for path in paths]),
    ], imported)
    if results[1].get('errors') or len(results[1]['items']) != len(paths):
        raise ValueError('FilmCraft did not import every audition asset')
    ids = dict(zip(paths, results[1]['items']))
    placements, names, gains = [], [], []

    def place(path, track, frame, frames, name, db=None):
        placements.append(command('timeline.place', item=ids[path], track=track, frame=frame,
                                  duration=frames*TICKS_PER_FRAME, video=track.startswith('V'), audio=track.startswith('A')))
        names.append(name)
        gains.append(db)

    cursor = 0
    for path, segment in zip(picture, prepared['picture']):
        place(path, 'V1', cursor, segment['frames'], f"{segment['cue_id']} - prepared source segment")
        cursor += segment['frames']
    for path, scene in zip(inserts, p['scenes']):
        place(path, 'V2', scene['start'], scene['end']-scene['start'], scene['id']+' - Remotion insert')
    place(voice, 'A1', 0, p['duration'], 'Approved narration - Star 4 excerpt')
    if music:
        place(music, 'A2', 0, p['duration'], 'Existing project music - review level', 20*math.log10(.065))
    for s, track in scheduled:
        place(asset(s['file']), track, s['target']-s['transient'], s['duration'], s['sceneId']+' - '+s['file'], s['db']+20*math.log10(.62))
    placed = work/'02-placed.fcproj'
    results = cli(placements, placed, imported)
    edits = []
    for result, name, db in zip(results, names, gains):
        clips = result['clips']
        if len(clips) != 1:
            raise ValueError('Expected one clip per placement')
        edits.append(command('clip.rename', clip=clips[0], name=name))
        if db is not None:
            edits.append(command('clip.audioGain', clips=clips, mode='set', db=db))
    profile=p.get('profile',{})
    edits += [command('captions.newTrack', format='Subtitle', name='Script-aligned captions - timing review' if p.get('wordCaptions') else 'Draft ASR captions - review wording', language='en'),
              command('captions.setStyle', track='C1', font=profile.get('captionFont','Arial'), size=profile.get('captionSize',48), color=profile.get('ink','#FFFFFF'), outline=1,
                      outlineColor='#111111', background=True, backgroundColor='#000000D6', anchor='bottom', margin=profile.get('captionBottom',65)/1080)]
    edits += [command('captions.add', track='C1', text=c['text'], time=c['start']*TICKS_PER_FRAME, durationSeconds=(c['end']-c['start'])/FPS) for c in p['captions']]
    edits += [command('markers.addRange', time=s['start']*TICKS_PER_FRAME, durationFrames=s['end']-s['start'], name=s['id'],
                      comment='Pending treatment review; graphics editable in pilot.json and Remotion.') for s in p['scenes']]
    edits.append(command('captions.export',track='C1',path=str(work/'captions.srt'),format='srt'))
    destination = work/'editing-audition.fcproj'
    cli(edits, destination, placed)
    result = subprocess.run([str(CLI), '--compact', '--project', str(destination), 'inspect', 'sequence'],
                            capture_output=True, text=True, encoding='utf-8', check=True, timeout=60)
    sequence = json.loads(result.stdout)
    save(work/'sequence-inspection.json', sequence)
    expected = dict(V1=len(picture), V2=len(inserts), A1=1, A2=int(bool(music)))
    for _, track in scheduled:
        expected[track] = expected.get(track, 0)+1
    for kind, tracks in [('V', sequence['video']), ('A', sequence['audio'])]:
        for i, track in enumerate(tracks, 1):
            if len(track['items']) != expected.get(f'{kind}{i}', 0):
                raise ValueError('Saved FilmCraft track layout differs from the audition')
    saved = {item['clip']:item for track in sequence['video']+sequence['audio'] for item in track['items']}
    for result, placement, db in zip(results, placements, gains):
        item, spec = saved[result['clips'][0]], placement['params']
        if item['startFrame'] != spec['frame'] or item['duration'] != spec['duration'] or abs(item['gainDb']-(db or 0)) > .001:
            raise ValueError('Saved clip timing or gain differs from the audition')
    if sequence['durationFrames'] != p['duration']:
        raise ValueError('Saved FilmCraft duration differs from the audition')
    result = subprocess.run([str(CLI), '--compact', '--project', str(destination), 'exec', 'captions.list'],
                            capture_output=True, text=True, encoding='utf-8', check=True, timeout=60)
    caption_tracks=json.loads(result.stdout)
    save(work/'caption-inspection.json',caption_tracks)
    saved_captions=caption_tracks['tracks'][0]['captions']
    if len(saved_captions)!=len(p['captions']):
        raise ValueError('Saved caption count differs from the shared plan')
    for actual, expected_caption in zip(saved_captions,p['captions']):
        if actual['text']!=expected_caption['text'] or actual['start']!=expected_caption['start']*TICKS_PER_FRAME or actual['end']!=expected_caption['end']*TICKS_PER_FRAME:
            raise ValueError('Saved caption text/timing differs from the shared plan')
    save(work/'verified.json', dict(project=str(destination), sha256=sha(destination), inserts={path.name:sha(path) for path in inserts}, duration_frames=sequence['durationFrames'], tracks=expected,
         approval='pending creator review', notes=['Prepared shot segments include documented holds; original source handles remain in prepared.json',
           'Remotion inserts are flattened picture layers; adjust labels and animation in pilot.json then rerender',
           'Captions, voice, music and each sound cue remain separate; sound palette is an unapproved procedural audition',
           'Native captions use clean editable phrases from the shared plan; animated word highlighting is available in Remotion reviews']))
    print('Verified separate FilmCraft audition:', destination, flush=True)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--folder', required=True, type=Path)
    export(parser.parse_args().folder)
