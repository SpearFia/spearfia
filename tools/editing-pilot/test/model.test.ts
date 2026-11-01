// SPDX-License-Identifier: MIT OR Apache-2.0
import {test} from 'node:test';
import assert from 'node:assert/strict';
import {PilotSchema} from '../src/model.ts';
import {decisionPacket, assessChoice} from '../src/decisions.ts';
const scene = {id:'comparison',kind:'comparison' as const,start:30,end:180,title:'Different outcomes',items:[{text:'Fight won',at:45},{text:'Still wanted',at:90}]};
const pilot = {version:1,fps:30,duration:300,baseline:'base.mp4',voice:'voice.wav',variant:'restrained',scenes:[scene],captions:[],sounds:[{sceneId:'comparison',file:'pop.wav',target:90,transient:2,duration:10,db:-12}]};
test('unsafe assets, omitted provenance, clipping reveals and overlaps are refused', () => {
  assert.ok(PilotSchema.safeParse(pilot).success);
  for (const bad of [
    {...pilot,baseline:'../source.mp4'},
    {...pilot,scenes:[{...scene,kind:'source-card'}]},
    {...pilot,scenes:[scene,{...scene,id:'later',start:100}]},
    {...pilot,scenes:[{...scene,items:[{text:'A',at:179},{text:'B',at:180}]}]},
    {...pilot,sounds:[{...pilot.sounds[0],target:0,transient:10}]},
    {...pilot,sounds:[{...pilot.sounds[0],transient:20}]},
    {...pilot,captions:[{start:0,end:20,text:'First'},{start:10,end:30,text:'Second'}]},
  ]) assert.equal(PilotSchema.safeParse(bad).success,false);
});
test('decisions reject stale contexts, missing alternatives and invalid probability mass', () => {
  const p=decisionPacket(scene);
  assert.notEqual(p.hash,decisionPacket(scene,{narration:'Different surrounding beat',neighbors:[]}).hash);
  const probabilities={'comparison':0.96,'plain-hold':0.03,'none-of-these':0.01};
  assert.equal(assessChoice(p,{hash:p.hash,probabilities}).eligible,true);
  assert.equal(assessChoice(p,{hash:p.hash,probabilities}).applied,false);
  assert.throws(() => assessChoice(p,{hash:'old',probabilities}));
  assert.throws(() => assessChoice(p,{hash:p.hash,probabilities:{comparison:1}}));
  assert.throws(() => assessChoice(p,{hash:p.hash,probabilities:{...probabilities,comparison:0.8}}));
  assert.equal(assessChoice(p,{hash:p.hash,probabilities:{comparison:0.01,'plain-hold':0.01,'none-of-these':0.98}}).eligible,false);
});
