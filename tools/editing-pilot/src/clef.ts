// SPDX-License-Identifier: MIT OR Apache-2.0
import {decisionPacket, assessChoice} from './decisions.ts';
import {SceneSchema} from './model.ts';
import {mkdir, readFile, writeFile} from 'node:fs/promises';
import {join} from 'node:path';

const model = 'Cloudflare/clef-flash';
export const revision = 'fb4417d31c1d0f9e7e77f1141b40aaede7491e00';
type Packet = ReturnType<typeof decisionPacket>;

function validatePacket(packet: Packet) {
  const checked = decisionPacket(SceneSchema.parse(packet.situation), packet.context);
  if (JSON.stringify(packet) !== JSON.stringify(checked)) throw new Error('Decision packet changed');
  if (!packet.context.narration.trim() || !Array.isArray(packet.context.visualPlan) || !packet.context.visualPlan.length ||
      !Array.isArray(packet.context.footage) || !packet.context.footage.length) throw new Error('Incomplete editorial context');
}

export async function judge(packet: Packet, send: typeof fetch = fetch) {
  validatePacket(packet);
  const response = await send('http://127.0.0.1:8008/v1/systemone', {
    method: 'POST', headers: {'Content-Type':'application/json'}, signal: AbortSignal.timeout(120000),
    body: JSON.stringify({model, state:{situation:packet.situation, context:packet.context}, questions:{treatment:{
      type:'choice', instructions:packet.question, criteria:{
        [packet.situation.kind]:packet.criteria,
        'plain-hold':'Keep the current footage and existing presentation.',
        'none-of-these':'Abstain when the supplied context or evidence is inadequate.',
      },
    }}}),
  });
  if (!response.ok) throw new Error(`Clef worker returned HTTP ${response.status}`);
  const result = await response.json() as {model?:unknown; answers?:{treatment?:{type?:unknown; choice?:unknown; probabilities?:unknown}}};
  const answer = result.answers?.treatment;
  if (result.model !== model || answer?.type !== 'choice') throw new Error('Invalid Clef response');
  const assessment = assessChoice(packet, {hash:packet.hash, probabilities:answer.probabilities});
  if (answer.choice !== assessment.choice) throw new Error('Clef choice does not match its probabilities');
  return {...assessment, probabilities:answer.probabilities, eligible:false, applied:false, revision,
    reason:'Recommendation only; evidence and creator calibration have not been verified'};
}

export async function judgeCached(packet: Packet, folder: string, send: typeof fetch = fetch) {
  validatePacket(packet);
  const path = join(folder, `${revision}-${packet.hash}.json`);
  let cached: string | undefined;
  try { cached = await readFile(path, 'utf8'); }
  catch (error) { if ((error as NodeJS.ErrnoException).code !== 'ENOENT') throw error; }
  if (cached !== undefined) {
    const record = JSON.parse(cached);
    if (record.revision !== revision || record.hash !== packet.hash) throw new Error('Invalid decision cache');
    const assessed = assessChoice(packet, {hash:record.hash, probabilities:record.probabilities});
    return {...assessed, probabilities:record.probabilities, eligible:false, applied:false, revision,
      reason:'Recommendation only; evidence and creator calibration have not been verified'};
  }
  const result = await judge(packet, send);
  await mkdir(folder, {recursive:true});
  await writeFile(path, JSON.stringify({revision, hash:packet.hash, probabilities:result.probabilities}), {flag:'wx'});
  return result;
}
