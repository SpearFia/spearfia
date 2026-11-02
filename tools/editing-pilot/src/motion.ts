// SPDX-License-Identifier: MIT OR Apache-2.0
import {interpolate, spring} from 'remotion';

export const motionPresets = {
  'slide-fade': {label:'Slide, fade and settle', sound:'whoosh', transientSeconds:0.27},
  pop: {label:'Small scale entrance', sound:'pop', transientSeconds:0.17},
  impact: {label:'Emphasis hit and short settle', sound:'impact',transientSeconds:0.17},
  fade: {label:'Quiet fade', sound:'none', transientSeconds:0.17},
} as const;
export type MotionPreset = keyof typeof motionPresets;

// Independent CSS transform properties keep combined slide/scale operations intact.
export function motionAt(preset: MotionPreset, frame: number, duration: number, fps: number, strength=1) {
  const fade=Math.min(Math.round(.23*fps),Math.floor(duration/3));
  if (frame < 0 || frame >= duration || duration < 3 || fade < 1) return {opacity:0,translate:'0px 0px',scale:1,rotate:'0deg'};
  const opacity=interpolate(frame,[0,fade,duration-fade,duration],[0,1,1,0],{extrapolateLeft:'clamp',extrapolateRight:'clamp'});
  const settle=spring({frame,fps,config:{damping:strength>1?17:25,stiffness:180,mass:.65}});
  return {opacity,translate:`0px ${preset==='slide-fade'?(1-settle)*46*strength:0}px`,
    scale:preset==='pop'||preset==='impact'?1-(1-settle)*.09*strength:1,
    rotate:`${preset==='impact'&&frame<fps*.4?Math.sin(frame/fps*24)*Math.exp(-frame/fps*12)*1.1*strength:0}deg`};
}
