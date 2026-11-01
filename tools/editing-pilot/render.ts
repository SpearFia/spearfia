// SPDX-License-Identifier: MIT OR Apache-2.0
import {readFileSync, writeFileSync, existsSync, realpathSync} from 'node:fs';
import {resolve, dirname, relative, isAbsolute, join} from 'node:path';
import {fileURLToPath} from 'node:url';
import {createHash} from 'node:crypto';
import {spawnSync} from 'node:child_process';
import {parseArgs} from 'node:util';
import {PilotSchema} from './src/model.ts';
import {decisionPacket} from './src/decisions.ts';

const {values} = parseArgs({options:{folder:{type:'string'},variant:{type:'string',default:'restrained'},scale:{type:'string',default:'1'}}});
if (!values.folder) throw new Error('Usage: npm run render -- --folder <prepared folder> --variant baseline|restrained|expressive [--scale 0.5]');
const root=dirname(fileURLToPath(import.meta.url)), folder=resolve(values.folder), assets=realpathSync(join(folder,'assets'));
const p=PilotSchema.parse({...JSON.parse(readFileSync(join(folder,'pilot.json'),'utf8')),variant:values.variant});
const scale=Number(values.scale);
if (![0.5,1].includes(scale)) throw new Error('Scale must be 0.5 (audition) or 1 (1080p)');
const hash=createHash('sha256');
hash.update(JSON.stringify({p,scale}));
for(const name of ['src/Root.tsx','src/index.ts','src/model.ts','render.ts','package-lock.json']) hash.update(readFileSync(join(root,name)));
for(const asset of new Set([p.baseline,p.voice,...(p.music?[p.music]:[]),...p.sounds.map(s=>s.file)])) {
  const path=realpathSync(join(assets,asset)), rel=relative(assets,path);
  if (rel.startsWith('..') || isAbsolute(rel)) throw new Error('Asset escapes public directory');
  hash.update(asset);hash.update(readFileSync(path));
}
const key=hash.digest('hex').slice(0,16), output=join(folder,`${p.variant}-${scale===1?'1080p':'540p'}-${key}.mp4`);
const reportPath=output+'.json';
const digest=(path:string)=>createHash('sha256').update(readFileSync(path)).digest('hex');
writeFileSync(join(folder,'clef-packets.json'),JSON.stringify(p.scenes.map((s,i)=>decisionPacket(s,{
  narration:p.captions.filter(c=>c.start<s.end+90&&c.end>s.start-90).map(c=>c.text).join(' '),
  neighbors:p.scenes.filter((_,j)=>Math.abs(i-j)===1).map(n=>`${n.id}: ${n.kind}`),
})),null,2));
if(existsSync(output)) {
  if(!existsSync(reportPath)||JSON.parse(readFileSync(reportPath,'utf8')).sha256!==digest(output)) throw new Error('Existing render is unverified or changed; preserve it and use a new folder');
  console.log('Cached verified render:',output);process.exit(0);
}
const props=join(folder,`props-${p.variant}.json`);
writeFileSync(props,JSON.stringify(p,null,2));
const started=Date.now();
const rendered=spawnSync(process.execPath,[join(root,'node_modules/@remotion/cli/remotion-cli.js'),'render',join(root,'src/index.ts'),'EditingPilot',output,
  '--props',props,'--public-dir',assets,'--codec','h264','--crf','19','--concurrency','2','--scale',String(scale),'--log','error'],{stdio:'inherit',cwd:root});
if(rendered.status!==0) throw new Error(`Remotion failed (${rendered.status}); partial output is preserved for diagnosis`);
const measured=spawnSync('ffprobe',['-v','error','-show_format','-show_streams','-of','json',output],{encoding:'utf8'});
if(measured.status!==0) throw new Error(measured.stderr);
const info=JSON.parse(measured.stdout), video=info.streams.find((s:{codec_type:string})=>s.codec_type==='video');
if(!video||video.width!==1920*scale||video.height!==1080*scale||!info.streams.some((s:{codec_type:string})=>s.codec_type==='audio')||Math.abs(Number(info.format.duration)-p.duration/p.fps)>.15) throw new Error('Render failed media checks');
writeFileSync(reportPath,JSON.stringify({output,variant:p.variant,seconds:(Date.now()-started)/1000,sha256:digest(output),duration:info.format.duration,
  width:video.width,height:video.height,model_calls:0,clef_applied:false,approval:'pending creator review',
  limitations:['Procedural sound audition; listening approval required','Cached ASR caption draft','Narration card is not independent film evidence']},null,2));
console.log('Verified review:',output);
