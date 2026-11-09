//! My Plugin: a panel beside the map.
//!
//! `panel` in `plugin.json` declares a pane of widgets bound to the plugin's commands. Its button
//! comes first in the plugin's group on the **Plugins** tab. RustGIS keeps the widgets' values
//! (a plugin keeps nothing between calls) and sends them with every request, as `req.panel`;
//! any command can answer the panel's new state as `{"panel": ...}` beside the rest of its
//! answer.
//!
//! - `panel-state` runs when the panel opens and when *Vector layers only* changes: it reads the
//!   layers and fills the slider with their names.
//! - `show-layer` runs when the slider is let go or stepped. RustGIS keeps a slider's value
//!   within its ends, so `plugin.json` declares wide ones (0 to 999) for a call made before the
//!   panel's state has set them to the number of layers.
//! - `zoom-to` and `toggle-visibility` are the buttons. They act on the slider's layer, so they
//!   work from the ribbon, scripts and `rustgis-cli` too (which have no panes):
//!   `plugin:my-plugin/zoom-to={"panel": {"layer": 1}}`.
//!
//! Panels need a RustGIS newer than 0.3.0; older ones ignore `panel`, and the commands still
//! appear on the Plugins tab.

use rustgis_plugin::{Panel, Request, Value, host, json};

/// `plugin.json`, for its translations.
const MANIFEST: &str = include_str!("../plugin.json");

fn handle(req: Request) -> Result<Value, String> {
    let layers = map_layers(&req)?;
    // The slider's layer: kept within the list, which may have changed since it was set.
    let at = req.panel_i64("layer").unwrap_or(0).clamp(0, layers.len().saturating_sub(1) as i64) as usize;
    match req.id.as_str() {
        "panel-state" | "show-layer" => {}
        "zoom-to" => {
            host::run("layer.zoomTo", json!({"layer": chosen(&req, &layers, at)?["name"]}))?;
        }
        "toggle-visibility" => {
            host::run("layer.visibility", json!({"layer": chosen(&req, &layers, at)?["name"]}))?;
        }
        other => return Err(format!("My Plugin has no command '{other}'")),
    }
    // Read again: the command may have changed a layer (its visibility).
    let layers = map_layers(&req)?;
    Ok(panel(&req, &layers, at).answer(json!({"layer": layers.get(at).map(|l| &l["name"])})))
}

rustgis_plugin::export!(handle);

/// The map's layers, or only its vector layers when *Vector layers only* is ticked.
fn map_layers(req: &Request) -> Result<Vec<Value>, String> {
    let vector_only = req.panel_bool("vector-only").unwrap_or(false);
    let all = host::layers()?.as_array().cloned().unwrap_or_default();
    Ok(all.into_iter().filter(|l| !vector_only || l["raster"] != true).collect())
}

fn chosen<'a>(req: &Request, layers: &'a [Value], at: usize) -> Result<&'a Value, String> {
    layers.get(at).ok_or_else(|| req.tr(MANIFEST, "The map has no layers.", &[]))
}

/// The panel's state for the layer at `at`: the slider's names, the layer's summary and fields.
fn panel(req: &Request, layers: &[Value], at: usize) -> Panel {
    let names: Vec<String> = layers.iter().map(|l| l["name"].as_str().unwrap_or("?").to_string()).collect();
    let Some(layer) = layers.get(at) else {
        let none = req.tr(MANIFEST, "The map has no layers.", &[]);
        return Panel::new()
            .value("layer", 0)
            .slider("layer", 0, 0, std::slice::from_ref(&none))
            .text("summary", none)
            .text("fields", "")
            .enabled("zoom-to", false)
            .enabled("toggle-visibility", false);
    };
    let visible = req.tr(MANIFEST, if layer["visible"] == false { "hidden" } else { "visible" }, &[]);
    let geometry = if layer["raster"] == true { "raster" } else { layer["geometry"].as_str().unwrap_or("table") };
    let summary = req.tr(
        MANIFEST,
        "{features} features, {geometry}, {visible}",
        &[("features", &layer["features"].as_u64().unwrap_or(0).to_string()), ("geometry", geometry), ("visible", &visible)],
    );
    Panel::new()
        .value("layer", at as i64)
        .slider("layer", 0, names.len() as i64 - 1, &names)
        .text("summary", format!("{}\n{summary}", names[at]))
        .text("fields", fields(layer))
        .enabled("zoom-to", true)
        .enabled("toggle-visibility", true)
}

/// A layer's fields, one `name (type)` per line.
fn fields(layer: &Value) -> String {
    let fields = layer["fields"].as_array().map(Vec::as_slice).unwrap_or_default();
    let lines: Vec<String> =
        fields.iter().map(|f| format!("{} ({})", f["name"].as_str().unwrap_or("?"), f["type"].as_str().unwrap_or("?"))).collect();
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_fields_one_per_line() {
        let layer = json!({"fields": [{"name": "name", "type": "text"}, {"name": "pop_est", "type": "integer"}]});
        assert_eq!(fields(&layer), "name (text)\npop_est (integer)");
        assert_eq!(fields(&json!({})), "");
    }
}
