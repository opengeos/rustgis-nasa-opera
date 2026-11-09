//! NASA OPERA for RustGIS: search OPERA granules in NASA CMR and show them through titiler-cmr.
//!
//! - **Search** asks NASA's public CMR granule search (`host.fetch`, no login) for the chosen
//!   product over the map view (or the selected features) and months. It draws the granules'
//!   footprints as the layer `OPERA footprints`, whose attributes are the plugin's memory between
//!   calls (a plugin keeps nothing): granule UR, date, tile, collection concept id and layers.
//! - The **Granules** tree lists them by date. Choosing a granule (or a date) selects its
//!   footprints and lists their layers (bands) in the **Layer** tree.
//! - **Display** shows the selected footprints' granules (one tile layer each) through
//!   [titiler-cmr](https://github.com/developmentseed/titiler-cmr), which reads the
//!   Cloud-Optimized GeoTIFFs with Earthdata credentials of its own, so RustGIS needs none.
//!
//! Every command also takes its parameters from scripts and `rustgis-cli`, which have no panes:
//!
//! ```sh
//! rustgis-cli run --plugin dist/nasa-opera \
//!   --cmd 'plugin:nasa-opera/search={"product": "OPERA_L3_DSWX-HLS_V1", "bbox": [-119.9, 35.9, -119.4, 36.2], "start": "2023-04-01", "end": "2023-04-30"}' \
//!   --cmd 'plugin:nasa-opera/display={"date": "2023-04-01", "band": "B01_WTR"}' \
//!   --cmd 'share.exportMap={"path": "tulare.png"}'
//! ```

mod opera;

use opera::{Granule, PRODUCTS, Product};
use rustgis_plugin::{Node, Panel, Request, Value, host, json};

/// The footprints layer: the search results, kept on the map.
pub const FOOTPRINTS: &str = "OPERA footprints";
/// The id prefix of the plugin's tile layers, so Clear finds them.
pub const TILE_ID: &str = "opera-";
/// The most granules one Display shows.
const MAX_DISPLAY: usize = 25;

fn handle(req: Request) -> Result<Value, String> {
    match req.id.as_str() {
        "panel-state" => state(&req, None),
        "search" => search(&req),
        "choose-granule" => choose(&req),
        "sync-dates" => sync_dates(&req),
        "display" => display(&req, false),
        "display-all" => display(&req, true),
        "zoom-results" => {
            if !has_footprints()? {
                return Err("Search first: there are no OPERA footprints on the map.".into());
            }
            host::run("layer.zoomTo", json!({"layer": FOOTPRINTS}))?;
            state(&req, None)
        }
        "clear" => clear(&req),
        other => Err(format!("NASA OPERA has no command '{other}'")),
    }
}

rustgis_plugin::export!(handle);

/// The chosen product: the `product` parameter, else the panel's dropdown, else DSWx-HLS.
fn product(req: &Request) -> Result<&'static Product, String> {
    let name = req.str("product").or_else(|| req.panel_str("product"));
    match name {
        Some(n) => opera::product(n).ok_or_else(|| format!("'{n}' isn't an OPERA product this plugin knows")),
        None => Ok(&PRODUCTS[0]),
    }
}

/// What `host.fetch` answers, with only what the plugin reads: `text` is skipped unparsed.
#[derive(serde::Deserialize)]
struct FetchReply {
    ok: Option<Fetched>,
    error: Option<String>,
}

#[derive(serde::Deserialize)]
struct Fetched {
    pending: Option<bool>,
    json: Option<opera::Page>,
}

/// A CMR search page through `host.fetch`; `None` while the web app downloads it (RustGIS runs
/// the command again once it has it).
///
/// The SDK's `host::call` would build a JSON value of the whole answer, the document twice (as
/// `text` and as `json`), which costs more of a command's fuel than 50 granules' metadata leave.
/// This reads the reply into [`opera::Page`] instead, through the same host import.
fn fetch_page(url: &str) -> Result<Option<opera::Page>, String> {
    let bytes = raw_call("host.fetch", &json!({"url": url}))?;
    let reply: FetchReply = serde_json::from_slice(&bytes).map_err(|e| format!("CMR's answer couldn't be read: {e}"))?;
    if let Some(e) = reply.error {
        return Err(e);
    }
    let ok = reply.ok.ok_or("RustGIS didn't answer the fetch")?;
    if ok.pending.unwrap_or(false) {
        return Ok(None);
    }
    ok.json.map(Some).ok_or_else(|| format!("{url} didn't answer JSON"))
}

