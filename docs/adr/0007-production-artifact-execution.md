# 0007: Durable service jobs and executable production treatments

Licensed under MIT OR Apache-2.0.

Status: accepted. Date: 2026-10-27.

Agents already coordinate stages through installed CLIs. Generation also needs durable
request identities, reconciliation, verified artifacts and registration. Model-provider
interfaces alone do not supply those service operations. A second layer in `spearfia-ops`
stores dependency jobs and checkpoints; the outer coordinator retains creator review gates.
MCP/HTTP dispatches work asynchronously so status and cancellation remain available.

Jobs use content fingerprints and saved service IDs. Unknown submission outcomes remain
uncertain; only a creator can authorize another request after inspecting the service.
ComfyUI prompts are recoverable by ID/marker, while synchronous VoiceStudio TTS is
reconciled against retained take history. The service adapter imports no service code.

`spearfia-analysis` validates measured word alignment against exact script/audio content.
`spearfia-sync` compiles typed visual treatments, captions and native recipes into a
render-plan sidecar. The FilmCraft adapter applies native graphics/effects and audio
automation. Prose-only actions and unmapped capabilities fail instead of disappearing.
Existing projects without these opt-in treatments keep the basic link-based push.

Tradeoffs: sequential scheduling initially; Python standard-library HTTP subprocesses;
strict speech matching can require take correction or verified external alignment;
external services may outlive a cancelled local process. FilmCraft-native recipes are
supported, while arbitrary plugin installation and analysis-dependent recipes remain
explicitly unavailable. Formats are defined first in `spearfia-spec`.
