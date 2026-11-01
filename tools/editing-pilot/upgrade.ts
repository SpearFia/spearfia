// SPDX-License-Identifier: MIT OR Apache-2.0
// Extend a prepared audition without changing it or its production inputs.
import {readFileSync,writeFileSync,mkdirSync,cpSync,existsSync,realpathSync} from 'node:fs';
import {resolve,join,dirname,relative,isAbsolute} from 'node:path';
import {fileURLToPath} from 'node:url';
import {createHash} from 'node:crypto';
import {spawnSync} from 'node:child_process';
import {parseArgs} from 'node:util';
import {z} from 'zod';
import {openBrowser} from '@remotion/renderer';
import {serializeSrt} from '@remotion/captions';
import {PilotSchema} from './src/model.ts';
import {editorialProfile,ProfileSchema} from './src/profile.ts';
import {reconcileWords,planCaptionPages,type AlignedWord} from './src/captions.ts';
import {motionPresets} from './src/motion.ts';

const {values}=parseArgs({options:{from:{type:'string'},out:{type:'string'},profile:{type:'string'},'caption-rules':{type:'string'}}});
if(!values.from||!values.out) throw new Error('Usage: node --experimental-strip-types upgrade.ts --from <prepared audition> --out <fresh external folder> [--profile profile.json]');
const root=dirname(fileURLToPath(import.meta.url)),repo=resolve(root,'../..'),source=realpathSync(values.from),requested=resolve(values.out);
let ancestor=requested;
while(!existsSync(ancestor)) {
  const parent=dirname(ancestor);
  if(parent===ancestor) throw new Error('Output drive or root is unavailable');
  ancestor=parent;
}
const out=resolve(realpathSync(ancestor),relative(ancestor,requested));
if(existsSync(out)) throw new Error('Use a new output folder; previous auditions are preserved');
for(let dir=out;;dir=dirname(dir)) {
  if(existsSync(join(dir,'.git'))) throw new Error('Audition must stay outside Git');
  if(dirname(dir)===dir) break;
}
const read=(path:string)=>JSON.parse(readFileSync(path,'utf8').replace(/^\uFEFF/,''));
const digest=(path:string)=>createHash('sha256').update(readFileSync(path)).digest('hex');
const save=(name:string,value:unknown)=>writeFileSync(join(out,name),JSON.stringify(value,null,2)+'\n');
const prepared=read(join(source,'prepared.json')),project=realpathSync(prepared.source_project);
const inside=(base:string,path:string)=>{const r=relative(base,path);return !r||(!r.startsWith('..')&&!isAbsolute(r));};
if(inside(project,out)||inside(source,out)) throw new Error('Output must be separate from the source project and audition');
const protectedFiles=['project.json','links.json','script/script.md','visuals/visuals.md','channel.md'];
if(Object.keys(prepared.source_hashes).some(name=>!protectedFiles.includes(name))) throw new Error('Unknown protected input');
for(const name of protectedFiles) if(digest(join(project,name))!==prepared.source_hashes[name]) throw new Error('Production inputs changed; prepare a new audition');
const p=PilotSchema.parse(read(join(source,'pilot.json')));
const profile=values.profile?ProfileSchema.parse(read(resolve(values.profile))):editorialProfile;
const rules=z.object({protectedPhrases:z.array(z.string().min(1).max(120)).max(100).default([]),
  emphasis:z.array(z.string().min(1).max(80)).max(100).optional()}).strict().parse(values['caption-rules']?read(resolve(values['caption-rules'])):{});
