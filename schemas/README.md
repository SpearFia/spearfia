# Pinned schemas

A copy of the schemas from [spearfia-spec](https://github.com/spearfia/spearfia-spec),
so the app builds and tests against a known version. **Do not edit these files here.**
Change the schema in `spearfia-spec` first, then update this copy.

Pinned to: [`spearfia-spec` 4031331cfc2022af131b473a1f33c6338c4038d3](https://github.com/SpearFia/spearfia-spec/commit/4031331cfc2022af131b473a1f33c6338c4038d3) (draft v1),
with `links` and `timeline-plan` from [SpearFia/spearfia-spec#5](https://github.com/SpearFia/spearfia-spec/pull/5)
(`28e5fea`, not merged yet).

## Updating

Production pipeline, report and run schemas additionally come from `spearfia-spec`
commit `3b84739` on `feat/cli-production-coordinator`; see its `docs/production.md`.

With both repos side by side in `C:\dev\spearfia`:

```powershell
git -C ..\spearfia-spec pull
Copy-Item ..\spearfia-spec\schemas\*.schema.json .\schemas\ -Force
```

Then update the "Pinned to" line above with the spec commit, and commit both together
with any code changes the new schemas need.

`document-edit.schema.json` and `visual-edit.schema.json` are copied unchanged from the pinned source commit. The existing schemas are unchanged from that commit; the Visual Script cue schema is referenced by the guide and is not embedded in this directory.
