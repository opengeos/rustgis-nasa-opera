# RustGIS Plugin Template

A template for [RustGIS](https://github.com/opengeos/RustGIS) plugins written in Rust. A plugin
is a WebAssembly module with a `plugin.json` manifest, packaged as a `.rgplugin` file. It runs
sandboxed in the desktop app, in `rustgis-cli` and in the [web app](https://web.rustgis.app).

Click **Use this template** on GitHub (or fork this repository), pick a starter, and the
workflows here build, test and release your plugin, ready to submit to the
[plugin registry](https://plugins.rustgis.app).

For a plugin in Python, use
[rustgis-python-plugin-template](https://github.com/opengeos/rustgis-python-plugin-template).

## Starters

Each starter in [`starters/`](starters) is a complete plugin that shows one way a plugin
appears in RustGIS:

| Starter | Where it appears | What it shows |
|---|---|---|
| [`commands`](starters/commands) (the default) | buttons in a group named after the plugin on the **Plugins** tab | reading the map, running RustGIS commands, adding a layer, translated messages |
| [`tab`](starters/tab) | a ribbon tab of its own, with groups, a dropdown menu and a tool button | `tab` in `plugin.json`, moving the map, a tool opened from the ribbon |
| [`panel`](starters/panel) | a pane beside the map, opened from the **Plugins** tab | `panel` in `plugin.json`: a slider, a checkbox, texts and buttons bound to commands, and the state the plugin answers |
| [`tools`](starters/tools) | geoprocessing tools in the **Tools** pane, under **Plugins ›** | tool parameters (layer, field, choice, number, text), a new layer, a selection |

The sections are independent: one plugin can have commands, tools, a tab and a panel. Start
from one starter and copy what you need from the others.

Panels need a RustGIS newer than 0.3.0. Older versions ignore `panel`, and the panel starter's
commands still appear on the Plugins tab.

## Quick start

You need [Rust](https://rustup.rs) and Python 3.11 or later.

```sh
rustup target add wasm32-unknown-unknown

# Pick a starter (commands, tab, panel or tools) and name your plugin.
python scripts/setup.py --starter tab --id my-plugin --name "My Plugin" --author "Your Name"

# Build, check and package it.
python scripts/package.py
```

`setup.py` copies the starter to `plugin.json` and `src/lib.rs`. It also sets the plugin's id,
name, author and homepage, and names the crate after the id. It takes the homepage, and by
default the id and author, from your repository's GitHub address. Run it again to switch
starters, but note that it overwrites `plugin.json` and `src/lib.rs`. Once you've picked one,
you can delete `starters/` and `scripts/setup.py`.

`package.py` writes:

| File | What |
|---|---|
| `dist/<id>/` | `plugin.json` and `plugin.wasm`, a plugin folder to load while you work |
| `dist/<id>-<version>.rgplugin` | the package |
| `dist/<id>.json` | the entry to submit to the registry |

It checks the package as the registry will, using the registry's own
[`build.py`](https://github.com/opengeos/rustgis-plugins/blob/main/scripts/build.py). Pass
`--offline` to skip that check.

## Develop

- **Try it in RustGIS:** open **Plugins › Plugin Manager › Settings › Add Plugin Folder…** and
  pick `dist/<id>`. RustGIS remembers the folder and loads it at each start. After
  `python scripts/package.py`, add the folder again (or restart RustGIS) to load the new
  build.
- **Try it on the command line:**

  ```sh
  rustgis-cli run --plugin dist/my-plugin --sample --cmd 'plugin:my-plugin/layer-report'
  rustgis-cli run --plugin dist/my-plugin --sample \
    --cmd 'gp.run={"tool": "plugin:my-plugin/centroids", "params": {"input": "Countries"}}' \
    --cmd 'share.exportMap={"path": "map.png"}'
  ```

- **Run it as CI does:** `python scripts/smoke.py` loads the package in `rustgis-cli`, on the
  World Atlas sample. It runs every tool with its defaults and every command that needs no
  parameters. `--strict` also fails when a command fails.
- **Unit tests:** `cargo test` runs the tests in `src/lib.rs` on your own computer. Keep
  `host::` calls out of the functions you test, since they only work inside RustGIS.
- **Lint:** `cargo fmt` and `cargo clippy --target wasm32-unknown-unknown`, or
  `pre-commit run --all-files`.

The plugin is a function that answers requests. RustGIS calls it when someone runs one of the
plugin's commands or tools. It reads and changes the map through the `rustgis-plugin` SDK's
`host` module, and returns JSON:

```rust
use rustgis_plugin::{Request, Value, host, json};

fn handle(req: Request) -> Result<Value, String> {
    match req.id.as_str() {
        "layer-report" => {
            let n = host::layers()?.as_array().map_or(0, Vec::len);
            host::notify(&format!("{n} layers"));
            Ok(json!({"layers": n}))
        }
        other => Err(format!("unknown command '{other}'")),
    }
}

rustgis_plugin::export!(handle);
```

See [Write a plugin](https://plugins.rustgis.app/develop/) for `plugin.json`, the permissions
(`edit`, `files`, `network`), file parameters, data from the web, translations, tabs and panels.
RustGIS's [plugin documentation](https://github.com/opengeos/RustGIS/blob/main/docs/plugins.md)
covers everything a plugin can ask for. `rustgis-cli commands` lists every RustGIS command a
plugin can run, with its parameters.

## Release

1. Bump `version` in `plugin.json` and in `Cargo.toml`. Packages never change once published,
   so every release needs a new version.
2. Commit, then push a tag that matches the version:

   ```sh
   git tag v0.1.0
   git push origin v0.1.0
   ```

The [Release](.github/workflows/release.yml) workflow builds and checks the plugin, and runs it
in the latest RustGIS. It then publishes a GitHub release with two files:

- `<id>-<version>.rgplugin`, the package.
- `<id>.json`, the registry entry, with the package's address and SHA-256.

The release notes repeat the entry. Anyone can install the package from the release, through
**Plugin Manager › Settings › Install From File…** or with
`rustgis-cli plugins install <file>`.

A tag that doesn't match `plugin.json`'s version fails the workflow. CI tests against the latest
RustGIS release; set the `RUSTGIS_VERSION` repository variable (to `v0.3.0`, say) to test
against another one.

## Submit to the plugin registry

The [registry](https://plugins.rustgis.app) is curated: a maintainer reviews each plugin before
it is listed. To submit yours, open a pull request on
[opengeos/rustgis-plugins](https://github.com/opengeos/rustgis-plugins) that adds
`registry/<id>.json`:

1. Take `<id>.json` from your release (or `dist/<id>.json`). Set its `categories` to one to four
   of: Analysis, Basemaps, Cartography, Data, Editing, Example, Hydrology, Imagery, Raster,
   Sampling, Terrain, Utilities, Vector, Visualization.

   ```json
   {
     "id": "my-plugin",
     "source": {
       "url": "https://github.com/you/my-plugin/releases/download/v0.1.0/my-plugin-0.1.0.rgplugin",
       "sha256": "..."
     },
     "categories": ["Vector"]
   }
   ```

2. Add it as `registry/<id>.json` and open the pull request, on the web or with `gh`:

   ```sh
   gh repo fork opengeos/rustgis-plugins --clone && cd rustgis-plugins
   git switch -c add-my-plugin
   cp ../my-plugin/dist/my-plugin.json registry/   # then edit its categories
   git add registry/my-plugin.json && git commit -m "Add My Plugin to the registry"
   git push -u origin add-my-plugin && gh pr create --fill
   ```

The registry's CI downloads the package, checks its SHA-256, validates it and runs it in
RustGIS. The pull request also gets a preview link that opens RustGIS's web app with your plugin
ready to install. The listing's name, description, version and permissions come from your
`plugin.json`. Once the PR is merged, the package is served from `packages.rustgis.app`, and
RustGIS users can install it from **Plugins › Plugin Manager › Browse**.

**Updating:** release a new version, then open a pull request that updates `source` in
`registry/<id>.json` to the new release's `url` and `sha256`. RustGIS shows an **Update**
button to everyone with an older version.

See [Publish a plugin](https://plugins.rustgis.app/registry/) for what the review looks at.

## Layout

| Path | What |
|---|---|
| `plugin.json` | the manifest: id, version, permissions, commands, tools, tab, panel, translations |
| `src/lib.rs` | the plugin |
| `Cargo.toml` | the crate; `Cargo.lock` pins the SDK's version |
| `starters/` | the four starters |
| `scripts/setup.py` | picks a starter and names the plugin |
| `scripts/package.py` | builds, checks and packages the plugin into `dist/` |
| `scripts/smoke.py` | runs the package in `rustgis-cli` |
| `.github/workflows/ci.yml` | builds, tests and runs the plugin on each push and pull request |
| `.github/workflows/release.yml` | publishes a release when you push a `v*` tag |

`cargo update -p rustgis-plugin` updates the SDK.

## License

MIT for the template. Your plugin states its own license in `plugin.json`.
