// SPDX-License-Identifier: MIT OR Apache-2.0
import {test} from 'node:test';
import assert from 'node:assert/strict';
import {reconcileWords,planCaptionPages,type CaptionWord} from '../src/captions.ts';
import {PilotSchema} from '../src/model.ts';

const words=(texts:string[]):CaptionWord[]=>texts.map((text,id)=>({id,text,startMs:id*300,endMs:id*300+240,timestampMs:null,confidence:null,timing:'matched-observation',emphasis:false}));
test('caption spelling comes from the approved script and interpolation stays visible',()=>{
  const w=reconcileWords([{id:1,line:1,text:'Grassrivers',startFrame:30,endFrame:45},{id:2,line:1,text:'city.',startFrame:45,endFrame:60}],
    [['grass',1,1.2],['rivers',1.2,1.4],['city.',1.5,1.7]],0,90,30);
  assert.equal(w[0].text,'Grassrivers');assert.equal(w[0].timing,'interpolated');
  assert.equal(w[1].timing,'matched-observation');assert.equal(w[1].endMs,1700);
  assert.throws(()=>reconcileWords([{id:1,line:1,text:'lost',startFrame:30,endFrame:30}],[],0,90,30));
});
test('page breaks respect punctuation, actual width, silence, names and negations',()=>{
  const ws=words(['We','need','John','Wick','not','tomorrow.','Next','point.']);
  const pages=planCaptionPages(ws,t=>t.length,18,['John Wick']);
  assert.equal(pages.map(p=>p.text).join(' '),ws.map(w=>w.text).join(' '));
  assert.ok(pages.some(p=>p.text.includes('John Wick')));
  assert.ok(pages.some(p=>p.text.includes('not tomorrow.')));
  assert.ok(pages.every(p=>!p.text.endsWith('John')&&!p.text.endsWith('not')));
  assert.ok(pages.every(p=>p.text.length<=18));
  const paused=words(['Before','after']);paused[1].startMs=2000;paused[1].endMs=2240;
  assert.ok(planCaptionPages(paused,t=>t.length,50)[0].endMs<paused[1].startMs);
  assert.throws(()=>planCaptionPages(words(['ImpossibleLongWord']),t=>t.length,3));
  assert.equal(planCaptionPages(words(['has','to','spend','to','reach','the','next','gap.']),t=>t.length,100).length,1);
  assert.ok(planCaptionPages(words(['We',"don't",'leave.']),t=>t.length,13).some(p=>p.text==="don't leave."));
});
test('caption pages cannot omit, duplicate or change source words',()=>{
  const ws=words(['One','two.']),pages=planCaptionPages(ws,t=>t.length,50);
  const p={version:1,fps:30,duration:300,baseline:'base.mp4',voice:'voice.wav',variant:'restrained',scenes:[],captions:[],sounds:[],wordCaptions:ws,captionPages:pages};
  assert.ok(PilotSchema.safeParse(p).success);
  assert.equal(PilotSchema.safeParse({...p,captionPages:[{...pages[0],text:'Changed'}]}).success,false);
  assert.equal(PilotSchema.safeParse({...p,captionPages:[]}).success,false);
  assert.equal(PilotSchema.safeParse({...p,captionPages:[{...pages[0],wordIds:[]}]}).success,false);
  assert.equal(PilotSchema.safeParse({...p,captionPages:[...pages,...pages]}).success,false);
});
