// SPDX-License-Identifier: MIT OR Apache-2.0
import React from 'react';
import {AbsoluteFill, Composition, registerRoot, staticFile} from 'remotion';
import {Video} from '@remotion/media';
import {Treatment} from './Root.tsx';
import {recipePilot,type InsertProps} from './recipe.ts';

const Insert:React.FC<InsertProps>=(p)=>{
  const pilot=recipePilot(p),scene=pilot.scenes[0];
  return <AbsoluteFill>
    <Video src={staticFile(p.background)} muted style={{width:'100%',height:'100%',objectFit:'contain'}}/>
    <div style={{position:'absolute',width:1920,height:1080,transform:`scale(${p.width/1920},${p.height/1080})`,transformOrigin:'top left'}}>
      <Treatment scene={scene} pilot={pilot}/>
    </div>
  </AbsoluteFill>;
};
const defaults:InsertProps={width:1920,height:1080,fps:30,duration:90,background:'background.mp4',kind:'progressive',labels:[' '],reveals:[0],motion:'fade'};
registerRoot(()=> <Composition id="RecipeInsert" component={Insert} width={1920} height={1080} fps={30} durationInFrames={90}
  defaultProps={defaults} calculateMetadata={({props})=>({width:props.width,height:props.height,fps:props.fps,durationInFrames:props.duration})}/>);
