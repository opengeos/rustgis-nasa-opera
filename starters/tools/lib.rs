//! My Plugin: geoprocessing tools.
//!
//! A plugin's `tools` appear in the **Tools** pane under **Plugins › <toolbox>**, in command
//! search, in `gp.run` and on the command line, with a form RustGIS builds from their `params`.
//! A tool gets its layer parameters' features as GeoJSON (only the selected features when a
//! layer has a selection) and returns new layers, tables or a selection, which RustGIS applies
//! with undo. Tools make no host requests, so they need no permissions.
//!
//! - `centroids` returns a new point layer.
//! - `select-by-value` returns a selection, reading a `numericField`, a `choice` and a `number`.
//!
//! Try them on the command line:
//!
//! ```text
//! rustgis-cli run --plugin dist/my-plugin --sample \
//!   --cmd 'gp.run={"tool": "plugin:my-plugin/select-by-value", "params": {"input": "Countries", "field": "pop_est", "value": 1e8}}'
//! ```

use rustgis_plugin::{Kind, Request, ToolOutput, Value, geojson};

/// `plugin.json`, for its translations.
const MANIFEST: &str = include_str!("../plugin.json");

fn handle(req: Request) -> Result<Value, String> {
    match (req.kind, req.id.as_str()) {
        (Kind::Tool, "centroids") => centroids(&req),
        (Kind::Tool, "select-by-value") => select_by_value(&req),
        (_, other) => Err(format!("My Plugin has no '{other}'")),
    }
}

rustgis_plugin::export!(handle);

fn centroids(req: &Request) -> Result<Value, String> {
    let features = req.features("input")?;
    let points: Vec<Value> = features
        .iter()
        .filter_map(|f| {
            let [x, y] = mean(&geojson::positions(&f["geometry"]))?;
            Some(geojson::feature(geojson::point(x, y), f["properties"].clone()))
        })
        .collect();
    let n = points.len().to_string();
    let name = format!("{} {}", req.str("input").unwrap_or("Input"), req.str("suffix").unwrap_or("centroids"));
    Ok(ToolOutput::new().layer(name.trim(), geojson::collection(points)).message(req.tr(MANIFEST, "{n} centroids", &[("n", &n)])).into())
}

fn select_by_value(req: &Request) -> Result<Value, String> {
    let layer = req.str("input").ok_or("'input' is required")?;
    let field = req.str("field").filter(|f| !f.is_empty()).ok_or("'field' is required")?;
    let value = req.f64("value").unwrap_or(0.0);
    let op = req.str("op").unwrap_or("greater than");
    let features = req.features("input")?;
    // Rows are positions in what the tool got: the whole layer, or its selected features.
    let rows: Vec<usize> = features
        .iter()
        .enumerate()
        .filter(|(_, f)| f["properties"][field].as_f64().is_some_and(|v| compare(v, op, value)))
        .map(|(i, _)| i)
        .collect();
    let (n, total) = (rows.len().to_string(), features.len().to_string());
    Ok(ToolOutput::new()
        .select(layer, &rows)
        .message(req.tr(MANIFEST, "{n} of {total} features selected", &[("n", &n), ("total", &total)]))
        .into())
}

/// The mean of `points`, or `None` when there are none.
fn mean(points: &[[f64; 2]]) -> Option<[f64; 2]> {
    if points.is_empty() {
        return None;
    }
    let n = points.len() as f64;
    let (x, y) = points.iter().fold((0.0, 0.0), |(x, y), p| (x + p[0], y + p[1]));
    Some([x / n, y / n])
}

/// Whether `v` is `op` (one of the `choice`'s values) `value`.
fn compare(v: f64, op: &str, value: f64) -> bool {
    match op {
        "less than" => v < value,
        "equal to" => v == value,
        _ => v > value,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn means_and_comparisons() {
        assert_eq!(mean(&[[0.0, 0.0], [2.0, 4.0]]), Some([1.0, 2.0]));
        assert_eq!(mean(&[]), None);
        assert!(compare(2.0, "greater than", 1.0) && compare(1.0, "less than", 2.0) && compare(1.0, "equal to", 1.0));
        assert!(!compare(1.0, "greater than", 1.0));
    }
}
