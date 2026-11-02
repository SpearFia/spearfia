// SPDX-License-Identifier: MIT OR Apache-2.0
import type {Caption} from '@remotion/captions';
import {z} from 'zod';

export const WordSchema=z.object({id:z.number().int().nonnegative(),text:z.string().min(1).max(100),
  startMs:z.number().finite().nonnegative(),endMs:z.number().finite().nonnegative(),
  timestampMs:z.number().finite().nonnegative().nullable(),confidence:z.number().min(0).max(1).nullable(),
  timing:z.enum(['matched-observation','interpolated']),emphasis:z.boolean().default(false),
}).strict();
export type CaptionWord=z.infer<typeof WordSchema> & Caption;
export type CaptionPage={startMs:number;endMs:number;wordIds:number[];text:string};
export type AlignedWord={id:number;line:number;text:string;startFrame:number;endFrame:number};
export const normalizeWord=(s:string)=>s.toLowerCase().replace(/[^\p{L}\p{N}]/gu,'');
export function reconcileWords(aligned:AlignedWord[],observed:[string,number,number][],start:number,duration:number,fps:number,emphasis:string[]=[]):CaptionWord[] {
  const marked=new Set(emphasis.map(normalizeWord));
  return aligned.filter(w=>w.startFrame>=start&&w.startFrame<start+duration).map(w=>{
    const matches=observed.filter(o=>normalizeWord(o[0])===normalizeWord(w.text)&&Math.round(o[1]*fps)===w.startFrame);
    const match=matches.length===1?matches[0]:undefined;
    const end=Math.min(start+duration,w.endFrame,match?Math.max(w.startFrame+1,Math.round(match[2]*fps)):w.endFrame);
    if(end<=w.startFrame) throw new Error('A script word has no usable timing; review alignment before captioning');
    return WordSchema.parse({id:w.id,text:w.text,startMs:(w.startFrame-start)/fps*1000,endMs:(end-start)/fps*1000,
      timestampMs:null,confidence:null,timing:match?'matched-observation':'interpolated',emphasis:marked.has(normalizeWord(w.text))});
  });
}
const stop=/[.!?;:]$/;
const negation=/^(not|no|never|without)$/i;

export function planCaptionPages(words:CaptionWord[],width:(text:string)=>number,maxWidth:number,protectedPhrases:string[]=[]) : CaptionPage[] {
  const boundaries=new Set<number>();
  const normalize=(s:string)=>s.toLowerCase().replace(/[^\p{L}\p{N}]/gu,'');
  const tokens=words.map(w=>normalize(w.text));
  for(const phrase of protectedPhrases) {
    const wanted=phrase.split(/\s+/).map(normalize);
    for(let i=0;i+wanted.length<=tokens.length;i++) if(wanted.every((t,j)=>t===tokens[i+j]))
      for(let j=0;j<wanted.length-1;j++) boundaries.add(i+j);
  }
  words.forEach((w,i)=>{if((negation.test(normalize(w.text))||/n['’]t[.,!?;:]?$/i.test(w.text)) && i+1<words.length && words[i+1].startMs-w.endMs<350) boundaries.add(i);});
  const pages:CaptionPage[]=[];
  let i=0;
  while(i<words.length) {
    let end=i;
    while(end+1<words.length) {
      const next=end+1;
      if(!boundaries.has(end) && (stop.test(words[end].text)||words[next].startMs-words[end].endMs>=350)) break;
      const candidate=words.slice(i,next+1).map(w=>w.text).join(' ');
      if(!boundaries.has(end) && (next-i>=7||words[next].endMs-words[i].startMs>3000||width(candidate)>maxWidth)) break;
      end=next;
    }
    // Move a protected name/negation chain intact to the next page when it would not fit.
    if(width(words.slice(i,end+1).map(w=>w.text).join(' '))>maxWidth) {
      let split=end;
      while(split>i && boundaries.has(split-1)) split--;
      if(split===i) throw new Error('Protected caption phrase cannot fit at the approved font size');
      end=split-1;
    }
    const group=words.slice(i,end+1),text=group.map(w=>w.text).join(' ');
    if(width(text)>maxWidth) throw new Error('Caption cannot fit at the approved font size');
    const nextStart=words[end+1]?.startMs??Infinity;
    pages.push({startMs:group[0].startMs,endMs:Math.min(group.at(-1)!.endMs+100,nextStart),wordIds:group.map(w=>w.id),text});
    i=end+1;
  }
  const balanced:CaptionPage[]=[];
  for(const page of pages) {
    const previous=balanced.at(-1);
    const text=previous?previous.text+' '+page.text:page.text;
    if(previous&&page.wordIds.length<=2&&!stop.test(previous.text)&&page.startMs-previous.endMs<250
      &&previous.wordIds.length+page.wordIds.length<=10&&page.endMs-previous.startMs<=3000&&width(text)<=maxWidth) {
      previous.text=text;previous.endMs=page.endMs;previous.wordIds.push(...page.wordIds);
    } else balanced.push({...page,wordIds:[...page.wordIds]});
  }
  return balanced;
}
