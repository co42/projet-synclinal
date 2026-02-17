use anyhow::{Context, Result};
use geo_types::LineString;
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::path::Path;

use crate::config::*;

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct Trail {
    pub id: i64,
    pub name: Option<String>,
    pub geometry: LineString<f64>,
}

#[derive(Deserialize)]
struct OverpassResponse {
    elements: Vec<OverpassElement>,
}

#[derive(Deserialize)]
struct OverpassElement {
    #[serde(rename = "type")]
    elem_type: String,
    id: i64,
    #[serde(default)]
    tags: Option<HashMap<String, String>>,
    #[serde(default)]
    geometry: Option<Vec<OverpassLatLon>>,
}

#[derive(Deserialize)]
struct OverpassLatLon {
    lat: f64,
    lon: f64,
}

pub fn clear_cache() {
    let path = Path::new(OSM_CACHE_PATH);
    if path.exists() {
        if let Err(e) = fs::remove_file(path) {
            eprintln!("Warning: failed to remove {OSM_CACHE_PATH}: {e}");
        } else {
            eprintln!("Cleared OSM cache");
        }
    }
}

pub async fn fetch_trails(client: &reqwest::Client) -> Result<Vec<Trail>> {
    let cache_path = Path::new(OSM_CACHE_PATH);
    if cache_path.exists() {
        eprintln!("Loading cached OSM data from {OSM_CACHE_PATH}");
        let data = fs::read_to_string(cache_path)?;
        return parse_overpass_json(&data);
    }

    let query = format!(
        r#"[out:json][timeout:60];
(
  way["highway"="path"]({s},{w},{n},{e});
  way["highway"="track"]({s},{w},{n},{e});
  way["highway"="footway"]({s},{w},{n},{e});
);
out geom;"#,
        s = BBOX_SOUTH,
        w = BBOX_WEST,
        n = BBOX_NORTH,
        e = BBOX_EAST,
    );

    eprintln!("Fetching trails from Overpass API...");
    let resp = client
        .post("https://overpass-api.de/api/interpreter")
        .form(&[("data", &query)])
        .send()
        .await
        .context("Failed to query Overpass API")?;

    let body = resp.text().await?;

    if let Some(parent) = cache_path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(cache_path, &body)?;
    eprintln!("Cached OSM data to {OSM_CACHE_PATH}");

    parse_overpass_json(&body)
}

fn parse_overpass_json(json: &str) -> Result<Vec<Trail>> {
    let response: OverpassResponse =
        serde_json::from_str(json).context("Failed to parse Overpass JSON")?;

    let trails: Vec<Trail> = response
        .elements
        .iter()
        .filter(|e| e.elem_type == "way")
        .filter_map(|elem| {
            let geom = elem.geometry.as_ref()?;
            if geom.len() < 2 {
                return None;
            }
            let coords: Vec<(f64, f64)> = geom.iter().map(|p| (p.lon, p.lat)).collect();
            let name = elem.tags.as_ref().and_then(|t| t.get("name").cloned());
            Some(Trail {
                id: elem.id,
                name,
                geometry: LineString::from(coords),
            })
        })
        .collect();

    eprintln!("Parsed {} trails from OSM", trails.len());
    Ok(trails)
}
