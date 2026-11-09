//! OPERA products, NASA CMR search results and titiler-cmr tile addresses: plain functions with
//! no `host::` calls, so `cargo test` covers them.

use rustgis_plugin::{Value, json};
use serde::Deserialize;

/// NASA CMR's public search API.
pub const CMR: &str = "https://cmr.earthdata.nasa.gov/search";
/// The titiler-cmr deployment that renders OPERA tiles (it holds the Earthdata login).
pub const TITILER_CMR: &str = "https://titiler-cmr.opengeos.org";

/// An OPERA product: a CMR collection.
pub struct Product {
    /// CMR `short_name`.
    pub short_name: &'static str,
    /// Short label.
    pub label: &'static str,
    /// One line on what it is.
    pub description: &'static str,
    /// The layers to show first, in order of preference.
    pub default_bands: &'static [&'static str],
}

impl Product {
    /// The first of the product's preferred layers that `bands` has.
    pub fn default_band(&self, bands: &[String]) -> Option<String> {
        self.default_bands.iter().find(|d| bands.iter().any(|b| b == *d)).map(|d| d.to_string())
    }
}

/// The OPERA products the plugin searches. The first is the default.
pub const PRODUCTS: [Product; 9] = [
    Product { short_name: "OPERA_L3_DSWX-HLS_V1", label: "DSWx-HLS", description: "surface water, optical", default_bands: &["B01_WTR"] },
    Product { short_name: "OPERA_L3_DSWX-S1_V1", label: "DSWx-S1", description: "surface water, radar", default_bands: &["B01_WTR"] },
    Product {
        short_name: "OPERA_L3_DIST-ALERT-HLS_V1",
        label: "DIST-ALERT",
        description: "disturbance alerts",
        default_bands: &["VEG-DIST-STATUS"],
    },
    Product {
        short_name: "OPERA_L3_DIST-ANN-HLS_V1",
        label: "DIST-ANN",
        description: "annual disturbance",
        default_bands: &["VEG-DIST-STATUS"],
    },
    Product { short_name: "OPERA_L2_RTC-S1_V1", label: "RTC-S1", description: "SAR backscatter", default_bands: &["VV", "HH"] },
    Product {
        short_name: "OPERA_L2_RTC-S1-STATIC_V1",
        label: "RTC-S1 static",
        description: "incidence angles, masks",
        default_bands: &["local_incidence_angle"],
    },
    Product { short_name: "OPERA_L2_CSLC-S1_V1", label: "CSLC-S1", description: "SLC, footprints only", default_bands: &[] },
    Product { short_name: "OPERA_L2_CSLC-S1-STATIC_V1", label: "CSLC-S1 static", description: "footprints only", default_bands: &[] },
    Product { short_name: "OPERA_L3_DISP-S1_V1", label: "DISP-S1", description: "displacement, footprints only", default_bands: &[] },
];

/// A product by its CMR short name (any case).
pub fn product(short_name: &str) -> Option<&'static Product> {
    PRODUCTS.iter().find(|p| p.short_name.eq_ignore_ascii_case(short_name))
}

// ---------- Dates ----------

/// The month slider's first month (index 1); index 0 is "the latest granules".
pub const FIRST_YEAR: i64 = 2014;
/// The month slider's last index (December 2030).
pub const MONTHS: i64 = (2030 - FIRST_YEAR + 1) * 12;

/// A date as (year, month, day).
pub type Date = (i64, u32, u32);

/// Parses `YYYY-MM-DD` (a time after it is ignored).
pub fn parse_date(s: &str) -> Result<Date, String> {
    let bad = || format!("'{s}' isn't a date: write it as YYYY-MM-DD");
    let s = s.get(..10).ok_or_else(bad)?;
    let mut parts = s.split('-');
    let (Some(y), Some(m), Some(d), None) = (parts.next(), parts.next(), parts.next(), parts.next()) else {
        return Err(bad());
    };
    let (y, m, d): (i64, u32, u32) = (y.parse().map_err(|_| bad())?, m.parse().map_err(|_| bad())?, d.parse().map_err(|_| bad())?);
    if !(1..=12).contains(&m) || d == 0 || d > days_in_month(y, m) {
        return Err(bad());
    }
    Ok((y, m, d))
}