#[cfg(target_arch = "wasm32")]
#[link(wasm_import_module = "rustgis")]
unsafe extern "C" {
    #[link_name = "call"]
    fn rustgis_host_call(ptr: *const u8, len: usize) -> i32;
    #[link_name = "result"]
    fn rustgis_host_result(ptr: *mut u8);
}

/// A host request's raw reply (`{"ok": …}` or `{"error": …}`), as the ABI gives it.
#[cfg(target_arch = "wasm32")]
fn raw_call(op: &str, params: &Value) -> Result<Vec<u8>, String> {
    let req = json!({"op": op, "params": params}).to_string();
    // SAFETY: the host reads `len` bytes at `ptr`, which `req` owns for the call.
    let n = unsafe { rustgis_host_call(req.as_ptr(), req.len()) };
    let len = usize::try_from(n).map_err(|_| String::from("RustGIS didn't answer"))?;
    let mut buf: Vec<u8> = Vec::with_capacity(len);
    // SAFETY: the host writes exactly `len` bytes into `buf`'s capacity, then marked initialized.
    unsafe {
        rustgis_host_result(buf.as_mut_ptr());
        buf.set_len(len);
    }
    Ok(buf)
}

#[cfg(not(target_arch = "wasm32"))]
fn raw_call(_op: &str, _params: &Value) -> Result<Vec<u8>, String> {
    Err("host requests need RustGIS".into())
}

/// The area to search: `bbox`, the selected features' box, or the map view.
fn search_area(req: &Request) -> Result<[f64; 4], String> {
    if let Some(b) = req.params.get("bbox").and_then(Value::as_array) {
        let b: Vec<f64> = b.iter().filter_map(Value::as_f64).collect();
        return match b[..] {
            [w, s, e, n] if w < e && s < n => Ok(opera::clamp_bbox([w, s, e, n])),
            _ => Err("bbox must be [west, south, east, north]".into()),
        };
    }
    if req.panel_bool("use-selection").unwrap_or(false) {
        let mut all: Vec<[f64; 2]> = Vec::new();
        for l in host::layers()?.as_array().into_iter().flatten() {
            let name = l["name"].as_str().unwrap_or_default();
            if name == FOOTPRINTS || l["selected"].as_u64().unwrap_or(0) == 0 {
                continue;
            }
            let fc = host::features(name, true)?;
            for f in fc["features"].as_array().into_iter().flatten() {
                all.extend(rustgis_plugin::geojson::positions(&f["geometry"]));
            }
        }
        return rustgis_plugin::geojson::bbox(&all)
            .map(opera::clamp_bbox)
            .ok_or_else(|| "Select some features to search their area, or untick 'Search the selected features' area'.".into());
    }
    let e = host::run("map.extent", json!({}))?;
    let e: Vec<f64> = e["extent"].as_array().into_iter().flatten().filter_map(Value::as_f64).collect();
    match e[..] {
        [w, s, e, n] => Ok(opera::clamp_bbox([w, s, e, n])),
        _ => Err("RustGIS didn't give the map's extent".into()),
    }
}

/// The time window: `start`/`end`, else the panel's dates; `None` for the latest granules.
fn search_dates(req: &Request) -> Result<Option<(String, String)>, String> {
    if req.bool("latest") == Some(true) {
        if req.str("start").is_some() || req.str("end").is_some() {
            return Err("Give either latest or start and end, not both.".into());
        }
        return Ok(None);
    }
    let given = (req.str("start"), req.str("end"));
    let (start, end) = if given == (None, None) { (req.panel_str("start"), req.panel_str("end")) } else { given };
    match (start, end) {
        (Some(s), Some(e)) => {
            let (s, e) = (opera::parse_date(s)?, opera::parse_date(e)?);
            if s > e {
                return Err("start comes after end".into());
            }
            Ok(Some((opera::fmt_date(s), opera::fmt_date(e))))
        }
        (Some(_), None) | (None, Some(_)) => Err("Pick both a start and an end date, or clear both for the latest granules.".into()),
        (None, None) => Ok(None),
    }
}

