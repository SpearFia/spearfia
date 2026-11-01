// SPDX-License-Identifier: MIT OR Apache-2.0
import {createHash} from 'node:crypto';
import {z} from 'zod';
import type {Scene} from './model.ts';

const criteria = {
  progressive: 'Choose progressive only when the beat contains 2–4 distinct points that benefit from accumulating in one view. Labels must agree with the supplied narration and follow its order. Prefer plain footage if the card adds no explanation or leaves too little reading time. Choose none-of-these if a safe decision needs missing context or new wording.',
  comparison: 'Choose comparison when two supplied statements clarify a meaningful contrast on the same basis. The second reveal must improve understanding without implying an unsupported cause or equivalence. Prefer plain footage when showing the actual action is more useful. Choose none-of-these if a safe decision needs missing context or new wording.',
  'source-card': 'Choose source-card when a brief quotation or summary and its supplied provenance help explain this beat. Narration summaries are not independent evidence, and ASR context cannot verify a factual claim. Prefer plain footage if the card merely repeats what is already clear. Choose none-of-these if evidence or context is missing.',
};

export function decisionPacket(scene: Scene, context: {narration: string; neighbors: string[]; [key:string]:unknown} = {narration: '', neighbors: []}) {
  const options = [scene.kind, 'plain-hold', 'none-of-these'];
  const packet = {id: scene.id, situation: scene, context, options,
    question: 'Which treatment best serves the supplied beat?',
    criteria: criteria[scene.kind],
    mode: 'shadow-only'};
  return {...packet, hash: createHash('sha256').update(JSON.stringify(packet)).digest('hex')};
}
export function assessChoice(packet: ReturnType<typeof decisionPacket>, result: unknown) {
  const r = z.object({hash: z.string(), probabilities: z.record(z.string(), z.number().finite().min(0).max(1))}).strict().parse(result);
  if (r.hash !== packet.hash) throw new Error('Stale decision context');
  const keys = Object.keys(r.probabilities);
  if (keys.length !== packet.options.length || keys.some(k => !packet.options.includes(k))) throw new Error('Candidate set differs');
  if (Math.abs(Object.values(r.probabilities).reduce((a, b) => a + b, 0) - 1) > 0.001) throw new Error('Probabilities must sum to one');
  const ranked = Object.entries(r.probabilities).sort((a, b) => b[1] - a[1]);
  return {choice: ranked[0][0], probability: ranked[0][1], margin: ranked[0][1] - ranked[1][1],
    eligible: ranked[0][0] !== 'none-of-these' && ranked[0][1] >= 0.95 && ranked[0][1] - ranked[1][1] >= 0.20,
    applied: false, reason: 'Shadow only; calibration and creator review still required'};
}
