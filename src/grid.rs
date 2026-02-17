use crate::config::*;
use crate::matching::{COVERED_THRESHOLD, SegmentCoverage};
use crate::osm::Segment;

const EARTH_RADIUS_M: f64 = 6_371_000.0;
const DISCRETIZE_STEP_M: f64 = 20.0;

/// Grid metadata: origin, cell deltas in lat/lon, dimensions.
pub struct GridConfig {
    pub cell_size_m: f64,
    pub origin_lon: f64,
    pub origin_lat: f64,
    pub dlat: f64,
    pub dlon: f64,
    pub cols: usize,
    pub rows: usize,
}

/// Per-cell data.
pub struct Cell {
    pub id: usize,
    pub row: usize,
    pub col: usize,
    pub has_trail: bool,
    pub visited: bool,
    pub trail_km: f64,
    pub covered_km: f64,
    pub segment_ids: Vec<usize>,
}

pub struct GridResult {
    pub config: GridConfig,
    pub cells: Vec<Cell>,
    /// For each segment index, the set of cell IDs it passes through.
    pub segment_cells: Vec<Vec<usize>>,
}

/// Compute the grid overlay from segments and their coverage.
pub fn compute_grid(
    segments: &[Segment],
    coverage: &[SegmentCoverage],
    cell_size_m: f64,
) -> GridResult {
    // Convert cell size to lat/lon deltas at the center of the bbox
    let center_lat = (BBOX_SOUTH + BBOX_NORTH) / 2.0;
    let dlat = cell_size_m / EARTH_RADIUS_M * (180.0 / std::f64::consts::PI);
    let dlon = cell_size_m / (EARTH_RADIUS_M * center_lat.to_radians().cos())
        * (180.0 / std::f64::consts::PI);

    let cols = ((BBOX_EAST - BBOX_WEST) / dlon).ceil() as usize;
    let rows = ((BBOX_NORTH - BBOX_SOUTH) / dlat).ceil() as usize;

    let config = GridConfig {
        cell_size_m,
        origin_lon: BBOX_WEST,
        origin_lat: BBOX_SOUTH,
        dlat,
        dlon,
        cols,
        rows,
    };

    // Initialize cells
    let total_cells = cols * rows;
    let mut cells: Vec<Cell> = (0..total_cells)
        .map(|id| Cell {
            id,
            row: id / cols,
            col: id % cols,
            has_trail: false,
            visited: false,
            trail_km: 0.0,
            covered_km: 0.0,
            segment_ids: Vec::new(),
        })
        .collect();

    let mut segment_cells: Vec<Vec<usize>> = Vec::with_capacity(segments.len());

    for (seg_idx, seg) in segments.iter().enumerate() {
        let cov = &coverage[seg_idx];
        let is_covered = cov.coverage_pct >= COVERED_THRESHOLD;
        let points = discretize_linestring(&seg.geometry, DISCRETIZE_STEP_M);

        let mut seen_cells = std::collections::HashSet::new();

        for (lat, lon) in &points {
            if let Some(cell_id) = point_to_cell(*lat, *lon, &config) {
                seen_cells.insert(cell_id);
            }
        }

        let mut cell_ids: Vec<usize> = seen_cells.into_iter().collect();
        cell_ids.sort_unstable();

        // Distribute segment length evenly across its cells
        let km_per_cell = if !cell_ids.is_empty() {
            cov.length_m / 1000.0 / cell_ids.len() as f64
        } else {
            0.0
        };
        let covered_km_per_cell = if is_covered { km_per_cell } else { 0.0 };

        for &cell_id in &cell_ids {
            let cell = &mut cells[cell_id];
            cell.has_trail = true;
            cell.trail_km += km_per_cell;
            cell.covered_km += covered_km_per_cell;
            if is_covered {
                cell.visited = true;
            }
            if !cell.segment_ids.contains(&seg_idx) {
                cell.segment_ids.push(seg_idx);
            }
        }

        segment_cells.push(cell_ids);
    }

    // Only keep cells that have trails
    let trail_cells: Vec<&Cell> = cells.iter().filter(|c| c.has_trail).collect();
    let total_trail_cells = trail_cells.len();
    let visited_cells = trail_cells.iter().filter(|c| c.visited).count();
    eprintln!(
        "Grid: {}x{} cells, {} with trails, {} visited ({:.0}%)",
        cols,
        rows,
        total_trail_cells,
        visited_cells,
        if total_trail_cells > 0 {
            visited_cells as f64 / total_trail_cells as f64 * 100.0
        } else {
            0.0
        },
    );

    GridResult {
        config,
        cells,
        segment_cells,
    }
}

fn point_to_cell(lat: f64, lon: f64, config: &GridConfig) -> Option<usize> {
    if !(BBOX_SOUTH..=BBOX_NORTH).contains(&lat) || !(BBOX_WEST..=BBOX_EAST).contains(&lon) {
        return None;
    }
    let col = ((lon - config.origin_lon) / config.dlon).floor() as usize;
    let row = ((lat - config.origin_lat) / config.dlat).floor() as usize;
    if col >= config.cols || row >= config.rows {
        return None;
    }
    Some(row * config.cols + col)
}

fn discretize_linestring(geom: &geo_types::LineString<f64>, step_m: f64) -> Vec<(f64, f64)> {
    let coords = &geom.0;
    if coords.len() < 2 {
        return vec![];
    }

    let mut points = vec![(coords[0].y, coords[0].x)];
    let mut remaining = 0.0_f64;

    for window in coords.windows(2) {
        let (lat1, lon1) = (window[0].y, window[0].x);
        let (lat2, lon2) = (window[1].y, window[1].x);
        let seg_len = haversine_m(lat1, lon1, lat2, lon2);
        if seg_len < 1e-6 {
            continue;
        }

        let mut d = step_m - remaining;
        while d <= seg_len {
            let frac = d / seg_len;
            let lat = lat1 + (lat2 - lat1) * frac;
            let lon = lon1 + (lon2 - lon1) * frac;
            points.push((lat, lon));
            d += step_m;
        }
        remaining = seg_len - (d - step_m);
    }

    // Always include last point
    let last = coords.last().unwrap();
    points.push((last.y, last.x));

    points
}

fn haversine_m(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let (lat1, lon1, lat2, lon2) = (
        lat1.to_radians(),
        lon1.to_radians(),
        lat2.to_radians(),
        lon2.to_radians(),
    );
    let dlat = lat2 - lat1;
    let dlon = lon2 - lon1;
    let a = (dlat / 2.0).sin().powi(2) + lat1.cos() * lat2.cos() * (dlon / 2.0).sin().powi(2);
    EARTH_RADIUS_M * 2.0 * a.sqrt().asin()
}
