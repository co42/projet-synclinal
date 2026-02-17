use anyhow::{Context, Result};
use serde_json::{Value, json};
use std::fs;
use std::path::Path;

use crate::grid::{GridConfig, GridResult};
use crate::matching::{COVERED_THRESHOLD, SegmentCoverage};
use crate::osm::Segment;

pub fn export_json(
    segments: &[Segment],
    coverage: &[SegmentCoverage],
    grid: &GridResult,
    output: &str,
) -> Result<()> {
    let segment_features = build_segment_features(segments, coverage, &grid.segment_cells);
    let cell_features = build_cell_features(grid);

    let data = json!({
        "bbox": [
            crate::config::BBOX_WEST,
            crate::config::BBOX_SOUTH,
            crate::config::BBOX_EAST,
            crate::config::BBOX_NORTH,
        ],
        "grid": {
            "cell_size_m": grid.config.cell_size_m,
            "origin": [grid.config.origin_lon, grid.config.origin_lat],
            "dlat": grid.config.dlat,
            "dlon": grid.config.dlon,
        },
        "segments": {
            "type": "FeatureCollection",
            "features": segment_features,
        },
        "cells": {
            "type": "FeatureCollection",
            "features": cell_features,
        },
    });

    if let Some(parent) = Path::new(output).parent() {
        fs::create_dir_all(parent)?;
    }
    let json_str = serde_json::to_string(&data).context("Failed to serialize data.json")?;
    fs::write(output, &json_str).context("Failed to write data.json")?;

    // Stats
    let total_seg_km: f64 = coverage.iter().map(|c| c.length_m).sum::<f64>() / 1000.0;
    let covered_seg_km: f64 = coverage
        .iter()
        .filter(|c| c.coverage_pct >= COVERED_THRESHOLD)
        .map(|c| c.length_m)
        .sum::<f64>()
        / 1000.0;
    let trail_cells = grid.cells.iter().filter(|c| c.has_trail).count();
    let visited_cells = grid
        .cells
        .iter()
        .filter(|c| c.has_trail && c.visited)
        .count();

    eprintln!(
        "Exported to {output}: {} segments ({:.1}/{:.1} km), {} cells ({}/{})",
        segments.len(),
        covered_seg_km,
        total_seg_km,
        grid.cells.iter().filter(|c| c.has_trail).count(),
        visited_cells,
        trail_cells,
    );

    Ok(())
}

fn build_segment_features(
    segments: &[Segment],
    coverage: &[SegmentCoverage],
    segment_cells: &[Vec<usize>],
) -> Vec<Value> {
    segments
        .iter()
        .enumerate()
        .map(|(i, seg)| {
            let cov = &coverage[i];
            let coords: Vec<Value> = seg.geometry.0.iter().map(|c| json!([c.x, c.y])).collect();

            json!({
                "type": "Feature",
                "geometry": {
                    "type": "LineString",
                    "coordinates": coords,
                },
                "properties": {
                    "id": i,
                    "length_m": (cov.length_m * 10.0).round() / 10.0,
                    "coverage_pct": (cov.coverage_pct * 100.0).round() / 100.0,
                    "covered": cov.coverage_pct >= COVERED_THRESHOLD,
                    "cells": segment_cells[i],
                },
            })
        })
        .collect()
}

fn build_cell_features(grid: &GridResult) -> Vec<Value> {
    grid.cells
        .iter()
        .filter(|c| c.has_trail)
        .map(|cell| {
            let polygon = cell_polygon(cell.row, cell.col, &grid.config);

            json!({
                "type": "Feature",
                "geometry": {
                    "type": "Polygon",
                    "coordinates": [polygon],
                },
                "properties": {
                    "id": cell.id,
                    "has_trail": cell.has_trail,
                    "visited": cell.visited,
                    "active": true,
                    "trail_km": (cell.trail_km * 1000.0).round() / 1000.0,
                    "covered_km": (cell.covered_km * 1000.0).round() / 1000.0,
                    "segment_ids": cell.segment_ids,
                },
            })
        })
        .collect()
}

fn cell_polygon(row: usize, col: usize, config: &GridConfig) -> Vec<Value> {
    let south = config.origin_lat + row as f64 * config.dlat;
    let north = south + config.dlat;
    let west = config.origin_lon + col as f64 * config.dlon;
    let east = west + config.dlon;

    vec![
        json!([west, south]),
        json!([east, south]),
        json!([east, north]),
        json!([west, north]),
        json!([west, south]),
    ]
}
