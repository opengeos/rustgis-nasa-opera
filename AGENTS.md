# AGENTS.md

The NASA OPERA plugin for RustGIS: a WebAssembly module (`wasm32-unknown-unknown`) built with the
`rustgis-plugin` SDK, with a `plugin.json` manifest. RustGIS runs it sandboxed in the desktop app,
`rustgis-cli` and the web app. See [README.md](README.md) for what it does and
[RustGIS's plugin docs](https://github.com/opengeos/RustGIS/blob/main/docs/plugins.md) for the
host API.

## Layout

- `src/lib.rs`: the commands and the panel. Every `host::` call lives here.
- `src/opera.rs`: products, CMR parsing, dates, styles and titiler-cmr addresses. Plain functions
  with no host calls, so `cargo test` covers them; put new logic here and test it.
- `plugin.json`: the manifest. A new or renamed command, panel widget or parameter changes it too,
  and each command's `params` string documents its parameters.
- `skills/opera-flood-map/SKILL.md`: the AI Assistant skill shipped in the package; keep it in
  step with the commands' parameters.
- `scripts/package.py` builds and packages the plugin and runs the registry's checks;
  `scripts/smoke.py` runs it in `rustgis-cli`.

## Rules

- No `unwrap`, `expect` or `panic!` outside tests: a panic aborts the module, and RustGIS reports
  the command as failed. Return `Err(String)` with a message a user can act on.
- A plugin keeps nothing between calls. State lives in the panel's values or on the map (the
  footprints layer's attributes); don't add statics that assume otherwise.
- A command has a fuel budget (500M instructions) and `host.fetch` answers at most 2 MB. Keep
  CMR pages at `opera::PAGE` granules and read them into the typed structs in `opera.rs`, not a
  `serde_json::Value` of the whole answer.
- In the web app `host.fetch` answers `{"pending": true}` and the command runs again later:
  return early before changing the project, as `search` does.
- The plugin contacts only `cmr.earthdata.nasa.gov` and the titiler-cmr endpoint; the README's
  Install section says so. A new host is a permissions change: say so in the pull request.
- Permissions stay `edit` and `network`.
- Every bug fix comes with a unit test.

## Check

```sh
cargo fmt --all
cargo clippy --target wasm32-unknown-unknown --locked -- -D warnings
cargo test --locked
python scripts/package.py   # build, package, registry checks
```

`pre-commit run --all-files` runs fmt and clippy. For a change to search or display, run it end to
end in `rustgis-cli` (the README has an example) and look at an exported map.

Commit messages are wrapped at 72 columns, subject under 72 characters, no AI attribution.