/// Keeps the panel's two dates and its month range in step: moving the range sets the dates to
/// its first and last days; picking a date moves the range to the dates' months. The range spans
/// 2014-01 to 2030-12, so a date outside it keeps its value while the range's handle waits at
/// that end.
fn sync_dates(req: &Request) -> Result<Value, String> {
    let (status, panel) = sync_panel(req.changed("months"), req.panel_range("months"), req.panel_str("start"), req.panel_str("end"))?;
    Ok(panel.text("status", status).answer(json!({})))
}

/// What `sync-dates` answers: the status, and the dates or the range it sets.
fn sync_panel(range_moved: bool, range: Option<(i64, i64)>, start: Option<&str>, end: Option<&str>) -> Result<(String, Panel), String> {
    let panel = Panel::new();
    if range_moved {
        return Ok(match range {
            Some((lo, hi)) => {
                let (s, e) = opera::range_window(lo, hi);
                (format!("Search from {s} to {e}."), panel.value("start", s).value("end", e))
            }
            // A range the request didn't give: no window rather than a guessed one.
            None => {
                ("No dates: Search finds the latest granules.".to_string(), panel.value("start", Value::Null).value("end", Value::Null))
            }
        });
    }
    Ok(match (start, end) {
        (Some(s), Some(e)) => {
            let (s, e) = (opera::parse_date(s)?, opera::parse_date(e)?);
            if s > e {
                ("The start date comes after the end date.".to_string(), panel)
            } else {
                let (lo, hi) = (opera::month_index(s), opera::month_index(e));
                (format!("Search from {} to {}.", opera::fmt_date(s), opera::fmt_date(e)), panel.value("months", json!([lo, hi])))
            }
        }
        (None, None) => ("No dates: Search finds the latest granules.".to_string(), panel),
        _ => ("Pick both a start and an end date, or clear both for the latest granules.".to_string(), panel),
    })
}

fn search(req: &Request) -> Result<Value, String> {
    let p = product(req)?;
    let bbox = search_area(req)?;
    let dates = search_dates(req)?;
    let limit = req.f64("limit").map(|l| l as i64).or_else(|| req.panel_i64("limit")).unwrap_or(25).clamp(1, 100) as usize;
    // Pages of PAGE granules keep each answer under host.fetch's 2 MB. All are asked for at
    // once, so the web app downloads them together before running the command again.
    let mut pages = Vec::new();
    let mut pending = false;
    for n in 1..=limit.div_ceil(opera::PAGE) {
        match fetch_page(&opera::cmr_search_url(p.short_name, bbox, dates.as_ref(), n))? {
            Some(page) => pages.push(page),
            None => pending = true,
        }
    }
    if pending {
        return Ok(json!({"pending": true}));
    }
    let hits = pages.first().map_or(0, |p| p.hits);
    let found: Vec<Granule> = pages.iter().flat_map(|p| &p.items).filter_map(Granule::from_umm).collect();
    let mut granules = opera::latest_versions(found);
    granules.truncate(limit);

    // The old results go once the new ones are in, so a failed search keeps them.
    if has_footprints()? {
        host::run("layer.remove", json!({"layer": FOOTPRINTS}))?;
    }
    let when = match &dates {
        Some((s, e)) => format!("{s} to {e}"),
        None => "the latest".to_string(),
    };
    if granules.is_empty() {
        let text = format!("No {} granules over this area, {when}.", p.label);
        host::notify(&text);
        let panel = Panel::new().text("status", text).options("granule", []).options("band", []).value("granule", Value::Null);
        return Ok(panel.answer(json!({"granules": 0, "hits": hits, "dates": []})));
    }
    let fc = rustgis_plugin::geojson::collection(granules.iter().map(|g| g.feature(p.short_name)).collect());
    host::add_layer(FOOTPRINTS, fc)?;
    // Hollow footprints, so the rasters under them stay visible.
    host::run("layer.color", json!({"layer": FOOTPRINTS, "color": "#ff450000"}))?;
    host::run("layer.outline", json!({"layer": FOOTPRINTS, "color": "#ff4500", "width": 1.5}))?;
    if req.params.get("bbox").is_some() {
        host::run("layer.zoomTo", json!({"layer": FOOTPRINTS}))?;
    }
    let text = format!("{} of {hits} {} granules, {when}.", granules.len(), p.label);
    host::notify(&text);
    // The newest date is chosen, ready for Display.
    let first = format!("date:{}", granules[0].date);
    let records: Vec<Value> = granules.iter().map(|g| g.record(p.short_name)).collect();
    select_chosen(Some(&first), &records)?;
    let panel = results_panel(req, &records, Some(&first)).text("status", text);
    Ok(panel.answer(json!({"granules": granules.len(), "hits": hits, "dates": dates_of(&records)})))
}

