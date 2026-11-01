// SPDX-License-Identifier: MIT OR Apache-2.0
import React,{useMemo} from 'react';
import {interpolate,useCurrentFrame,useVideoConfig} from 'remotion';
import {measureText} from '@remotion/layout-utils';
import type {Pilot} from './model.ts';

export const WordCaptions:React.FC<{pilot:Pilot}>=({pilot:p})=>{
  const frame=useCurrentFrame(),{fps}=useVideoConfig(),ms=frame/fps*1000;
  const byId=useMemo(()=>new Map(p.wordCaptions.map(w=>[w.id,w])),[p.wordCaptions]);
  const page=p.captionPages.find(c=>c.startMs<=ms&&c.endMs>ms);
  if(!page) return null;
  const words=page.wordIds.map(id=>byId.get(id)!);
  const pr=p.profile;
  const measured=measureText({text:page.text,fontFamily:pr.captionFont,fontWeight:'700',fontSize:pr.captionSize});
  if(measured.width>pr.captionWidth+1) throw new Error('Caption exceeds the measured safe width; revise its page breaks');
  const enter=interpolate(ms-page.startMs,[0,100],[0,1],{extrapolateLeft:'clamp',extrapolateRight:'clamp'});
  return <div style={{position:'absolute',bottom:pr.captionBottom,left:(1920-pr.captionWidth)/2,width:pr.captionWidth,textAlign:'center',
    fontFamily:pr.captionFont,fontSize:pr.captionSize,fontWeight:700,lineHeight:1.25,color:pr.ink}}>
    <div style={{display:'inline-block',padding:'10px 24px',borderRadius:12,background:'rgba(5,8,14,.84)',
      boxShadow:'0 3px 16px rgba(0,0,0,.25)',whiteSpace:'pre',scale:p.captionStyle==='emphasis'?.98+.02*enter:1}}>
      {words.map((w,i)=>{
        const active=w.timing==='matched-observation'&&w.startMs<=ms&&w.endMs>ms;
        const accented=p.captionStyle==='active-word'?active:p.captionStyle==='emphasis'&&w.emphasis;
        return <React.Fragment key={w.id}>{i>0?' ':''}<span style={{color:accented?pr.secondary:pr.ink,
          textDecoration:p.captionStyle==='emphasis'&&w.emphasis&&active?'underline':'none',textUnderlineOffset:8}}>{w.text}</span></React.Fragment>;
      })}
    </div>
  </div>;
};