const voicePath=prepared.voice_file as string,wordCache=read(join(project,'media/vo-words.json'))[voicePath];
if(digest(join(project,voicePath))!==wordCache.sha256) throw new Error('Cached voice observations are stale');
const exported=spawnSync('cargo',['run','--quiet','--release','-p','spearfia-sync','--example','editing_words','--',project,voicePath],{cwd:repo,encoding:'utf8',maxBuffer:8*1024*1024});
if(exported.status!==0) throw new Error(exported.stderr);
const aligned=JSON.parse(exported.stdout) as {fps:number;method:string;words:AlignedWord[]};
if(aligned.fps!==p.fps) throw new Error('Voice alignment and audition frame rates differ');
// These emphasis choices belong to this audition, rather than to the caption engine.
const emphasis=rules.emphasis??p.scenes.flatMap(s=>s.items.map(i=>i.text)).flatMap(t=>t.split(/\s+/));
const words=reconcileWords(aligned.words,wordCache.words,prepared.voice_start_frame,p.duration,p.fps,emphasis);
if(!words.length) throw new Error('No script-aligned words in the selected voice range');
const candidates=new Set<string>();
for(let i=0;i<words.length;i++) for(let n=1;n<=12&&i+n<=words.length;n++) candidates.add(words.slice(i,i+n).map(w=>w.text).join(' '));
const browser=await openBrowser('chrome',{logLevel:'error'});
let widths:Record<string,number>;
try {
  const page=await browser.newPage({context:()=>null,logLevel:'error',indent:false,pageIndex:0,onBrowserLog:null,onLog:()=>{}});
  widths=await page.evaluate((texts:string[],font:string)=>{
    const canvas=document.createElement('canvas'),ctx=canvas.getContext('2d')!;
    ctx.font=font;
    return Object.fromEntries(texts.map(t=>[t,ctx.measureText(t).width]));
  },[...candidates],`700 ${profile.captionSize}px ${profile.captionFont}`);
} finally {await browser.close({silent:true});}
const pages=planCaptionPages(words,t=>widths[t]??Infinity,profile.captionWidth,rules.protectedPhrases);
pages.forEach(page=>{page.endMs=Math.min(page.endMs,p.duration/p.fps*1000);});
p.profile=profile;p.captionStyle='active-word';p.wordCaptions=words;p.captionPages=pages;
p.captions=pages.map(page=>({start:Math.round(page.startMs/1000*p.fps),end:Math.min(p.duration,Math.round(page.endMs/1000*p.fps)),text:page.text}));
for(const s of p.scenes) {
  s.entrance=s.kind==='source-card'?'fade':'slide-fade';
  s.reveal=s.kind==='source-card'?'fade':'pop';
  if(s.kind==='comparison') s.items[1].motion='impact';
}
// Keep a single entrance sound when the context card's first item appears with it.
p.sounds=p.sounds.filter(s=>!(p.scenes.find(c=>c.id===s.sceneId)?.kind==='source-card'&&s.file.startsWith('pop')));
for(const event of p.sounds) {
  const scene=p.scenes.find(s=>s.id===event.sceneId)!;
  if(event.file.startsWith('whoosh')&&scene.entrance==='fade') continue;
  const preset=event.file.startsWith('whoosh')?scene.entrance:scene.reveal;
  const oldOffset=event.file.startsWith('whoosh')?8:5;
  event.target=event.target-oldOffset+Math.round(motionPresets[preset].transientSeconds*p.fps);
}
// A quiet fade has no movement requiring a whoosh.
p.sounds=p.sounds.filter(s=>!(s.file.startsWith('whoosh')&&p.scenes.find(c=>c.id===s.sceneId)?.entrance==='fade'));
PilotSchema.parse(p);
mkdirSync(out,{recursive:true});
for(const name of ['assets','work']) cpSync(join(source,name),join(out,name),{recursive:true,errorOnExist:true,force:false});
cpSync(join(source,'prepared.json'),join(out,'prepared.json'));
if(existsSync(join(source,'sound-palette.json'))) cpSync(join(source,'sound-palette.json'),join(out,'sound-palette.json'));
save('pilot.json',p);
writeFileSync(join(out,'captions.srt'),serializeSrt({lines:pages.map(page=>[{text:page.text,startMs:page.startMs,endMs:page.endMs,timestampMs:null,confidence:null}])}));
writeFileSync(join(out,'captions.vtt'),'WEBVTT\n\n'+readFileSync(join(out,'captions.srt'),'utf8').replace(/(\d\d:\d\d:\d\d),(\d{3})/g,'$1.$2'));
save('caption-plan.json',{method:aligned.method,source_voice:voicePath,voice_sha256:wordCache.sha256,script_sha256:prepared.source_hashes['script/script.md'],
  alignment:'Production LCS alignment, with matched ASR ends; not acoustic forced alignment',profile,rules,words,pages,
  uncertain_words:words.filter(w=>w.timing==='interpolated'),approval:'pending creator timing and style review'});
const visualText=readFileSync(join(project,'visuals/visuals.md'),'utf8');
const sections=visualText.split(/(?=^<!-- visual-cue )/m).slice(1).map(block=>{
  const end=block.indexOf(' -->'),meta=JSON.parse(block.slice('<!-- visual-cue '.length,end));
  return {meta,prose:block.slice(end+4).split(/\n## /)[0].slice(0,1500)};
});
let pictureFrame=0;
const footage=prepared.picture.map((clip:Record<string,unknown>,i:number)=>{
  const first=pictureFrame;pictureFrame+=Number(clip.frames);
  const asset=`work/${String(i).padStart(3,'0')}.mp4`;
  return {...clip,start:first,end:pictureFrame,preparedAsset:asset,sha256:digest(join(source,asset))};
});
save('editorial-context.json',p.scenes.map(scene=>{
  const local=words.filter(w=>w.startMs<scene.end/p.fps*1000&&w.endMs>scene.start/p.fps*1000);
  const lines=local.map(w=>aligned.words.find(a=>a.id===w.id)!.line);
  const matching=sections.filter(c=>lines.some(line=>c.meta.anchor.line<=line&&line<c.meta.anchor.line+c.meta.anchor.lines));
  return {id:scene.id,narration:local.map(w=>w.text).join(' '),visualPlan:matching.map(c=>({id:c.meta.id,details:c.meta.details,direction:c.prose})),
    footage:footage.filter((clip:{start:number;end:number})=>clip.start<scene.end+90&&clip.end>scene.start-90),
    visualEvidence:[],visualEvidenceStatus:'No matched VideoJev results attached to these source windows; not inspected by this worker',
    constraints:['Preserve approved narration','Reserve caption band','A narration summary is not independent evidence'],
    uncertainCaptionWords:local.filter(w=>w.timing==='interpolated').map(w=>w.id)};
}));
if(!protectedFiles.every(name=>digest(join(project,name))===prepared.source_hashes[name])) throw new Error('Production inputs changed during preparation; partial audition preserved for diagnosis');
save('upgrade.json',{from:source,project,word_count:words.length,page_count:pages.length,uncertain_words:words.filter(w=>w.timing==='interpolated').length,
  model_calls:0,source_unchanged:true,
  motion:'Original slide/fade/scale/stagger implementation inspired by the reviewed remotion-kit patterns; no unlicensed source copied',
  native_caption_presentation:'Clean editable phrases; animated word highlighting is available in the Remotion render',approval:'pending creator review'});
console.log('Prepared upgraded audition:',out);
