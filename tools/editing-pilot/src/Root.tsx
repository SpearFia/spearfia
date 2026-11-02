// SPDX-License-Identifier: MIT OR Apache-2.0
import React from 'react';
import {AbsoluteFill, Composition, Sequence, interpolate, staticFile, useCurrentFrame,useVideoConfig} from 'remotion';
import {Audio, Video} from '@remotion/media';
import {PilotSchema, type Pilot, type Scene} from './model.ts';
import {motionAt} from './motion.ts';
import {WordCaptions} from './Captions.tsx';

const ramp = (f: number, a: number, b: number, x = 0, y = 1) => interpolate(f, [a, b], [x, y], {extrapolateLeft: 'clamp', extrapolateRight: 'clamp'});

export const Treatment: React.FC<{scene: Scene; pilot:Pilot}> = ({scene,pilot:p}) => {
  const f = useCurrentFrame()+scene.start;
  const {fps}=useVideoConfig(),pr=p.profile;
  const {ink,accent:red,secondary:gold,headlineFont:headline,bodyFont:body}=pr;
  const strength=pr.motionStrength*(p.variant==='expressive'?1.3:1);
  const enter=motionAt(scene.entrance,f-scene.start,scene.end-scene.start,fps,strength);
  const source = scene.kind === 'source-card';
  return <AbsoluteFill style={{opacity:enter.opacity, background:`linear-gradient(140deg,${pr.background}F5,${pr.panel}F7)`, color: ink, fontFamily: body, padding: '96px 130px 210px'}}>
    <div style={{height: 6, width: 84, background: red, marginBottom: 24}} />
    <div style={{fontSize: 22, letterSpacing: 5, color: gold, marginBottom: 16}}>{scene.eyebrow??(source?'NARRATION CONTEXT':scene.kind==='comparison'?'THE DISTINCTION':'THE BREAKDOWN')}</div>
    <div style={{fontFamily: headline, fontSize: 76, lineHeight: 1.08, maxWidth: 1550,translate:enter.translate,scale:enter.scale}}>{scene.title}</div>
    <div style={{display: 'flex', flexDirection: scene.kind === 'comparison' ? 'row' : 'column', gap: 22, marginTop: 48}}>
      {scene.items.map((item, i) => {
        const reveal=motionAt(item.motion??scene.reveal,f-item.at,scene.end-item.at,fps,strength);
        const arrived = f >= item.at;
        return <div key={i} style={{flex: scene.kind === 'comparison' ? 1 : undefined, minHeight: scene.kind === 'comparison' ? 205 : 86,
          ...reveal,opacity: reveal.opacity * (scene.kind === 'comparison' && i === 0 ? ramp(f,scene.items[1].at,scene.items[1].at+10,1,0.62) : 1),
          border: `1px solid ${i === scene.items.length-1 ? red : '#454955'}`, borderLeft: `6px solid ${i === scene.items.length-1 ? red : gold}`,
          borderRadius: 14, padding: '24px 30px', background: arrived ? pr.panel : 'transparent',
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
  const {fps}=useVideoConfig();
  const current = p.captions.find(c => c.start <= frame && c.end > frame);
  const designed = p.variant !== 'baseline';
  const soundGain = p.variant === 'expressive' ? 1 : 0.62;
  return <AbsoluteFill style={{background:p.profile.background}}>
    <Video src={staticFile(p.baseline)} muted durationInFrames={p.duration} premountFor={fps} style={{width:'100%',height:'100%',objectFit:'contain'}} />
    {designed && p.scenes.map(s => <Sequence key={s.id} from={s.start} durationInFrames={s.end-s.start} premountFor={fps} name={s.id}>
      <Treatment scene={s} pilot={p} />
    </Sequence>)}
    {p.wordCaptions.length>0?<WordCaptions pilot={p}/>:current && <div style={{position:'absolute', bottom:66, left:150, right:150,textAlign:'center'}}>
      <span style={{fontFamily:p.profile.captionFont,fontSize:48,lineHeight:1.3,color:p.profile.ink,background:'rgba(0,0,0,.73)',padding:'8px 20px',borderRadius:8}}>{current.text}</span>
    </div>}
    <Audio src={staticFile(p.voice)} durationInFrames={p.duration} premountFor={fps} />
    {p.music && <Audio src={staticFile(p.music)} durationInFrames={p.duration} premountFor={fps} volume={0.065} />}
    {designed && p.sounds.map((s,i) => <Audio key={i} src={staticFile(s.file)} from={s.target-s.transient} durationInFrames={s.duration} premountFor={fps} volume={soundGain*Math.pow(10,s.db/20)} />)}
  </AbsoluteFill>;
};

// The sequence is intentionally data-controlled; individual edits live in pilot.json.
export const Root = () => <Composition id="EditingPilot" component={PilotVideo} width={1920} height={1080} fps={30} durationInFrames={2700}
  defaultProps={PilotSchema.parse({version:1,fps:30,duration:2700,baseline:'baseline.mp4',voice:'voice.wav',variant:'restrained',scenes:[],captions:[],sounds:[]})}
  schema={PilotSchema} calculateMetadata={({props}) => {const p=PilotSchema.parse(props);return {durationInFrames:p.duration,props:p};}} />;
