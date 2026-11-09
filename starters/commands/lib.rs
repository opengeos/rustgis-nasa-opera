//! My Plugin: commands on the Plugins tab.
//!
//! RustGIS puts the commands declared in `plugin.json` in a group named after the plugin on the
//! **Plugins** tab, in command search, and on the command line (`plugin:<id>/<command>`).
//!
//! - `layer-report` reads the map (`host::layers`) and shows a summary (`host::notify`).
//! - `zoom-largest` runs a RustGIS command (`layer.zoomTo`), as a ribbon button would.
//! - `pin-center` adds a layer (`host::add_layer`), which needs the `edit` permission.
//!
//! Messages are worded in the interface language with `Request::tr`, from the `i18n` table in
//! `plugin.json`.

use rustgis_plugin::{Request, Value, geojson, host, json};

/// `plugin.json`, for its translations.
const MANIFEST: &str = include_str!("../plugin.json");

fn handle(req: Request) -> Result<Value, String> {
    match req.id.as_str() {
        "layer-report" => layer_report(&req),
        "zoom-largest" => zoom_largest(&req),
        "pin-center" => pin_center(&req),
        other => Err(format!("My Plugin has no command '{other}'")),
    }
}

rustgis_plugin::export!(handle);

fn layer_report(req: &Request) -> Result<Value, String> {
    let layers = host::layers()?;
    let (n, features) = count(&layers);
    host::notify(&req.tr(
        MANIFEST,
        "{n} layers with {features} features in all.",
        &[("n", &n.to_string()), ("features", &features.to_string())],
    ));
    Ok(json!({"layers": n, "features": features}))
}

fn zoom_largest(req: &Request) -> Result<Value, String> {
    let layers = host::layers()?;
    let name = largest(&layers).ok_or_else(|| req.tr(MANIFEST, "The map has no feature layers.", &[]))?;
    host::run("layer.zoomTo", json!({"layer": name}))?;
    host::notify(&req.tr(MANIFEST, "Zoomed to {name}.", &[("name", &name)]));
    Ok(json!({"layer": name}))
}

fn pin_center(req: &Request) -> Result<Value, String> {
    let view = host::view()?;
    let (lon, lat) = (view["center"][0].as_f64().unwrap_or(0.0), view["center"][1].as_f64().unwrap_or(0.0));
    let name = req.str("name").unwrap_or("Pins");
    let pins = geojson::collection(vec![geojson::feature(geojson::point(lon, lat), json!({"lon": lon, "lat": lat}))]);
    host::add_layer(name, pins)
}

/// The number of layers in `host.layers`'s answer, and of their features.
fn count(layers: &Value) -> (usize, u64) {
    let all = layers.as_array().map(Vec::as_slice).unwrap_or_default();
    (all.len(), all.iter().filter_map(|l| l["features"].as_u64()).sum())
}

/// The name of the vector layer with the most features.
fn largest(layers: &Value) -> Option<String> {
    let all = layers.as_array()?;
    let vector = all.iter().filter(|l| l["raster"] != true);
    vector.max_by_key(|l| l["features"].as_u64().unwrap_or(0)).and_then(|l| l["name"].as_str()).map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_the_largest_vector_layer() {
        let layers = json!([
            {"name": "Countries", "features": 177},
            {"name": "Cities", "features": 243},
            {"name": "Relief", "raster": true, "features": 0}
        ]);
        assert_eq!(count(&layers), (3, 420));
        assert_eq!(largest(&layers).as_deref(), Some("Cities"));
        assert_eq!(largest(&json!([])), None);
    }
}
