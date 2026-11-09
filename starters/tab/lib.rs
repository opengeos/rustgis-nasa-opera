//! My Plugin: a ribbon tab of its own.
//!
//! `tab` in `plugin.json` lays the plugin's commands and tools out on a tab at the end of the
//! tab strip, in groups, with a dropdown menu. An item is a command id, `tool:<id>` (which opens
//! that tool in the Tools pane) or a `menu` of them.
//!
//! - `go-*` and `zoom-world` move the map with RustGIS's own commands (`map.goToXY`,
//!   `map.zoomWorld`). They change only the view, so the plugin needs no permissions.
//! - `layer-report` reads the map and shows a summary in the status bar.
//! - `bounding-boxes` is a geoprocessing tool: it gets its input layer as GeoJSON and returns a
//!   new layer, which RustGIS adds with undo.

use rustgis_plugin::{Kind, Request, ToolOutput, Value, geojson, host, json};

/// `plugin.json`, for its translations.
const MANIFEST: &str = include_str!("../plugin.json");

/// Where each `go-*` command goes: (id, longitude, latitude, scale).
const PLACES: &[(&str, f64, f64, f64)] = &[
    ("go-africa", 20.0, 2.0, 40_000_000.0),
    ("go-americas", -80.0, 10.0, 70_000_000.0),
    ("go-asia", 95.0, 35.0, 50_000_000.0),
    ("go-europe", 15.0, 52.0, 20_000_000.0),
];

fn handle(req: Request) -> Result<Value, String> {
    match (req.kind, req.id.as_str()) {
        (Kind::Command, "zoom-world") => host::run("map.zoomWorld", json!({})),
        (Kind::Command, "layer-report") => layer_report(&req),
        (Kind::Command, id) if id.starts_with("go-") => go_to(id),
        (Kind::Tool, "bounding-boxes") => bounding_boxes(&req),
        (_, other) => Err(format!("My Plugin has no '{other}'")),
    }
}

rustgis_plugin::export!(handle);

fn go_to(id: &str) -> Result<Value, String> {
    let &(_, lon, lat, scale) = PLACES.iter().find(|p| p.0 == id).ok_or_else(|| format!("no place '{id}'"))?;
    host::run("map.goToXY", json!({"lon": lon, "lat": lat, "scale": scale}))
}

fn layer_report(req: &Request) -> Result<Value, String> {
    let layers = host::layers()?;
    let all = layers.as_array().map(Vec::as_slice).unwrap_or_default();
    let features: u64 = all.iter().filter_map(|l| l["features"].as_u64()).sum();
    host::notify(&req.tr(
        MANIFEST,
        "{n} layers with {features} features in all.",
        &[("n", &all.len().to_string()), ("features", &features.to_string())],
    ));
    Ok(json!({"layers": all.len(), "features": features}))
}

fn bounding_boxes(req: &Request) -> Result<Value, String> {
    let features = req.features("input")?;
    let name = req.str("input").unwrap_or("Input");
    let boxes: Vec<Value> = if req.bool("whole") == Some(true) {
        let all: Vec<[f64; 2]> = features.iter().flat_map(|f| geojson::positions(&f["geometry"])).collect();
        geojson::bbox(&all).map(|b| vec![box_feature(b, json!({"features": features.len()}))]).unwrap_or_default()
    } else {
        features
            .iter()
            .filter_map(|f| geojson::bbox(&geojson::positions(&f["geometry"])).map(|b| box_feature(b, f["properties"].clone())))
            .collect()
    };
    let n = boxes.len();
    Ok(ToolOutput::new().layer(&format!("{name} boxes"), geojson::collection(boxes)).message(format!("{n} boxes")).into())
}

fn box_feature(b: [f64; 4], properties: Value) -> Value {
    geojson::feature(geojson::polygon(&[[b[0], b[1]], [b[2], b[1]], [b[2], b[3]], [b[0], b[3]]]), properties)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_place_has_a_command() {
        let manifest: Value = rustgis_plugin::serde_json::from_str(MANIFEST).unwrap();
        let commands: Vec<&str> = manifest["commands"].as_array().unwrap().iter().filter_map(|c| c["id"].as_str()).collect();
        for (id, ..) in PLACES {
            assert!(commands.contains(id), "{id} isn't in plugin.json");
        }
    }

    #[test]
    fn boxes_close_their_ring() {
        let ring = box_feature([0.0, 1.0, 2.0, 3.0], json!({}));
        let coords = &ring["geometry"]["coordinates"][0];
        assert_eq!(coords[0], coords[coords.as_array().unwrap().len() - 1]);
    }
}