/// The footprints' attributes, newest first: one record per granule.
fn footprints(selected: bool) -> Result<Vec<Value>, String> {
    if !has_footprints()? {
        return Ok(Vec::new());
    }
    let fc = host::features(FOOTPRINTS, selected)?;
    let mut out: Vec<Value> = fc["features"].as_array().into_iter().flatten().map(|f| f["properties"].clone()).collect();
    newest_first(&mut out);
    Ok(out)
}

/// Sorts footprint records newest first, by acquisition time.
fn newest_first(records: &mut [Value]) {
    records.sort_by(|a, b| b["datetime"].as_str().cmp(&a["datetime"].as_str()));
}

/// Whether the map has the plugin's footprints layer: named `OPERA footprints` and with the
/// fields the plugin writes, so a layer of the user's that happens to share the name is left alone.
fn has_footprints() -> Result<bool, String> {
    Ok(host::layers()?.as_array().into_iter().flatten().any(is_footprints))
}

fn is_footprints(layer: &Value) -> bool {
    let fields: Vec<&str> = layer["fields"].as_array().into_iter().flatten().filter_map(|f| f["name"].as_str()).collect();
    layer["name"] == FOOTPRINTS && ["granule", "product", "concept_id", "bands"].iter().all(|f| fields.contains(f))
}

/// The distinct dates of `records`, newest first.
fn dates_of(records: &[Value]) -> Vec<String> {
    let mut dates: Vec<String> = Vec::new();
    for r in records {
        let d = r["date"].as_str().unwrap_or_default().to_string();
        if !dates.contains(&d) {
            dates.push(d);
        }
    }
    dates
}

/// The records a choice stands for: a granule, or every granule of a `date:` option.
fn chosen<'a>(choice: Option<&str>, records: &'a [Value]) -> Vec<&'a Value> {
    match choice {
        Some(c) => match c.strip_prefix("date:") {
            Some(d) => records.iter().filter(|r| r["date"] == d).collect(),
            None => records.iter().filter(|r| r["granule"] == c).collect(),
        },
        None => Vec::new(),
    }
}

/// Selects the chosen footprints on the map.
fn select_chosen(choice: Option<&str>, records: &[Value]) -> Result<(), String> {
    let Some(c) = choice else { return Ok(()) };
    let (field, value) = match c.strip_prefix("date:") {
        Some(d) => ("date", d),
        None => ("granule", c),
    };
    if records.is_empty() {
        return Ok(());
    }
    let clause = format!("{field} = '{}'", value.replace('\'', "''"));
    host::run("select.byAttributes", json!({"layer": FOOTPRINTS, "where": clause, "mode": "new"}))?;
    Ok(())
}

