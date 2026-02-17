use geo_types::LineString;

use crate::gpx::Activity;
use crate::osm::Segment;

const MATCH_THRESHOLD_M: f64 = 10.0;
const TRAIL_STEP_M: f64 = 5.0;
const GPX_STEP_M: f64 = 2.0;
const EARTH_RADIUS_M: f64 = 6_371_000.0;
const GRID_CELL_M: f64 = 20.0;
pub const COVERED_THRESHOLD: f64 = 0.5;

#[derive(Debug)]
pub struct SegmentCoverage {
    pub coverage_pct: f64,
    pub length_m: f64,
}

pub fn compute_coverage(segments: &[Segment], activities: &[Activity]) -> Vec<SegmentCoverage> {
    let gps_index = build_gps_index(activities);
    eprintln!(
        "Built GPS index: {} cells, {} points",
        gps_index.cells.len(),
        gps_index.point_count,
    );

    let result: Vec<SegmentCoverage> = segments
        .iter()
        .map(|seg| {
            let length_m = linestring_length_m(&seg.geometry);
            let coverage_pct = segment_coverage(&seg.geometry, &gps_index);
            SegmentCoverage {
                coverage_pct,
                length_m,
            }
        })
        .collect();

    let total_km: f64 = result.iter().map(|c| c.length_m).sum::<f64>() / 1000.0;
    let covered_count = result
        .iter()
        .filter(|c| c.coverage_pct >= COVERED_THRESHOLD)
        .count();
    let covered_km: f64 = result
        .iter()
        .filter(|c| c.coverage_pct >= COVERED_THRESHOLD)
        .map(|c| c.length_m)
        .sum::<f64>()
        / 1000.0;
    eprintln!(
        "Coverage: {covered_count}/{} segments, {covered_km:.1}/{total_km:.1} km ({:.0}%)",
        result.len(),
        if total_km > 0.0 {
            covered_km / total_km * 100.0
        } else {
            0.0
        },
    );

    result
}

// --- Spatial grid index over interpolated GPS points ---

struct GpsIndex {
    cells: std::collections::HashMap<(i64, i64), Vec<(f64, f64)>>,
    point_count: usize,
}

impl GpsIndex {
    fn has_point_within(&self, lat: f64, lon: f64, radius_m: f64) -> bool {
        let (cx, cy) = lat_lon_to_cell(lat, lon);
        for dx in -1..=1 {
            for dy in -1..=1 {
                if let Some(pts) = self.cells.get(&(cx + dx, cy + dy)) {
                    for &(plat, plon) in pts {
                        if haversine_m(lat, lon, plat, plon) <= radius_m {
                            return true;
                        }
                    }
                }
            }
        }
        false
    }
}

fn lat_lon_to_cell(lat: f64, lon: f64) -> (i64, i64) {
    let lat_m = lat * EARTH_RADIUS_M.to_radians();
    let lon_m = lon * EARTH_RADIUS_M.to_radians() * lat.to_radians().cos();
    (
        (lat_m / GRID_CELL_M).floor() as i64,
        (lon_m / GRID_CELL_M).floor() as i64,
    )
}

fn build_gps_index(activities: &[Activity]) -> GpsIndex {
    let mut cells: std::collections::HashMap<(i64, i64), Vec<(f64, f64)>> =
        std::collections::HashMap::new();
    let mut point_count = 0_usize;

    for activity in activities {
        for track in &activity.tracks {
            let interpolated = discretize(track, GPX_STEP_M);
            for (lat, lon) in &interpolated {
                let cell = lat_lon_to_cell(*lat, *lon);
                cells.entry(cell).or_default().push((*lat, *lon));
            }
            point_count += interpolated.len();
        }
    }

    GpsIndex { cells, point_count }
}

// --- Coverage computation ---

fn segment_coverage(geom: &LineString<f64>, index: &GpsIndex) -> f64 {
    let sample_points = discretize(geom, TRAIL_STEP_M);
    if sample_points.is_empty() {
        return 0.0;
    }
    let matched = sample_points
        .iter()
        .filter(|&&(lat, lon)| index.has_point_within(lat, lon, MATCH_THRESHOLD_M))
        .count();
    matched as f64 / sample_points.len() as f64
}

fn discretize(geom: &LineString<f64>, step_m: f64) -> Vec<(f64, f64)> {
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

    points
}

// --- Geometry helpers ---

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

fn linestring_length_m(geom: &LineString<f64>) -> f64 {
    geom.0
        .windows(2)
        .map(|w| haversine_m(w[0].y, w[0].x, w[1].y, w[1].x))
        .sum()
}
