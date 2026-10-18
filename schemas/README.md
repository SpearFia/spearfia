# Pinned schemas

A copy of the schemas from [spearfia-spec](https://github.com/spearfia/spearfia-spec),
so the app builds and tests against a known version. **Do not edit these files here.**
Change the schema in `spearfia-spec` first, then update this copy.

Pinned to: `spearfia-spec` initial scaffold (draft v1).

## Updating

With both repos side by side in `C:\dev\spearfia`:

```powershell
git -C ..\spearfia-spec pull
Copy-Item ..\spearfia-spec\schemas\*.schema.json .\schemas\ -Force
```

Then update the "Pinned to" line above with the spec commit, and commit both together
with any code changes the new schemas need.
