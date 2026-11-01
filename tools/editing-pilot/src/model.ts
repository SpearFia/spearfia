// SPDX-License-Identifier: MIT OR Apache-2.0
import {z} from 'zod';

const frame = z.number().int().nonnegative();
const asset = z.string().min(1).refine(s => !s.includes('..') && !/[:\\]/.test(s) && !s.startsWith('/'), 'Expected a relative asset path');
export const SceneSchema = z.object({
  id: z.string().regex(/^[a-z][a-z0-9-]*$/),
  kind: z.enum(['progressive', 'comparison', 'source-card']),
  start: frame, end: frame,
  title: z.string().min(1).max(52),
  items: z.array(z.object({text: z.string().min(1).max(65), at: frame})).min(1).max(4),
  sourceLabel: z.string().max(95).optional(),
}).strict();
export type Scene = z.infer<typeof SceneSchema>;
export const PilotSchema = z.object({
  version: z.literal(1), fps: z.literal(30), duration: z.number().int().positive(),
  baseline: asset, voice: asset, music: asset.optional(),
  variant: z.enum(['baseline', 'restrained', 'expressive']),
  scenes: z.array(SceneSchema),
  captions: z.array(z.object({start: frame, end: frame, text: z.string().min(1).max(60)})),
  sounds: z.array(z.object({
    sceneId: z.string(), file: asset, target: frame, transient: frame,
    duration: z.number().int().positive(), db: z.number().min(-45).max(-3),
  })),
}).strict().superRefine((p, ctx) => {
  const fail = (message: string) => ctx.addIssue({code: 'custom', message});
  const ids = new Set<string>();
  let end = 0;
  for (const s of p.scenes) {
    if (ids.has(s.id)) fail('Duplicate scene ID');
    ids.add(s.id);
    if (s.start < end || s.end <= s.start || s.end > p.duration) fail('Scene overlap or invalid range');
    end = s.end;
    if (s.kind === 'comparison' && s.items.length !== 2) fail('Comparison needs exactly two supplied statements');
    if (s.kind === 'source-card' && !s.sourceLabel?.trim()) fail('Source cards require provenance');
    let previous = s.start;
    for (const item of s.items) {
      if (item.at < previous || item.at + 18 >= s.end) fail('Reveal needs ordered anchors and room to hold');
      previous = item.at;
    }
  }
  let captionEnd = 0;
  for (const c of p.captions) {
    if (c.start < captionEnd || c.end <= c.start || c.end > p.duration) fail('Invalid or overlapping caption range');
    captionEnd = c.end;
  }
  for (const a of p.sounds) {
    const s = p.scenes.find(s => s.id === a.sceneId);
    const at = a.target - a.transient;
    if (!s || a.target < s.start || a.target >= s.end || a.transient >= a.duration || at < 0 || at + a.duration > p.duration) fail('Sound is outside its action or timeline');
  }
});
export type Pilot = z.infer<typeof PilotSchema>;
