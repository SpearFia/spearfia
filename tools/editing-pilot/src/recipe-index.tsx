// SPDX-License-Identifier: MIT OR Apache-2.0
import React from 'react';
import {AbsoluteFill, Composition, registerRoot, staticFile} from 'remotion';
import {Video} from '@remotion/media';
import {Treatment} from './Root.tsx';
import {PilotSchema, SceneSchema} from './model.ts';

type Props={width:number;height:number;fps:number;duration:number;background:string;kind:'progressive'|'comparison'|'source-card';labels:string[];reveals:number[];motion:'slide-fade'|'pop'|'fade'|'impact';title?:string;sourceLabel?:string};
const Insert:React.FC<Props>=(p)=>{
  const scene=SceneSchema.parse({id:'recipe',kind:p.kind,start:0,end:p.duration,title:p.title??' ',
    items:p.labels.map((text,i)=>({text,at:p.reveals[i]})),entrance:p.motion,reveal:p.motion,
    ...(p.kind==='source-card'?{sourceLabel:p.sourceLabel??p.labels.at(-1)}:{})});
  const pilot=PilotSchema.parse({version:1,fps:30,duration:p.duration,baseline:p.background,voice:p.background,
    variant:'restrained',scenes:[scene],captions:[],sounds:[]});
  return <AbsoluteFill>
    <Video src={staticFile(p.background)} muted style={{width:'100%',height:'100%',objectFit:'contain'}}/>
    <div style={{position:'absolute',width:1920,height:1080,transform:`scale(${p.width/1920},${p.height/1080})`,transformOrigin:'top left'}}>
      <Treatment scene={scene} pilot={pilot}/>
    </div>
  </AbsoluteFill>;
};
const defaults:Props={width:1920,height:1080,fps:30,duration:90,background:'background.mp4',kind:'progressive',labels:[' '],reveals:[0],motion:'fade'};
registerRoot(()=> <Composition id="RecipeInsert" component={Insert} width={1920} height={1080} fps={30} durationInFrames={90}
  defaultProps={defaults} calculateMetadata={({props})=>({width:props.width,height:props.height,fps:props.fps,durationInFrames:props.duration})}/>);