/// The granule dropdown's options: each date (all its granules together), then its granules.
fn granule_options(records: &[Value]) -> Vec<Node> {
    let mut options: Vec<Node> = Vec::new();
    for d in dates_of(records) {
        let of_day: Vec<&Value> = records.iter().filter(|r| r["date"] == d.as_str()).collect();
        let n = of_day.len();
        options.push(Node::new(format!("date:{d}"), format!("{d} · all {n} granule{}", if n == 1 { "" } else { "s" })));
        for r in of_day {
            let ur = r["granule"].as_str().unwrap_or_default();
            options
                .push(Node::new(ur, format!("      {d} · {} · {}", r["tile"].as_str().unwrap_or("?"), r["time"].as_str().unwrap_or(""))));
        }
    }
    options
}

/// The granule dropdown and the layer dropdown for `choice`.
fn results_panel(req: &Request, records: &[Value], choice: Option<&str>) -> Panel {
    // With no choice (or a stale one), the bands of all the results: Display then uses the
    // footprints selected on the map, or says to choose.
    let mut picked = chosen(choice, records);
    if picked.is_empty() {
        picked = records.iter().collect();
    }
    let bands = bands_of(&picked);
    let product = picked.first().and_then(|r| r["product"].as_str()).and_then(opera::product);
    let band = req
        .panel_str("band")
        .filter(|b| bands.iter().any(|x| x == b))
        .map(str::to_string)
        .or_else(|| product.and_then(|p| p.default_band(&bands)))
        .or_else(|| bands.first().cloned());
    let can_show = !bands.is_empty();
    Panel::new()
        .options("granule", granule_options(records))
        .value("granule", choice.map_or(Value::Null, Value::from))
        .options("band", bands.iter().map(|b| Node::new(b.clone(), b.clone())))
        .value("band", band.map_or(Value::Null, Value::from))
        .enabled("display", can_show)
        .enabled("display-all", !records.is_empty())
        .enabled("zoom-results", !records.is_empty())
}

/// The band to show: an explicit `band` must be one of `bands`; the panel's falls back to the
/// product's default band, then the first.
fn pick_band(explicit: Option<&str>, panel: Option<&str>, bands: &[String], product: &opera::Product) -> Result<String, String> {
    if let Some(b) = explicit.filter(|b| !b.trim().is_empty()) {
        return if bands.iter().any(|x| x == b) {
            Ok(b.to_string())
        } else {
            Err(format!("The chosen granules have no layer {b}: they have {}.", bands.join(", ")))
        };
    }
    panel
        .filter(|b| bands.iter().any(|x| x == b))
        .map(str::to_string)
        .or_else(|| product.default_band(bands))
        .or_else(|| bands.first().cloned())
        .ok_or_else(|| format!("{} granules have no GeoTIFF layers to show (they are HDF5 or NetCDF).", product.label))
}

fn has_band(record: &Value, band: &str) -> bool {
    record["bands"].as_str().unwrap_or_default().split(',').any(|b| b == band)
}

/// The layers (band tokens) the records have, in their order, each once.
fn bands_of(records: &[&Value]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for r in records {
        for b in r["bands"].as_str().unwrap_or_default().split(',').filter(|b| !b.is_empty()) {
            if !out.iter().any(|x| x == b) {
                out.push(b.to_string());
            }
        }
    }
    out
}

/// The panel's state: the product dropdown, the month range and the results on the map.
fn state(req: &Request, status: Option<String>) -> Result<Value, String> {
    let p = product(req)?;
    let records = footprints(false)?;
    let choice = req.panel_str("granule").filter(|c| !chosen(Some(c), &records).is_empty()).map(str::to_string);
    let status = status.unwrap_or_else(|| {
        if records.is_empty() {
            format!("{}: {}\nPick the dates (or none for the latest), then Search.", p.label, p.description)
        } else {
            format!("{}: {} granules on the map.", p.label, records.len())
        }
    });
    let panel = results_panel(req, &records, choice.as_deref())
        .options("product", PRODUCTS.iter().map(|p| Node::new(p.short_name, format!("{} · {}", p.label, p.description))))
        .value("product", p.short_name)
        .slider("months", 0, opera::MONTHS - 1, &opera::month_labels())
        .text("status", status);
    Ok(panel.answer(json!({"product": p.short_name, "granules": records.len()})))
}

