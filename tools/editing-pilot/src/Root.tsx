// SPDX-License-Identifier: MIT OR Apache-2.0
import React from 'react';
import {AbsoluteFill, Composition, Sequence, interpolate, spring, staticFile, useCurrentFrame} from 'remotion';
import {Audio, Video} from '@remotion/media';
import {PilotSchema, type Pilot, type Scene} from './model.ts';

const ink = '#F4F0E8', red = '#E34747', gold = '#E3BB78';
const headline = 'Impact, Arial Black, sans-serif';
const body = 'Arial, sans-serif';
const ramp = (f: number, a: number, b: number, x = 0, y = 1) => interpolate(f, [a, b], [x, y], {extrapolateLeft: 'clamp', extrapolateRight: 'clamp'});

const Treatment: React.FC<{scene: Scene; expressive: boolean}> = ({scene, expressive}) => {
  const f = useCurrentFrame();
  const enter = spring({frame: f - scene.start, fps: 30, config: {damping: expressive ? 16 : 26, stiffness: 110}});
  const opacity = ramp(f, scene.start, scene.start + 8) * ramp(f, scene.end - 8, scene.end, 1, 0);
  const source = scene.kind === 'source-card';
  return <AbsoluteFill style={{opacity, background: 'linear-gradient(140deg,rgba(6,10,17,.96),rgba(21,24,33,.97))', color: ink, fontFamily: body, padding: '96px 130px 190px'}}>
    <div style={{height: 6, width: 84, background: red, marginBottom: 24}} />
    <div style={{fontSize: 22, letterSpacing: 5, color: gold, marginBottom: 16}}>{source ? 'NARRATION CONTEXT' : 'FOUR STARS · THE DISTINCTION'}</div>
    <div style={{fontFamily: headline, fontSize: 76, lineHeight: 1.08, maxWidth: 1550, translate: `0px ${(1-enter)*25}px`}}>{scene.title}</div>
    <div style={{display: 'flex', flexDirection: scene.kind === 'comparison' ? 'row' : 'column', gap: 22, marginTop: 48}}>
      {scene.items.map((item, i) => {
        const reveal = spring({frame: f-item.at, fps: 30, config: {damping: expressive ? 16 : 26, stiffness: 120}});
        const arrived = f >= item.at;
        return <div key={i} style={{flex: scene.kind === 'comparison' ? 1 : undefined, minHeight: scene.kind === 'comparison' ? 205 : 86,
          opacity: ramp(f,item.at,item.at+7) * (scene.kind === 'comparison' && i === 0 ? ramp(f,scene.items[1].at,scene.items[1].at+10,1,0.62) : 1), translate: `0px ${(1-reveal)*24}px`,
          border: `1px solid ${i === scene.items.length-1 ? red : '#454955'}`, borderLeft: `6px solid ${i === scene.items.length-1 ? red : gold}`,
          borderRadius: 14, padding: '24px 30px', background: arrived ? '#171B24' : 'transparent',
          display: 'flex', alignItems: 'center', gap: 25}}>
          {!source && <span style={{fontSize: 24, color: gold, fontWeight: 700}}>0{i+1}</span>}
          <span style={{fontSize: source ? 48 : 46, lineHeight: 1.25, fontWeight: 600}}>{item.text}</span>
        </div>;
      })}
    </div>
    {source && <div style={{marginTop: 22, color: '#B8BEC9', fontSize: 24}}>{scene.sourceLabel}</div>}
  </AbsoluteFill>;
};

export const PilotVideo: React.FC<Pilot> = (p) => {
  const frame = useCurrentFrame();
  const current = p.captions.find(c => c.start <= frame && c.end > frame);
  const designed = p.variant !== 'baseline';
  const soundGain = p.variant === 'expressive' ? 1 : 0.62;
  return <AbsoluteFill style={{background: '#080C13'}}>
    <Video src={staticFile(p.baseline)} muted durationInFrames={p.duration} premountFor={30} style={{width:'100%',height:'100%',objectFit:'contain'}} />
    {designed && p.scenes.map(s => <Sequence key={s.id} from={0} durationInFrames={s.end} premountFor={30} name={s.id}>
      <Treatment scene={s} expressive={p.variant === 'expressive'} />
    </Sequence>)}
    {current && <div style={{position:'absolute', bottom:66, left:150, right:150, textAlign:'center'}}>
      <span style={{fontFamily:headline,fontSize:48,lineHeight:1.3,color:'#FFF',WebkitTextStroke:'1px #111',textShadow:'0 3px 8px #000',background:'rgba(0,0,0,.73)',padding:'8px 20px',borderRadius:8}}>{current.text}</span>
    </div>}
    <Audio src={staticFile(p.voice)} durationInFrames={p.duration} premountFor={30} />
    {p.music && <Audio src={staticFile(p.music)} durationInFrames={p.duration} premountFor={30} volume={0.065} />}
    {designed && p.sounds.map((s,i) => <Audio key={i} src={staticFile(s.file)} from={s.target-s.transient} durationInFrames={s.duration} premountFor={30} volume={soundGain*Math.pow(10,s.db/20)} />)}
  </AbsoluteFill>;
};

// The sequence is intentionally data-controlled; individual edits live in pilot.json.
export const Root = () => <Composition id="EditingPilot" component={PilotVideo} width={1920} height={1080} fps={30} durationInFrames={2700}
  defaultProps={{version:1,fps:30,duration:2700,baseline:'baseline.mp4',voice:'voice.wav',variant:'restrained',scenes:[],captions:[],sounds:[]}}
  schema={PilotSchema} calculateMetadata={({props}) => {const p=PilotSchema.parse(props);return {durationInFrames:p.duration,props:p};}} />;
