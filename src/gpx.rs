use anyhow::{Context, Result};
use geo_types::LineString;
use std::fs::File;
use std::io::BufReader;
use std::path::Path;

use crate::config::*;

#[derive(Debug)]
pub struct Activity {
    pub name: String,
    pub tracks: Vec<LineString<f64>>,
}

pub fn load_activities(dir: &str) -> Result<Vec<Activity>> {
    let dir_path = Path::new(dir);
    if !dir_path.exists() {
        anyhow::bail!("Activities directory '{dir}' does not exist. Run 'synclinal sync' first.");
    }

    let mut activities = Vec::new();

    let mut entries: Vec<_> = std::fs::read_dir(dir_path)
        .with_context(|| format!("Failed to read directory {dir}"))?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|ext| ext == "gpx"))
        .collect();
    entries.sort_by_key(|e| e.file_name());

    for entry in entries {
        let path = entry.path();
        match parse_gpx(&path) {
            Ok(Some(activity)) => {
                let total_points: usize = activity.tracks.iter().map(|t| t.0.len()).sum();
                eprintln!(
                    "Loaded {} — {} tracks, {} points",
                    activity.name,
                    activity.tracks.len(),
                    total_points,
                );
                activities.push(activity);
            }
            Ok(None) => {
                eprintln!("Skipping {} — no tracks in bbox", path.display());
            }
            Err(e) => {
                eprintln!("Warning: failed to parse {}: {e}", path.display());
            }
        }
    }

    eprintln!("Loaded {} activities from {dir}", activities.len());
    Ok(activities)
}

fn parse_gpx(path: &Path) -> Result<Option<Activity>> {
    let file = File::open(path).with_context(|| format!("Failed to open {}", path.display()))?;
    let reader = BufReader::new(file);
    let gpx_data =
        gpx::read(reader).with_context(|| format!("Failed to parse {}", path.display()))?;

    let name = gpx_data
        .metadata
        .and_then(|m| m.name)
        .or_else(|| path.file_stem().map(|s| s.to_string_lossy().to_string()))
        .unwrap_or_default();

    let mut tracks = Vec::new();

    for track in &gpx_data.tracks {
        for segment in &track.segments {
            let coords: Vec<(f64, f64)> = segment
                .points
                .iter()
                .map(|p| (p.point().x(), p.point().y()))
                .collect();

            if coords.len() < 2 {
                continue;
            }

            // Check if any point falls within the bbox
            let in_bbox = coords.iter().any(|&(lon, lat)| {
                (BBOX_SOUTH..=BBOX_NORTH).contains(&lat) && (BBOX_WEST..=BBOX_EAST).contains(&lon)
            });

            if in_bbox {
                tracks.push(LineString::from(coords));
            }
        }
    }

    if tracks.is_empty() {
        return Ok(None);
    }

    Ok(Some(Activity { name, tracks }))
}
