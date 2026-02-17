use anyhow::{Context, Result};
use serde::Deserialize;
use std::fs;
use std::path::Path;
use std::process::Command;

use crate::config::*;

const COORD_BUFFER: f64 = 0.15; // ~15km buffer around bbox for start coordinate check

#[derive(Deserialize)]
struct ActivityDetails {
    #[serde(rename = "activityName")]
    activity_name: Option<String>,
    #[serde(rename = "summaryDTO")]
    summary: Option<SummaryDTO>,
}

#[derive(Deserialize)]
struct SummaryDTO {
    #[serde(rename = "startLatitude")]
    start_latitude: Option<f64>,
    #[serde(rename = "startLongitude")]
    start_longitude: Option<f64>,
    distance: Option<f64>,
}

pub fn sync(activities_dir: &str, since: &str) -> Result<()> {
    fs::create_dir_all(activities_dir)?;

    let ids = list_activity_ids(since)?;
    eprintln!("Found {} activities since {since}", ids.len());

    let mut downloaded = 0;
    for (i, (id, date, activity_type)) in ids.iter().enumerate() {
        // Skip activities without GPS (strength, indoor)
        if is_indoor_activity(activity_type) {
            eprintln!(
                "[{}/{}] Skipping {} ({}) — indoor/no GPS",
                i + 1,
                ids.len(),
                id,
                activity_type
            );
            continue;
        }

        // Check if already downloaded
        let gpx_path = format!("{activities_dir}/{id}.gpx");
        if Path::new(&gpx_path).exists() {
            eprintln!("[{}/{}] Already have {id}.gpx", i + 1, ids.len());
            continue;
        }

        // Get details to check location
        let info = match get_activity_location(id)? {
            Some(info) => info,
            None => {
                eprintln!(
                    "[{}/{}] Skipping {id} ({date}) — no GPS coordinates",
                    i + 1,
                    ids.len()
                );
                continue;
            }
        };

        if !is_near_bbox(info.start_lat, info.start_lon) {
            eprintln!(
                "[{}/{}] Skipping {} — {} too far ({:.3}, {:.3})",
                i + 1,
                ids.len(),
                id,
                info.name,
                info.start_lat,
                info.start_lon
            );
            continue;
        }

        eprintln!(
            "[{}/{}] Downloading {id} — {} ({date}, {:.1} km)",
            i + 1,
            ids.len(),
            info.name,
            info.distance_km
        );
        download_gpx(id, &gpx_path)?;
        downloaded += 1;
    }

    eprintln!("Downloaded {downloaded} new GPX files to {activities_dir}");
    Ok(())
}

struct LocationInfo {
    name: String,
    start_lat: f64,
    start_lon: f64,
    distance_km: f64,
}

fn list_activity_ids(since: &str) -> Result<Vec<(String, String, String)>> {
    // garmin-cli list doesn't support date filtering or JSON output,
    // so we fetch a large batch and filter by date ourselves
    let output = Command::new("garmin")
        .args(["activities", "list", "-l", "200"])
        .output()
        .context("Failed to run 'garmin activities list'")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("garmin activities list failed: {stderr}");
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut activities = Vec::new();

    for line in stdout.lines() {
        let line = line.trim();
        // Parse table rows: "21887868116  2026-02-16 trail_running     15.61 km      2:08:53        -"
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 3 {
            continue;
        }
        // First field should be a numeric ID
        if !parts[0].chars().all(|c| c.is_ascii_digit()) {
            continue;
        }
        let id = parts[0].to_string();
        let date = parts[1].to_string();
        let activity_type = parts[2].to_string();

        if date.as_str() < since {
            break; // Activities are sorted newest first
        }

        activities.push((id, date, activity_type));
    }

    Ok(activities)
}

fn get_activity_location(id: &str) -> Result<Option<LocationInfo>> {
    let output = Command::new("garmin")
        .args(["activities", "get", id, "-f", "json"])
        .output()
        .with_context(|| format!("Failed to run 'garmin activities get {id}'"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("garmin activities get {id} failed: {stderr}");
    }

    let details: ActivityDetails = serde_json::from_slice(&output.stdout)
        .with_context(|| format!("Failed to parse activity {id} JSON"))?;

    let summary = match details.summary {
        Some(s) => s,
        None => return Ok(None),
    };

    let (lat, lon) = match (summary.start_latitude, summary.start_longitude) {
        (Some(lat), Some(lon)) => (lat, lon),
        _ => return Ok(None),
    };

    let name = details.activity_name.unwrap_or_default();
    let distance_km = summary.distance.unwrap_or(0.0) / 1000.0;

    Ok(Some(LocationInfo {
        name,
        start_lat: lat,
        start_lon: lon,
        distance_km,
    }))
}

fn download_gpx(id: &str, output_path: &str) -> Result<()> {
    let output = Command::new("garmin")
        .args(["activities", "download", id, "-t", "gpx", "-o", output_path])
        .output()
        .with_context(|| format!("Failed to download activity {id}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("garmin activities download {id} failed: {stderr}");
    }

    Ok(())
}

fn is_near_bbox(lat: f64, lon: f64) -> bool {
    (BBOX_SOUTH - COORD_BUFFER..=BBOX_NORTH + COORD_BUFFER).contains(&lat)
        && (BBOX_WEST - COORD_BUFFER..=BBOX_EAST + COORD_BUFFER).contains(&lon)
}

fn is_indoor_activity(activity_type: &str) -> bool {
    // Activity types may be truncated with "..." in table output
    let t = activity_type.trim_end_matches("...");
    t.starts_with("strength")
        || t.starts_with("indoor")
        || t.starts_with("treadmill")
        || t.starts_with("yoga")
        || t.starts_with("breathwork")
        || t == "0.00" // fallback: 0 distance sometimes appears as type
}