pub fn fmt_date((y, m, d): Date) -> String {
    format!("{y:04}-{m:02}-{d:02}")
}

pub fn days_in_month(y: i64, m: u32) -> u32 {
    match m {
        4 | 6 | 9 | 11 => 30,
        2 if (y % 4 == 0 && y % 100 != 0) || y % 400 == 0 => 29,
        2 => 28,
        _ => 31,
    }
}

/// The month at slider index `i` (from 1).
pub fn month_of(i: i64) -> (i64, u32) {
    let k = (i - 1).max(0);
    (FIRST_YEAR + k / 12, (k % 12) as u32 + 1)
}

/// The month slider's labels: "Latest", then every month.
pub fn month_labels() -> Vec<String> {
    let mut v = vec!["Latest".to_string()];
    v.extend((1..=MONTHS).map(|i| {
        let (y, m) = month_of(i);
        format!("{y}-{m:02}")
    }));
    v
}

/// The first and last day of `span` months from slider index `i`; `None` for index 0 (latest).
pub fn month_window(i: i64, span: i64) -> Option<(String, String)> {
    if i <= 0 {
        return None;
    }
    let (y, m) = month_of(i);
    let (y2, m2) = month_of((i + span - 1).min(MONTHS));
    Some((fmt_date((y, m, 1)), fmt_date((y2, m2, days_in_month(y2, m2)))))
}

// ---------- CMR ----------

/// `[west, south, east, north]` kept within longitude and latitude.
pub fn clamp_bbox([w, s, e, n]: [f64; 4]) -> [f64; 4] {
    [w.clamp(-180.0, 180.0), s.clamp(-90.0, 90.0), e.clamp(-180.0, 180.0), n.clamp(-90.0, 90.0)]
}

