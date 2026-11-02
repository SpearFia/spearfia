// SPDX-License-Identifier: MIT OR Apache-2.0
import {test} from 'node:test';
import assert from 'node:assert/strict';
import {recipePilot,type InsertProps} from '../src/recipe.ts';

const props:InsertProps={width:3840,height:2160,fps:30,duration:150,background:'background.mp4',
  kind:'source-card',labels:['7 ROUNDS'],sourceLabel:'CHAPTER 2: BOWERY GIFT',reveals:[15],motion:'pop'};
test('source/context lines stay separate from body items and require explicit mapping',()=>{
  const pilot=recipePilot(props),s=pilot.scenes[0];
  assert.deepEqual(s.items.map(i=>i.text),['7 ROUNDS']);assert.equal(s.sourceLabel,props.sourceLabel);
  assert.deepEqual(pilot.captions,[]);assert.deepEqual(pilot.sounds,[]);
  assert.throws(()=>recipePilot({...props,sourceLabel:undefined}));
  assert.throws(()=>recipePilot({...props,reveals:[]}));
});
test('a comparison qualifier is never a compared statement',()=>{
  const s=recipePilot({...props,kind:'comparison',labels:['FIGHT WON','STILL WANTED'],reveals:[15,60],qualifier:'OUR GUESS'}).scenes[0];
  assert.equal(s.eyebrow,'OUR GUESS');assert.equal(s.sourceLabel,undefined);assert.equal(s.items.length,2);
  assert.throws(()=>recipePilot({...props,kind:'comparison',labels:['ROAD','BUILDING','SWAMP'],reveals:[15,45,75]}));
});
