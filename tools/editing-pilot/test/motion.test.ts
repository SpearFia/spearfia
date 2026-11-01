// SPDX-License-Identifier: MIT OR Apache-2.0
import {test} from 'node:test';
import assert from 'node:assert/strict';
import {motionAt,motionPresets,type MotionPreset} from '../src/motion.ts';

test('motion presets have finite bounded entrances, holds and exits at varied frame rates',()=>{
  for(const name of Object.keys(motionPresets) as MotionPreset[]) for(const fps of [24,30,60]) {
    for(const frame of [-1,0,1,Math.round(fps/3),fps,3*fps-1,3*fps]) {
      const s=motionAt(name,frame,3*fps,fps);
      assert.ok(Number.isFinite(s.scale)&&s.scale>.7&&s.scale<1.2);
      assert.ok(s.opacity>=0&&s.opacity<=1);
      assert.ok(!s.translate.includes('NaN'));
    }
    assert.equal(motionAt(name,-1,90,fps).opacity,0);
    assert.equal(motionAt(name,90,90,fps).opacity,0);
  }
});
test('a one-frame item is safely hidden and slide/scale properties remain independent',()=>{
  assert.equal(motionAt('pop',0,1,30).opacity,0);
  const combined={...motionAt('slide-fade',2,90,30),scale:motionAt('pop',2,90,30).scale};
  assert.notEqual(combined.translate,'0px 0px');assert.notEqual(combined.scale,1);
});