fn choose(req: &Request) -> Result<Value, String> {
    let records = footprints(false)?;
    let choice = match (req.str("granule"), req.str("date")) {
        (Some(g), _) => Some(g.to_string()),
        (None, Some(d)) => Some(format!("date:{d}")),
        (None, None) => req.panel_str("granule").map(str::to_string),
    };
    let picked = chosen(choice.as_deref(), &records);
    if picked.is_empty() {
        return Err("Choose a granule or a date from the search results.".into());
    }
    select_chosen(choice.as_deref(), &records)?;
    let n = picked.len();
    let text = if n == 1 {
        format!("{} chosen.", picked[0]["granule"].as_str().unwrap_or_default())
    } else {
        format!("{n} granules of {} chosen.", picked[0]["date"].as_str().unwrap_or_default())
    };
    let panel = results_panel(req, &records, choice.as_deref()).text("status", text);
    Ok(panel.answer(json!({"granules": n})))
}

/// The granules to show: all of them, the parameters', the selected footprints or the tree's.
fn to_display<'a>(req: &Request, records: &'a [Value], selected: &[Value], all: bool) -> Vec<&'a Value> {
    if all {
        return records.iter().collect();
    }
    if let Some(g) = req.str("granule") {
        return chosen(Some(g), records);
    }
    if let Some(d) = req.str("date") {
        return chosen(Some(&format!("date:{d}")), records);
    }
    if !selected.is_empty() {
        return records.iter().filter(|r| selected.iter().any(|s| s["granule"] == r["granule"])).collect();
    }
    chosen(req.panel_str("granule"), records)
}

fn display(req: &Request, all: bool) -> Result<Value, String> {
    let records = footprints(false)?;
    if records.is_empty() {
        return Err("Search first: there are no OPERA footprints on the map.".into());
    }
    let selected = footprints(true)?;
    let mut picked = to_display(req, &records, &selected, all);
    if picked.is_empty() {
        return Err(match (all, req.str("granule"), req.str("date")) {
            (false, Some(g), _) => format!("The search results have no granule {g}."),
            (false, None, Some(d)) => format!("The search results have no granule of {d}: they have {}.", dates_of(&records).join(", ")),
            _ => "Choose a granule or a date, or select footprints on the map.".into(),
        });
    }
    let product = picked[0]["product"].as_str().and_then(opera::product).ok_or("The footprints name no OPERA product")?;
    let band = pick_band(req.str("band"), req.panel_str("band"), &bands_of(&picked), product)?;
    // Only the granules that have the band count toward the cap.
    picked.retain(|r| has_band(r, &band));
    let skipped = picked.len().saturating_sub(MAX_DISPLAY);
    picked.truncate(MAX_DISPLAY);
    let water_only = req.bool("waterOnly").or_else(|| req.panel_bool("water-only")).unwrap_or(false);
    let style = opera::Style::for_band(product, &band, water_only, req.str("rescale"), req.str("colormap"));
    let opacity = req.f64("opacity").or_else(|| req.panel_i64("opacity").map(|o| o as f64)).unwrap_or(100.0).clamp(0.0, 100.0) / 100.0;

    let on_map = tile_ids()?;
    let mut shown = Vec::new();
    // Oldest first, so the newest granule ends on top.
    for r in picked.iter().rev() {
        let ur = r["granule"].as_str().unwrap_or_default();
        let concept = r["concept_id"].as_str().unwrap_or_default();
        let id = format!("{TILE_ID}{:016x}", opera::fnv1a(&format!("{ur}/{band}")));
        if on_map.contains(&id) {
            host::run("map.removeTiles", json!({"id": id}))?;
        }
        let name = format!("OPERA {} {band} {} {}", product.label, r["date"].as_str().unwrap_or(""), r["tile"].as_str().unwrap_or(""));
        let bounds: Vec<f64> = ["west", "south", "east", "north"].iter().filter_map(|k| r[*k].as_f64()).collect();
        let mut params = json!({
            "url": opera::tile_url(opera::TITILER_CMR, concept, ur, &band, &style),
            "kind": "raster", "name": name, "id": id, "tileSize": 512, "maxZoom": 18, "overlay": true,
            "opacity": opacity, "attribution": "NASA OPERA (JPL/NASA), via titiler-cmr",
        });
        if bounds.len() == 4 {
            params["bounds"] = json!(bounds);
        }
        host::run("map.addTiles", params)?;
        shown.push(name);
    }
    if shown.is_empty() {
        return Err(format!("None of the chosen granules has the layer {band}."));
    }
    // Footprints on top of the rasters.
    // ("already there" when they are on top.)
    let _ = host::run("layer.move", json!({"layer": FOOTPRINTS, "to": 0}));
    // The selection's highlight would tint the rasters just shown; the tree keeps the choice.
    if !selected.is_empty() {
        host::run("select.rows", json!({"layer": FOOTPRINTS, "rows": [], "mode": "new"}))?;
    }
    let mut text = format!("Showing {} {} layer(s) of {band}.", shown.len(), product.label);
    if skipped > 0 {
        text.push_str(&format!(" {skipped} more left out: at most {MAX_DISPLAY} at a time."));
    }
    host::notify(&text);
    let choice = req.panel_str("granule").map(str::to_string);
    let panel = results_panel(req, &records, choice.as_deref()).text("status", text).value("band", band.clone());
    Ok(panel.answer(json!({"layers": shown, "band": band})))
}

