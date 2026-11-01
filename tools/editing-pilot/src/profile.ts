// SPDX-License-Identifier: MIT OR Apache-2.0
import {z} from 'zod';
const color=z.string().regex(/^#[0-9a-fA-F]{6}$/);
export const ProfileSchema=z.object({
  name:z.string().min(1).max(80),
  ink:color,accent:color,secondary:color,background:color,panel:color,
  headlineFont:z.string().min(1).max(80),bodyFont:z.string().min(1).max(80),
  captionFont:z.string().min(1).max(80),captionSize:z.number().min(40).max(72),
  captionWidth:z.number().int().min(900).max(1500),captionBottom:z.number().min(50).max(110),
  motionStrength:z.number().min(.3).max(1.3),
}).strict();
export const editorialProfile=ProfileSchema.parse({name:'Editorial slate - draft',ink:'#F4F0E8',accent:'#E34747',secondary:'#E3BB78',
  background:'#080C13',panel:'#171B24',headlineFont:'Impact',bodyFont:'Arial',captionFont:'Arial',
  captionSize:56,captionWidth:1400,captionBottom:72,motionStrength:.9});
export type ChannelProfile=z.infer<typeof ProfileSchema>;