/// Percent-encodes a query value.
pub fn encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// Rounds a coordinate to 4 decimals (about 10 m), so the same view asks the same address.
fn coord(x: f64) -> String {
    let s = format!("{x:.4}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" { "0".into() } else { s.into() }
}

/// Granules per CMR page: about 0.7 MB of metadata at most, under `host.fetch`'s 2 MB.
pub const PAGE: usize = 25;

/// The address of page `page` (from 1) of a CMR granule search, newest first.
pub fn cmr_search_url(short_name: &str, bbox: [f64; 4], dates: Option<&(String, String)>, page: usize) -> String {
    let b: Vec<String> = bbox.iter().map(|x| coord(*x)).collect();
    let mut url = format!(
        "{CMR}/granules.umm_json?short_name={}&bounding_box={}&page_size={PAGE}&page_num={page}&sort_key=-start_date",
        encode(short_name),
        encode(&b.join(","))
    );
    if let Some((s, e)) = dates {
        url.push_str(&format!("&temporal={}", encode(&format!("{s}T00:00:00Z,{e}T23:59:59Z"))));
    }
    url
}

/// A granule found in CMR.
#[derive(Debug, Clone, PartialEq)]
pub struct Granule {
    pub ur: String,
    pub concept_id: String,
    /// Start of acquisition, as CMR gives it.
    pub datetime: String,
    /// Its date, `YYYY-MM-DD`.
    pub date: String,
    /// Its time of day, `HH:MM`.
    pub time: String,
    /// The tile or burst id (T11SKA, T064-135524-IW2…).
    pub tile: String,
    /// Its outline: one closed ring of [lon, lat] per part (most granules have one).
    pub rings: Vec<Vec<[f64; 2]>>,
    /// The layer tokens of its GeoTIFFs (B01_WTR, VV…).
    pub bands: Vec<String>,
}

/// A page of CMR's `granules.umm_json`, with only what the plugin reads.
#[derive(Debug, Default, Deserialize)]
pub struct Page {
    #[serde(default)]
    pub hits: u64,
    #[serde(default)]
    pub items: Vec<Item>,
}

/// A UMM-G item.
#[derive(Debug, Default, Deserialize)]
pub struct Item {
    #[serde(default)]
    pub meta: Meta,
    #[serde(default)]
    pub umm: Umm,
}

#[derive(Debug, Default, Deserialize)]
pub struct Meta {
    #[serde(rename = "collection-concept-id")]
    pub collection_concept_id: Option<String>,
    #[serde(rename = "native-id")]
    pub native_id: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Umm {
    #[serde(rename = "GranuleUR")]
    pub granule_ur: Option<String>,
    #[serde(default)]
    pub temporal_extent: Temporal,
    #[serde(default)]
    pub spatial_extent: Spatial,
    #[serde(default)]
    pub related_urls: Vec<RelatedUrl>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Temporal {
    pub range_date_time: Option<Range>,
    pub single_date_time: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Range {
    pub beginning_date_time: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Spatial {
    #[serde(default)]
    pub horizontal_spatial_domain: Horizontal,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Horizontal {
    #[serde(default)]
    pub geometry: Geometry,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Geometry {
    #[serde(default, rename = "GPolygons")]
    pub gpolygons: Vec<GPolygon>,
    #[serde(default)]
    pub bounding_rectangles: Vec<Rect>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct GPolygon {
    #[serde(default)]
    pub boundary: Boundary,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Boundary {
    #[serde(default)]
    pub points: Vec<Point>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Point {
    pub longitude: f64,
    pub latitude: f64,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Rect {
    pub west_bounding_coordinate: f64,
    pub south_bounding_coordinate: f64,
    pub east_bounding_coordinate: f64,
    pub north_bounding_coordinate: f64,
}

#[derive(Debug, Default, Deserialize)]
pub struct RelatedUrl {
    #[serde(rename = "URL")]
    pub url: Option<String>,
}

impl Granule {
    /// Reads a UMM-G item of CMR's `granules.umm_json`.
    pub fn from_umm(item: &Item) -> Option<Granule> {
        let umm = &item.umm;
        let ur = umm.granule_ur.clone().or_else(|| item.meta.native_id.clone())?;
        let concept_id = item.meta.collection_concept_id.clone()?;
        let t = &umm.temporal_extent;
        let datetime = t
            .range_date_time
            .as_ref()
            .and_then(|r| r.beginning_date_time.clone())
            .or_else(|| t.single_date_time.clone())
            .unwrap_or_default();
        let date = datetime.get(..10).unwrap_or("").to_string();
        let time = datetime.get(11..16).unwrap_or("").to_string();
        let rings = rings_of(&umm.spatial_extent.horizontal_spatial_domain.geometry)?;
        let tile = ur.split('_').nth(3).unwrap_or("").to_string();
        let mut bands: Vec<String> = Vec::new();
        for url in umm.related_urls.iter().filter_map(|u| u.url.as_deref()) {
            if !url.starts_with("https://") {
                continue;
            }
            if let Some(b) = band_of(url, &ur)
                && !bands.contains(&b)
            {
                bands.push(b);
            }
        }
        Some(Granule { ur, concept_id, datetime, date, time, tile, rings, bands })
    }

    /// `[west, south, east, north]` of the outline.
    pub fn bounds(&self) -> [f64; 4] {
        let mut b = [f64::MAX, f64::MAX, f64::MIN, f64::MIN];
        for [x, y] in self.rings.iter().flatten() {
            b = [b[0].min(*x), b[1].min(*y), b[2].max(*x), b[3].max(*y)];
        }
        b
    }

    /// The footprint's attributes (what Display reads back from the map).
    pub fn record(&self, product: &str) -> Value {
        let [w, s, e, n] = self.bounds();
        json!({
            "granule": self.ur, "product": product, "concept_id": self.concept_id, "date": self.date,
            "time": self.time, "datetime": self.datetime, "tile": self.tile, "bands": self.bands.join(","),
            "west": w, "south": s, "east": e, "north": n,
        })
    }

    /// The footprint as a GeoJSON feature.
    pub fn feature(&self, product: &str) -> Value {
        let geometry = match &self.rings[..] {
            [ring] => json!({"type": "Polygon", "coordinates": [ring]}),
            rings => json!({"type": "MultiPolygon", "coordinates": rings.iter().map(|r| vec![r]).collect::<Vec<_>>()}),
        };
        json!({"type": "Feature", "geometry": geometry, "properties": self.record(product)})
    }
}

/// One granule per tile and acquisition: CMR lists reprocessed granules beside the ones they
/// replace, and their URs differ only from the production time on, so the greatest UR is the
/// newest. The order of `granules` is kept.
pub fn latest_versions(granules: Vec<Granule>) -> Vec<Granule> {
    let mut out: Vec<Granule> = Vec::new();
    for g in granules {
        match out.iter_mut().find(|o| o.tile == g.tile && o.datetime == g.datetime) {
            Some(o) if g.ur > o.ur => *o = g,
            Some(_) => {}
            None => out.push(g),
        }
    }
    out
}

/// The 64-bit FNV-1a hash of `s`: a short, stable tile layer id for a granule's layer.
pub fn fnv1a(s: &str) -> u64 {
    s.bytes().fold(0xcbf2_9ce4_8422_2325, |h, b| (h ^ u64::from(b)).wrapping_mul(0x0000_0100_0000_01b3))
}

/// The closed outlines of a UMM-G geometry: every polygon (a granule across the antimeridian
/// has two), else every bounding rectangle. `None` when it has neither.
fn rings_of(geo: &Geometry) -> Option<Vec<Vec<[f64; 2]>>> {
    let mut rings: Vec<Vec<[f64; 2]>> = Vec::new();
    for poly in &geo.gpolygons {
        let mut ring: Vec<[f64; 2]> = poly.boundary.points.iter().map(|p| [p.longitude, p.latitude]).collect();
        if ring.len() >= 3 {
            if ring.first() != ring.last() {
                ring.push(ring[0]);
            }
            rings.push(ring);
        }
    }
    if rings.is_empty() {
        for r in &geo.bounding_rectangles {
            let (w, s) = (r.west_bounding_coordinate, r.south_bounding_coordinate);
            let (e, n) = (r.east_bounding_coordinate, r.north_bounding_coordinate);
            rings.push(vec![[w, s], [e, s], [e, n], [w, n], [w, s]]);
        }
    }
    (!rings.is_empty()).then_some(rings)
}

/// The layer token of a GeoTIFF link: what follows `<granule UR>_` in its file name
/// (`B01_WTR`, `VV`, `VEG-DIST-STATUS`, `local_incidence_angle`). `None` for other files.
pub fn band_of(url: &str, ur: &str) -> Option<String> {
    let name = url.rsplit('/').next()?;
    let stem = name.strip_suffix(".tif").or_else(|| name.strip_suffix(".tiff"))?;
    let token = match stem.strip_prefix(ur).and_then(|r| r.strip_prefix('_')) {
        Some(t) => t,
        None => stem.rsplit('_').next()?,
    };
    (!token.is_empty()).then(|| token.to_string())
}

// ---------- Rendering ----------

/// How a layer is drawn: titiler-cmr's `colormap`, `colormap_name` and `rescale`.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Style {
    /// An explicit colormap, `{"value": [r, g, b, a]}` as JSON.
    pub colormap: Option<String>,
    pub colormap_name: Option<String>,
    pub rescale: Option<String>,
}

/// DSWx water classes (WTR, BWTR, WTR-1, WTR-2): not-water and fill transparent.
const DSWX_CLASSES: &str = r#"{"0":[0,0,0,0],"1":[0,0,255,255],"2":[135,206,250,255],"252":[0,255,255,255],"253":[128,128,128,255],"254":[0,0,128,255],"255":[0,0,0,0]}"#;
/// The same, with only open water (1) and partial surface water (2) shown.
const DSWX_WATER_ONLY: &str =
    r#"{"0":[0,0,0,0],"1":[0,0,255,255],"2":[135,206,250,255],"252":[0,0,0,0],"253":[0,0,0,0],"254":[0,0,0,0],"255":[0,0,0,0]}"#;
/// DIST status: 0 none (transparent), first/provisional/confirmed detections of low (<50%) and
/// high (≥50%) loss, finished low and high, and 255 no data.
const DIST_STATUS: &str = r#"{"0":[0,0,0,0],"1":[255,255,178,255],"2":[254,204,92,255],"3":[253,141,60,255],"4":[240,59,32,255],"5":[220,30,30,255],"6":[189,0,38,255],"7":[150,150,150,255],"8":[90,90,90,255],"255":[0,0,0,0]}"#;

impl Style {
    /// The style of `band` of `product`. `rescale` and `colormap` (a titiler colormap name)
    /// override the band's own; a categorical band keeps its classes unless one is given.
    pub fn for_band(product: &Product, band: &str, water_only: bool, rescale: Option<&str>, colormap: Option<&str>) -> Style {
        let b = band.to_ascii_uppercase();
        let dswx = product.short_name.contains("DSWX");
        let mut s = if dswx && b.contains("WTR") {
            Style { colormap: Some((if water_only { DSWX_WATER_ONLY } else { DSWX_CLASSES }).into()), ..Style::default() }
        } else if product.short_name.contains("DIST") && b.ends_with("DIST-STATUS") {
            Style { colormap: Some(DIST_STATUS.into()), ..Style::default() }
        } else if b.contains("DEM") {
            named("0,3000", "terrain")
        } else if matches!(b.as_str(), "VV" | "VH" | "HH" | "HV") {
            named("0,0.4", "gray")
        } else if b.contains("CONF") && dswx {
            named("0,100", "viridis")
        } else if b.contains("INCIDENCE_ANGLE") {
            named("0,90", "viridis")
        } else if b.contains("ANOM") {
            named("0,100", "ylorrd")
        } else if b.ends_with("-DATE") {
            named("0,1500", "viridis")
        } else {
            Style::default()
        };
        let rescale = rescale.filter(|r| !r.trim().is_empty());
        let colormap = colormap.filter(|c| !c.trim().is_empty());
        if rescale.is_some() || colormap.is_some() {
            s.colormap = None;
            if let Some(r) = rescale {
                s.rescale = Some(r.trim().to_string());
            }
            if let Some(c) = colormap {
                s.colormap_name = Some(c.trim().to_ascii_lowercase());
            }
        }
        s
    }
}

fn named(rescale: &str, name: &str) -> Style {
    Style { colormap: None, colormap_name: Some(name.into()), rescale: Some(rescale.into()) }
}

/// Escapes regex metacharacters, for titiler-cmr's `assets_regex`.
fn regex_escape(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        if "\\.^$|?*+()[]{}".contains(c) {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// The XYZ address of one granule's layer, through titiler-cmr (PNG, so classes can be
/// transparent; 512-pixel tiles).
pub fn tile_url(endpoint: &str, concept_id: &str, ur: &str, band: &str, style: &Style) -> String {
    let mut url = format!(
        "{endpoint}/rasterio/tiles/WebMercatorQuad/{{z}}/{{x}}/{{y}}.png?collection_concept_id={}&granule_ur={}&assets={}&assets_regex={}&tilesize=512",
        encode(concept_id),
        encode(ur),
        encode(band),
        encode(&regex_escape(band)),
    );
    if let Some(c) = &style.colormap {
        url.push_str(&format!("&colormap={}", encode(c)));
    }
    if let Some(n) = &style.colormap_name {
        url.push_str(&format!("&colormap_name={}", encode(n)));
    }
    if let Some(r) = &style.rescale {
        url.push_str(&format!("&rescale={}", encode(r)));
    }
    url
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_layer_tokens() {
        let ur = "OPERA_L3_DSWx-HLS_T11SKA_20230401T183929Z_20260315T235803Z_S2B_30_v1.1";
        let base = format!("https://archive.podaac.earthdata.nasa.gov/x/{ur}");
        assert_eq!(band_of(&format!("{base}_B01_WTR.tif"), ur).as_deref(), Some("B01_WTR"));
        assert_eq!(band_of(&format!("{base}_B05_WTR-1.tif"), ur).as_deref(), Some("B05_WTR-1"));
        assert_eq!(band_of(&format!("{base}_B01_WTR.tif.md5"), ur), None);
        assert_eq!(band_of(&format!("{base}_BROWSE.png"), ur), None);
        let rtc = "OPERA_L2_RTC-S1-STATIC_T004-006637-IW3_20140403_S1A_30_v1.0";
        assert_eq!(band_of(&format!("https://h/{rtc}_local_incidence_angle.tif"), rtc).as_deref(), Some("local_incidence_angle"));
        assert_eq!(band_of("https://h/other_name_VV.tif", rtc).as_deref(), Some("VV"));
    }

    #[test]
    fn parses_a_umm_granule() {
        let ur = "OPERA_L2_RTC-S1_T064-135524-IW2_20240101T000000Z_20240102T000000Z_S1A_30_v1.0";
        let item = json!({
            "meta": {"collection-concept-id": "C123-ASF"},
            "umm": {
                "GranuleUR": ur,
                "TemporalExtent": {"RangeDateTime": {"BeginningDateTime": "2024-01-01T13:44:23Z"}},
                "SpatialExtent": {"HorizontalSpatialDomain": {"Geometry": {"GPolygons": [{"Boundary": {"Points": [
                    {"Longitude": 1.0, "Latitude": 2.0}, {"Longitude": 3.0, "Latitude": 2.0}, {"Longitude": 3.0, "Latitude": 4.0}
                ]}}]}}},
                "RelatedUrls": [
                    {"URL": format!("https://d/{ur}_VV.tif")}, {"URL": format!("s3://d/{ur}_VV.tif")},
                    {"URL": format!("https://d/{ur}_VH.tif")}, {"URL": format!("https://d/{ur}.h5")}
                ]
            }
        });
        let g = Granule::from_umm(&serde_json::from_value(item).unwrap()).unwrap();
        assert_eq!(g.date, "2024-01-01");
        assert_eq!(g.time, "13:44");
        assert_eq!(g.tile, "T064-135524-IW2");
        assert_eq!(g.bands, ["VV", "VH"]);
        assert_eq!(g.rings, [vec![[1.0, 2.0], [3.0, 2.0], [3.0, 4.0], [1.0, 2.0]]], "the ring is closed");
        assert_eq!(g.feature("P")["geometry"]["type"], "Polygon");
        assert_eq!(g.bounds(), [1.0, 2.0, 3.0, 4.0]);
        let static_item = json!({
            "meta": {"collection-concept-id": "C1"},
            "umm": {"GranuleUR": "A_B_C_T1", "TemporalExtent": {"SingleDateTime": "2014-04-03T00:00:00Z"},
                "SpatialExtent": {"HorizontalSpatialDomain": {"Geometry": {"BoundingRectangles": [
                    {"WestBoundingCoordinate": -1.0, "SouthBoundingCoordinate": -2.0, "EastBoundingCoordinate": 1.0, "NorthBoundingCoordinate": 2.0}
                ]}}}}
        });
        let s = Granule::from_umm(&serde_json::from_value(static_item).unwrap()).unwrap();
        assert_eq!((s.date.as_str(), s.rings[0].len(), s.bands.len()), ("2014-04-03", 5, 0));
        let two = json!({
            "meta": {"collection-concept-id": "C1"},
            "umm": {"GranuleUR": "A_B_C_T1", "SpatialExtent": {"HorizontalSpatialDomain": {"Geometry": {"GPolygons": [
                {"Boundary": {"Points": [{"Longitude": 179.0, "Latitude": 0.0}, {"Longitude": 180.0, "Latitude": 0.0}, {"Longitude": 180.0, "Latitude": 1.0}]}},
                {"Boundary": {"Points": [{"Longitude": -180.0, "Latitude": 0.0}, {"Longitude": -179.0, "Latitude": 0.0}, {"Longitude": -180.0, "Latitude": 1.0}]}}
            ]}}}}
        });
        let t = Granule::from_umm(&serde_json::from_value(two).unwrap()).unwrap();
        assert_eq!(t.rings.len(), 2, "both parts are kept");
        assert_eq!(t.feature("P")["geometry"]["type"], "MultiPolygon");
        assert_eq!(t.bounds(), [-180.0, 0.0, 180.0, 1.0]);
    }

    #[test]
    fn keeps_the_latest_version_of_a_granule() {
        let g = |ur: &str, tile: &str, dt: &str| Granule {
            ur: ur.into(),
            concept_id: String::new(),
            datetime: dt.into(),
            date: String::new(),
            time: String::new(),
            tile: tile.into(),
            rings: Vec::new(),
            bands: Vec::new(),
        };
        let out = latest_versions(vec![
            g("X_T1_A_P1_v1.0", "T1", "a"),
            g("X_T2_A_P1_v1.0", "T2", "a"),
            g("X_T1_A_P2_v1.1", "T1", "a"),
            g("X_T1_B_P1_v1.0", "T1", "b"),
        ]);
        let urs: Vec<&str> = out.iter().map(|g| g.ur.as_str()).collect();
        assert_eq!(urs, ["X_T1_A_P2_v1.1", "X_T2_A_P1_v1.0", "X_T1_B_P1_v1.0"]);
        assert_ne!(fnv1a("a/B01_WTR"), fnv1a("a/B02_BWTR"));
    }

    #[test]
    fn months_and_dates() {
        assert_eq!(month_window(0, 3), None);
        assert_eq!(month_window(1, 1), Some(("2014-01-01".into(), "2014-01-31".into())));
        // 2024-02 is index (2024-2014)*12 + 2.
        assert_eq!(month_window(122, 1), Some(("2024-02-01".into(), "2024-02-29".into())));
        assert_eq!(month_window(122, 12), Some(("2024-02-01".into(), "2025-01-31".into())));
        assert_eq!(month_window(MONTHS, 6), Some(("2030-12-01".into(), "2030-12-31".into())));
        assert_eq!(month_labels().len() as i64, MONTHS + 1);
        assert_eq!(month_labels()[122], "2024-02");
        assert_eq!(parse_date("2023-04-30T10:00:00Z"), Ok((2023, 4, 30)));
        assert!(parse_date("2023-02-29").is_err());
        assert!(parse_date("2023-4-1").is_err());
    }

    #[test]
    fn builds_addresses() {
        let url =
            cmr_search_url("OPERA_L3_DSWX-HLS_V1", [-119.9, 35.9, -119.4, 36.2], Some(&("2023-04-01".into(), "2023-04-30".into())), 2);
        assert_eq!(
            url,
            "https://cmr.earthdata.nasa.gov/search/granules.umm_json?short_name=OPERA_L3_DSWX-HLS_V1\
             &bounding_box=-119.9%2C35.9%2C-119.4%2C36.2&page_size=25&page_num=2&sort_key=-start_date\
             &temporal=2023-04-01T00%3A00%3A00Z%2C2023-04-30T23%3A59%3A59Z"
        );
        let p = product("opera_l3_dswx-hls_v1").unwrap();
        let s = Style::for_band(p, "B01_WTR", false, None, None);
        let t = tile_url(TITILER_CMR, "C1", "UR_1.1", "B05_WTR-1", &s);
        assert!(t.starts_with("https://titiler-cmr.opengeos.org/rasterio/tiles/WebMercatorQuad/{z}/{x}/{y}.png?collection_concept_id=C1"));
        assert!(t.contains("&granule_ur=UR_1.1&assets=B05_WTR-1&assets_regex=B05_WTR-1&"));
        assert!(t.contains("&colormap=%7B%220%22%3A%5B0%2C0%2C0%2C0%5D"));
        assert_eq!(regex_escape("a.b(c)"), "a\\.b\\(c\\)");
    }

    #[test]
    fn styles() {
        let hls = product("OPERA_L3_DSWX-HLS_V1").unwrap();
        assert_eq!(Style::for_band(hls, "B01_WTR", true, None, None).colormap.as_deref(), Some(DSWX_WATER_ONLY));
        assert_eq!(Style::for_band(hls, "B10_DEM", false, None, None), named("0,3000", "terrain"));
        assert_eq!(
            Style::for_band(hls, "B01_WTR", false, Some(" "), Some("")).colormap.as_deref(),
            Some(DSWX_CLASSES),
            "blank overrides keep the classes"
        );
        let custom = Style::for_band(hls, "B01_WTR", false, Some("0,2"), Some("Blues"));
        assert_eq!(custom, named("0,2", "blues"));
        let rtc = product("OPERA_L2_RTC-S1_V1").unwrap();
        assert_eq!(Style::for_band(rtc, "VV", false, None, None), named("0,0.4", "gray"));
        assert_eq!(rtc.default_band(&["VH".into(), "VV".into()]).as_deref(), Some("VV"));
        assert_eq!(product("nope").map(|p| p.label), None);
    }
}