/// The ids of the plugin's tile layers on the map.
fn tile_ids() -> Result<Vec<String>, String> {
    let tiles = host::run("map.tiles", json!({}))?;
    Ok(tiles
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|t| t["id"].as_str())
        .filter(|id| id.starts_with(TILE_ID))
        .map(str::to_string)
        .collect())
}

fn clear(req: &Request) -> Result<Value, String> {
    let ids = tile_ids()?;
    for id in &ids {
        host::run("map.removeTiles", json!({"id": id}))?;
    }
    let had = has_footprints()?;
    if had {
        host::run("layer.remove", json!({"layer": FOOTPRINTS}))?;
    }
    let text = format!("Removed {} OPERA layer(s){}.", ids.len(), if had { " and the footprints" } else { "" });
    host::notify(&text);
    let v = state(req, Some(text))?;
    Ok(v)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rec(granule: &str, date: &str, bands: &str) -> Value {
        json!({"granule": granule, "date": date, "bands": bands, "product": "OPERA_L3_DSWX-HLS_V1"})
    }

    #[test]
    fn chooses_a_granule_or_a_date() {
        let rs = [rec("a", "2023-04-01", "B01_WTR"), rec("b", "2023-04-01", "B01_WTR"), rec("c", "2023-04-03", "B01_WTR")];
        assert_eq!(chosen(Some("b"), &rs).len(), 1);
        assert_eq!(chosen(Some("date:2023-04-01"), &rs).len(), 2);
        assert!(chosen(Some("date:2020-01-01"), &rs).is_empty());
        assert!(chosen(None, &rs).is_empty());
        assert_eq!(dates_of(&rs), ["2023-04-01", "2023-04-03"]);
    }

    #[test]
    fn sorts_newest_first() {
        let mut rs = vec![
            json!({"datetime": "2023-04-01T18:00:00Z"}),
            json!({"datetime": "2023-04-30T18:00:00Z"}),
            json!({"datetime": "2023-04-14T19:00:00Z"}),
        ];
        newest_first(&mut rs);
        let order: Vec<&str> = rs.iter().map(|r| r["datetime"].as_str().unwrap()).collect();
        assert_eq!(order, ["2023-04-30T18:00:00Z", "2023-04-14T19:00:00Z", "2023-04-01T18:00:00Z"]);
    }

    #[test]
    fn picks_a_band() {
        let rtc = opera::product("OPERA_L2_RTC-S1_V1").unwrap();
        let hh: Vec<String> = vec!["HH".into(), "HV".into()];
        assert_eq!(pick_band(None, None, &hh, rtc).unwrap(), "HH");
        assert_eq!(pick_band(None, Some("VV"), &hh, rtc).unwrap(), "HH", "a panel band the granules lack falls back");
        assert_eq!(pick_band(Some("HV"), None, &hh, rtc).unwrap(), "HV");
        assert!(pick_band(Some("VV"), None, &hh, rtc).is_err(), "an explicit band they lack is refused");
        assert_eq!(pick_band(Some(""), None, &hh, rtc).unwrap(), "HH");
        let cslc = opera::product("OPERA_L2_CSLC-S1_V1").unwrap();
        assert!(pick_band(None, None, &[], cslc).is_err());
        assert!(has_band(&json!({"bands": "B01_WTR,B02_BWTR"}), "B02_BWTR"));
        assert!(!has_band(&json!({"bands": "B01_WTR"}), "B01"));
    }

    #[test]
    fn knows_its_footprints_layer() {
        let fields = json!([{"name": "granule"}, {"name": "product"}, {"name": "concept_id"}, {"name": "bands"}, {"name": "date"}]);
        assert!(is_footprints(&json!({"name": FOOTPRINTS, "fields": fields})));
        assert!(!is_footprints(&json!({"name": FOOTPRINTS, "fields": [{"name": "name"}]})), "a user's layer of that name");
        assert!(!is_footprints(&json!({"name": "Other", "fields": fields})));
    }

    #[test]
    fn the_product_dropdown_lists_every_product() {
        let m: Value = serde_json::from_str(include_str!("../plugin.json")).unwrap();
        let declared: Vec<(String, String)> = m["panel"]["items"][0]["options"]
            .as_array()
            .unwrap()
            .iter()
            .map(|o| (o["id"].as_str().unwrap().to_string(), o["label"].as_str().unwrap().to_string()))
            .collect();
        let products: Vec<(String, String)> =
            PRODUCTS.iter().map(|p| (p.short_name.to_string(), format!("{} · {}", p.label, p.description))).collect();
        assert_eq!(declared, products);
        assert_eq!(m["panel"]["items"][3]["max"], opera::MONTHS - 1, "the month range spans opera::MONTHS");
    }

    #[test]
    fn granule_options_list_dates_then_granules() {
        let rs = [rec("a", "2023-04-30", "B01_WTR"), rec("b", "2023-04-30", "B01_WTR"), rec("c", "2023-04-14", "B01_WTR")];
        let ids: Vec<String> = granule_options(&rs).into_iter().map(|n| n.id).collect();
        assert_eq!(ids, ["date:2023-04-30", "a", "b", "date:2023-04-14", "c"]);
    }

    #[test]
    fn sync_keeps_the_dates_and_the_range_in_step() {
        let answer = |r: (String, Panel)| (r.0, Value::from(r.1)["values"].clone());
        let (text, values) = answer(sync_panel(true, Some((111, 112)), None, None).unwrap());
        assert_eq!(text, "Search from 2023-04-01 to 2023-05-31.");
        assert_eq!(values, json!({"start": "2023-04-01", "end": "2023-05-31"}));
        let (text, values) = answer(sync_panel(true, None, Some("2023-04-01"), None).unwrap());
        assert!(text.contains("latest"), "a range missing from the request clears the dates: {text}");
        assert_eq!(values, json!({"start": null, "end": null}));
        let (_, values) = answer(sync_panel(false, None, Some("2023-04-14"), Some("2024-01-02")).unwrap());
        assert_eq!(values, json!({"months": [111, 120]}));
        let (text, _) = answer(sync_panel(false, None, Some("2023-04-14"), None).unwrap());
        assert!(text.starts_with("Pick both"), "{text}");
        let (text, values) = answer(sync_panel(false, None, Some("2023-05-01"), Some("2023-04-01")).unwrap());
        assert_eq!((text.as_str(), values), ("The start date comes after the end date.", json!({})));
        assert!(sync_panel(false, None, Some("2023-02-30"), Some("2023-04-01")).is_err());
    }

    #[test]
    fn lists_each_band_once() {
        let a = rec("a", "d", "B01_WTR,B02_BWTR");
        let b = rec("b", "d", "B02_BWTR,B03_CONF");
        assert_eq!(bands_of(&[&a, &b]), ["B01_WTR", "B02_BWTR", "B03_CONF"]);
    }
}
