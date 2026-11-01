// SPDX-License-Identifier: MIT OR Apache-2.0
import {test} from 'node:test';
import assert from 'node:assert/strict';
import {SceneSchema} from '../src/model.ts';
import {decisionPacket} from '../src/decisions.ts';
import {judge, judgeCached} from '../src/clef.ts';
import {mkdtemp, rm} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import {join} from 'node:path';

test('live judgments stay advisory and reject missing or changed context', async () => {
  const scene = SceneSchema.parse({id:'a',kind:'comparison',start:0,end:120,title:'A',items:[{text:'A',at:0},{text:'B',at:30}]});
  const packet = decisionPacket(scene,{narration:'A then B',neighbors:[],visualPlan:[{id:'cue'}],footage:[{id:'shot'}]});
  const send:typeof fetch = async () => new Response(JSON.stringify({model:'Cloudflare/clef-flash',answers:{treatment:{type:'choice',choice:'comparison',probabilities:{comparison:0.96,'plain-hold':0.03,'none-of-these':0.01}}}}));
  const result = await judge(packet,send);
  assert.equal(result.eligible,false);
  assert.equal(result.applied,false);
  await assert.rejects(() => judge({...packet,hash:'old'},send),/changed/);
  await assert.rejects(() => judge(decisionPacket(scene),send),/Incomplete/);
  await assert.rejects(() => judge(packet,async () => new Response('',{status:503})),/503/);
  const folder = await mkdtemp(join(tmpdir(),'clef-cache-'));
  try {
    await judgeCached(packet,folder,send);
    await judgeCached(packet,folder,async () => {throw new Error('Unexpected model call');});
    const changed = decisionPacket(scene,{...packet.context,narration:'Changed narration'});
    await assert.rejects(() => judgeCached(changed,folder,async () => {throw new Error('New model call');}),/New model call/);
  } finally { await rm(folder,{recursive:true,force:true}); }
});
