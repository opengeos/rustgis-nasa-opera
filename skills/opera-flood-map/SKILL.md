---
name: opera-flood-map
description: Map surface water, floods, land disturbance or SAR backscatter with NASA OPERA products through the NASA OPERA plugin. Use when the user asks for a flood map, water extent, before/after water comparison, wildfire or deforestation disturbance, or OPERA / DSWx / DIST / RTC data for a place and dates.
---

# Mapping with NASA OPERA

The NASA OPERA plugin (`plugin:nasa-opera/...`) searches NASA CMR for OPERA granules and shows
them as raster tile layers through titiler-cmr. No Earthdata login is needed.

## Products

| Product (`product`) | Use it for | Layer to show (`band`) |
|---|---|---|
| `OPERA_L3_DSWX-HLS_V1` | surface water from Landsat/Sentinel-2 (optical: clouds hide water) | `B01_WTR` |
| `OPERA_L3_DSWX-S1_V1` | surface water from Sentinel-1 radar (sees through clouds: best for floods) | `B01_WTR` |
| `OPERA_L3_DIST-ALERT-HLS_V1` | recent vegetation loss (fires, clearing, storms) | `VEG-DIST-STATUS` |
| `OPERA_L3_DIST-ANN-HLS_V1` | a year's vegetation loss | `VEG-DIST-STATUS` |
| `OPERA_L2_RTC-S1_V1` | SAR backscatter | `VV` (or `HH`) |

DSWx `B01_WTR` classes: 1 open water (blue), 2 partial surface water (light blue), 252 snow/ice,
253 cloud, 254 ocean. `waterOnly: true` hides everything but water.

## Steps

1. Find the place: `map.findPlace` or `map.search`, then note its `[west, south, east, north]`.
   Keep the box small (a city or a basin, under about 2 degrees across).
2. Search:
   `plugin:nasa-opera/search` with `{"product": "OPERA_L3_DSWX-S1_V1", "bbox": [w, s, e, n],
   "start": "YYYY-MM-DD", "end": "YYYY-MM-DD"}`. It draws the footprints (`OPERA footprints`) and
   answers `granules`, `hits` and `dates` (newest first). Without dates it finds the latest.
3. Show one date's granules together:
   `plugin:nasa-opera/display` with `{"date": "YYYY-MM-DD", "band": "B01_WTR", "waterOnly": true}`.
   Each granule becomes a tile layer named `OPERA <product> <band> <date> <tile>`.
4. For a before/after comparison, search and display a date before the event and one after; the
   newest layers are on top, so turn layers on and off (`map.setTiles` with `visible`) to compare.
5. `plugin:nasa-opera/clear` removes the footprints and every OPERA layer.

Optional `display` parameters: `rescale` (`"min,max"`) and `colormap` (a titiler colormap name such
as `viridis`, `terrain`, `gray`) for continuous layers, and `opacity` (0 to 100).
