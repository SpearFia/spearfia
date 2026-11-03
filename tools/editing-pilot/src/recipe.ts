// SPDX-License-Identifier: MIT OR Apache-2.0
import {PilotSchema} from './model.ts';

export type InsertProps={width:number;height:number;fps:number;duration:number;background:string;
  kind:'progressive'|'comparison'|'source-card';labels:string[];reveals:number[];
  motion:'slide-fade'|'pop'|'fade'|'impact';sourceLabel?:string;qualifier?:string};

export function recipePilot(p:InsertProps) {
  if(p.labels.length!==p.reveals.length) throw new Error('Every body label needs a reveal anchor');
  return PilotSchema.parse({version:1,fps:p.fps,duration:p.duration,baseline:p.background,voice:p.background,
    variant:'restrained',scenes:[{id:'recipe',kind:p.kind,start:0,end:p.duration,title:' ',
      items:p.labels.map((text,i)=>({text,at:p.reveals[i]})),entrance:p.motion,reveal:p.motion,
      eyebrow:p.qualifier??'',...(p.kind==='source-card'?{sourceLabel:p.sourceLabel}:{})}],captions:[],sounds:[]});
}
