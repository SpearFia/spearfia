<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Remotion community source review

Reviewed 2026-11-01 for Spear Fia's editing design. Read selected source, package
manifests, READMEs and available license files at the pinned commits below; did not
install packages or run third-party code. Source copies and file manifests are under
`D:\SpearFia\research\editing-repos-20261101`, outside Git. No `.env`, credentials or
other secret files were opened. This is a source review, not a compatibility or visual
quality benchmark. No dependency or production project changes were made.

Follow-up: the creator requested incorporation. The [pilot](../../tools/editing-pilot/README.md)
now has original kit-inspired motion presets, data-controlled channel appearance and
script-aligned caption styles, with actual renders and focused tests. No community
source was copied; official caption/layout utilities were added. The source-review
findings below remain pinned historical observations. Production model selection and
VideoJev integration are still incomplete.

## Decisions

| Repository and inspected commit | Useful extraction | Adoption decision |
| --- | --- | --- |
| [seblavoie/remotion-kit](https://github.com/seblavoie/remotion-kit/tree/89d41078deb218806b0922f839137c33d8a3d13f), `89d4107` | Composable entrance/exit concepts; slide, fade, scale and stagger controls | Lowest priority. Use the already installed Remotion primitives; do not import this package now. |
| [AleBrito124356/remotion-video-templates](https://github.com/AleBrito124356/remotion-video-templates/tree/90149464d51a43fcb90e15491bf0d681c1b0fa49), `9014946` | Word-timed caption groups, active-word styling, lower-thirds with enter/hold/exit, and data-driven bar/line charts | Strongest source for selected adaptations. Port only needed MIT components/patterns into the current toolchain, retaining attribution for copied code. |
| [ahgsql/remotion-subtitles](https://github.com/ahgsql/remotion-subtitles/tree/d8ad50fd5bc94b5ec3e5388a2b535fa85553fd43), `d8ad50f` | Selectable caption skins, style overrides, fade/zoom/bounce and a typewriter option | Useful styling references or selected MIT component ports. Its SRT parser is not the foundation for accurate word-synchronized captions. |

## remotion-kit: convenient concepts, little benefit from a dependency

Inspected `src/AnimatedElement.tsx`, `AnimatedText.tsx`, `useAnimation.ts`,
`presets.ts`, `types.ts`, `index.ts` and `package.json`.

The small preset list supplies opacity, scale and vertical slide examples. It does
not supply footage-aware editing decisions, evidence checks or sound alignment.
The pilot already has the official spring/interpolation primitives needed for these.

Two source-level problems need resolution before adopting it:

- `useAnimation` reduces styles through object spreads. Multiple transform animations
  in one entrance can overwrite the earlier transform string, so a slide-plus-scale
  combination can lose one operation. This finding is inferred from the merge code;
  a full package render was not run.
- `createStaggeredAnimations` divides by `count - 1`; a single word/character gives
  `0 / 0` for its calculated delay. This needs an explicit single-element case.

Its manifest declares React 18 peers and an older exact `@remotion/zod-types` peer;
our pilot uses React 19 and Remotion 4.0.538. The inspected tree has no license file,
and package.json has no license declaration. Code reuse permission has not been
established, so no source was copied into Spear Fia.

References: [transform merge](https://github.com/seblavoie/remotion-kit/blob/89d41078deb218806b0922f839137c33d8a3d13f/src/useAnimation.ts),
[stagger calculation](https://github.com/seblavoie/remotion-kit/blob/89d41078deb218806b0922f839137c33d8a3d13f/src/AnimatedText.tsx),
[manifest](https://github.com/seblavoie/remotion-kit/blob/89d41078deb218806b0922f839137c33d8a3d13f/package.json).

## AleBrito templates: best candidates for targeted reuse

Inspected all 15 selected source files, including caption, lower-third, chart,
kinetic-text, intro, promo, composition, layout and font implementations. The MIT
license is present. No branded assets were imported.

- `CaptionedShort.tsx` consumes actual `startMs`/`endMs` per word, supports single
  words or groups, highlights the active word and anchors its spring to that word's
  start. This is a useful starting point for the behavior missing in the pilot.
- Its default is a vertical composition with large center captions, a title chip
  and progress bar. Adapt the word renderer to an overlay and the channel's safe
  area; do not import that entire composition as a long-form caption system.
- It groups by a fixed word count, shows the first group before a delayed first
  word, and retains the most recently started word through silence. Add punctuation,
  name/negation boundaries and explicit pause/end behavior. Its props validate basic
  numeric types but do not establish sorted/nonoverlapping word ranges or reading fit.
- `LowerThirds/styles.tsx` exposes entrance, hold and exit timing across three looks.
  A caption-safe, channel-themed version could serve CFB speakers, players or teams,
  and other channels' names/locations. Exact content remains supplied and verified.
- The bar/line chart templates accept typed data and animate its presentation.
  Useful for later news/stat comparisons. Add units, source/date, sensible axes and
  equal comparison bases; a working chart template does not verify its numbers.

The manifest pins Remotion 4.0.200, React 18 and Zod 3. Keep the pilot's current
versions; adapt selected code instead of installing this project's old dependency
tree. Its font helper loads Inter through `@remotion/google-fonts`; make licensed
font availability deterministic for local rendering. Measure real caption/graphic
collisions before treating the components as approved presets.

References: [word renderer](https://github.com/AleBrito124356/remotion-video-templates/blob/90149464d51a43fcb90e15491bf0d681c1b0fa49/src/templates/CaptionedShort/CaptionedShort.tsx),
[lower-thirds](https://github.com/AleBrito124356/remotion-video-templates/blob/90149464d51a43fcb90e15491bf0d681c1b0fa49/src/templates/LowerThirds/styles.tsx),
[line chart](https://github.com/AleBrito124356/remotion-video-templates/blob/90149464d51a43fcb90e15491bf0d681c1b0fa49/src/templates/ChartVideo/LineChart.tsx),
[license](https://github.com/AleBrito124356/remotion-video-templates/blob/90149464d51a43fcb90e15491bf0d681c1b0fa49/LICENSE).

## remotion-subtitles: skins, not a speech-alignment upgrade

Inspected `src/core/SubtitleSequence.js`, all 17 caption components and their exports.
Package name is **`remotion-subtitle`**, singular, version 2.0.0 in the inspected
manifest. It declares React 19 and Remotion `^4.0.374` peers, broadly compatible with
our declared versions, but actual integration has not been tested. MIT license present.

The core reads SRT cues and wraps each cue in a Remotion Sequence. The skin receives
the cue's text, not individual word timestamps. Fade, restrained zoom or an adapted
bounce could become optional channel styles. Typewriter is useful for a deliberate
quotation/document reveal, rather than a narration-synchronization mechanism.

`TypewriterCaption` reveals characters evenly over half the cue's duration. Its
reveal rate does not follow individual spoken words. The general skins do not solve
misrecognized names, phrase grouping, real speech timing, reading time or collision
with a diagram. Defaults use large viewport-relative type and differing fonts; some
have repeated motion, blur or glow requiring a channel-specific readability review.
The core also multiplies SRT seconds by fps directly, so any port must handle frame
rounding and range validation explicitly. Fonts and styles need normalization.

Retain our word timing/data pipeline and the official caption utilities. Reuse a
small approved set of visual skins on that data, after audition. The package's test
script explicitly has no tests; successful installation would not establish quality.

References: [SRT wrapper](https://github.com/ahgsql/remotion-subtitles/blob/d8ad50fd5bc94b5ec3e5388a2b535fa85553fd43/src/core/SubtitleSequence.js),
[typewriter](https://github.com/ahgsql/remotion-subtitles/blob/d8ad50fd5bc94b5ec3e5388a2b535fa85553fd43/src/captions/TypewriterCaption.js),
[fade](https://github.com/ahgsql/remotion-subtitles/blob/d8ad50fd5bc94b5ec3e5388a2b535fa85553fd43/src/captions/FadeCaption.js),
[manifest](https://github.com/ahgsql/remotion-subtitles/blob/d8ad50fd5bc94b5ec3e5388a2b535fa85553fd43/package.json),
[license](https://github.com/ahgsql/remotion-subtitles/blob/d8ad50fd5bc94b5ec3e5388a2b535fa85553fd43/LICENSE).

## Caption implementation order

1. Reuse the main script/VO alignment; expose uncertain/interpolated words and
   speech gaps rather than treating the pilot's raw ASR as authoritative text.
2. Carry one word-level caption plan through both native captions and animated
   renders. The official [`Caption` format](https://www.remotion.dev/docs/captions/caption)
   and [caption grouping utilities](https://www.remotion.dev/docs/captions/create-tiktok-style-captions)
   are appropriate render-boundary foundations. Grouping still needs our editorial
   constraints; the utility name does not require choosing a TikTok visual style.
3. Audition clean phrase, active-word accent and restrained-emphasis variants on
   identical narration/footage, with channel-controlled fonts, colors and intensity.
   AleBrito supplies useful active-word behavior; ahgsql supplies optional skin ideas.
4. Keep the same plan available as native editable captions and SRT/VTT. Select
   exactly one visible presentation so animated overlays do not double native text.
5. Check timing, names, negations, punctuation, silence, frame-rate conversion, text
   bounds and caption/graphic overlap. Require creator style approval before reuse.

See [the editing handoff](../editing-handoff.md) for the shared engine/channel-pack
boundary and VideoJev/Clef responsibilities. No live model inference, package import,
subtitle replacement or new render was performed as part of this source review.
