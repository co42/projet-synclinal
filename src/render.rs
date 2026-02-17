use anyhow::{Context, Result};
use std::fs;
use std::path::Path;

use crate::matching::{COVERED_THRESHOLD, SegmentCoverage};
use crate::osm::Segment;
use crate::tiles::TileMap;

pub fn render_png(
    tile_map: &TileMap,
    segments: &[Segment],
    coverage: &[SegmentCoverage],
    output_path: &str,
) -> Result<()> {
    let w = tile_map.width;
    let h = tile_map.height;

    let svg_content = build_svg_overlay(tile_map, segments, coverage, w, h);

    let overlay = rasterize_svg(&svg_content)?;
    let composite = composite_images(&tile_map.image, &overlay);

    let output = Path::new(output_path);
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)?;
    }
    composite
        .save(output)
        .with_context(|| format!("Failed to save PNG to {output_path}"))?;

    eprintln!("Saved render to {output_path}");
    Ok(())
}

fn build_svg_overlay(
    tile_map: &TileMap,
    segments: &[Segment],
    coverage: &[SegmentCoverage],
    w: u32,
    h: u32,
) -> String {
    let mut svg = format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}">"##,
    );

    // Glow filter for covered segments
    svg.push_str(
        r##"<defs><filter id="glow"><feGaussianBlur stdDeviation="2.5" result="blur"/><feMerge><feMergeNode in="blur"/><feMergeNode in="SourceGraphic"/></feMerge></filter></defs>"##,
    );

    // Pass 1: uncovered segments — thin, semi-transparent white
    for (i, seg) in segments.iter().enumerate() {
        let cov = coverage.get(i).map(|c| c.coverage_pct).unwrap_or(0.0);
        if cov >= COVERED_THRESHOLD {
            continue;
        }
        if let Some(d) = linestring_to_path(&seg.geometry.0, tile_map) {
            svg.push_str(&format!(
                r##"<path d="{d}" fill="none" stroke="white" stroke-width="1.5" stroke-opacity="0.35" stroke-linecap="round" stroke-linejoin="round"/>"##,
            ));
        }
    }

    // Pass 2: covered segments — thick orange with glow
    for (i, seg) in segments.iter().enumerate() {
        let cov = coverage.get(i).map(|c| c.coverage_pct).unwrap_or(0.0);
        if cov < COVERED_THRESHOLD {
            continue;
        }
        if let Some(d) = linestring_to_path(&seg.geometry.0, tile_map) {
            svg.push_str(&format!(
                r##"<path d="{d}" fill="none" stroke="#FF4500" stroke-width="3" stroke-opacity="0.9" stroke-linecap="round" stroke-linejoin="round" filter="url(#glow)"/>"##,
            ));
        }
    }

    // Stats
    let total_km: f64 = coverage.iter().map(|c| c.length_m).sum::<f64>() / 1000.0;
    let covered_km: f64 = coverage
        .iter()
        .filter(|c| c.coverage_pct >= COVERED_THRESHOLD)
        .map(|c| c.length_m)
        .sum::<f64>()
        / 1000.0;
    let pct = if total_km > 0.0 {
        covered_km / total_km * 100.0
    } else {
        0.0
    };
    let stats_text = format!("{covered_km:.1} km / {total_km:.1} km ({pct:.0}%)");

    // Title bar
    svg.push_str(
        r##"<rect x="10" y="10" width="380" height="40" rx="5" fill="black" fill-opacity="0.6"/>"##,
    );
    svg.push_str(
        r##"<text x="20" y="37" font-family="sans-serif" font-size="18" fill="white" font-weight="bold">Synclinal de Saou — Trail Coverage</text>"##,
    );

    // Stats box
    let stats_box_w = 200;
    let stats_box_x = w - stats_box_w - 10;
    svg.push_str(&format!(
        r##"<rect x="{stats_box_x}" y="10" width="{stats_box_w}" height="40" rx="5" fill="black" fill-opacity="0.6"/>"##,
    ));
    svg.push_str(&format!(
        r##"<text x="{}" y="37" font-family="sans-serif" font-size="16" fill="white" text-anchor="end">{stats_text}</text>"##,
        w - 20,
    ));

    // Legend
    let legend_y = h - 50;
    svg.push_str(&format!(
        r##"<rect x="10" y="{legend_y}" width="180" height="40" rx="5" fill="black" fill-opacity="0.6"/>"##,
    ));
    svg.push_str(&format!(
        r##"<line x1="20" y1="{}" x2="45" y2="{}" stroke="#FF4500" stroke-width="3" stroke-linecap="round"/>"##,
        legend_y + 15, legend_y + 15,
    ));
    svg.push_str(&format!(
        r##"<text x="50" y="{}" font-family="sans-serif" font-size="12" fill="white">Covered</text>"##,
        legend_y + 19,
    ));
    svg.push_str(&format!(
        r##"<line x1="20" y1="{}" x2="45" y2="{}" stroke="white" stroke-width="1.5" stroke-opacity="0.5" stroke-linecap="round"/>"##,
        legend_y + 30, legend_y + 30,
    ));
    svg.push_str(&format!(
        r##"<text x="50" y="{}" font-family="sans-serif" font-size="12" fill="white">Uncovered</text>"##,
        legend_y + 34,
    ));

    // Attribution
    svg.push_str(&format!(
        r##"<text x="{}" y="{}" font-family="sans-serif" font-size="12" fill="white" text-anchor="end" opacity="0.8">© OpenStreetMap contributors</text>"##,
        w - 10,
        h - 10,
    ));

    svg.push_str("</svg>");
    svg
}

pub fn render_debug_png(tile_map: &TileMap, segments: &[Segment], output_path: &str) -> Result<()> {
    let w = tile_map.width;
    let h = tile_map.height;

    let mut svg = format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}">"##,
    );

    // Each segment in a different color
    let colors = [
        "#FF1493", "#00FFFF", "#7FFF00", "#FFD700", "#FF6347", "#1E90FF", "#FF00FF", "#00FF7F",
        "#FFA500", "#DA70D6", "#40E0D0", "#F0E68C", "#FF4500", "#ADFF2F", "#6495ED", "#FF69B4",
        "#00CED1", "#9ACD32", "#DC143C", "#48D1CC",
    ];
    for (i, seg) in segments.iter().enumerate() {
        let color = colors[i % colors.len()];
        if let Some(d) = linestring_to_path(&seg.geometry.0, tile_map) {
            svg.push_str(&format!(
                r##"<path d="{d}" fill="none" stroke="{color}" stroke-width="2" stroke-opacity="0.9" stroke-linecap="round" stroke-linejoin="round"/>"##,
            ));
        }
    }

    // Title
    svg.push_str(
        r##"<rect x="10" y="10" width="420" height="40" rx="5" fill="black" fill-opacity="0.6"/>"##,
    );
    svg.push_str(&format!(
        r##"<text x="20" y="37" font-family="sans-serif" font-size="18" fill="white" font-weight="bold">DEBUG — {} segments</text>"##,
        segments.len(),
    ));

    svg.push_str("</svg>");

    let overlay = rasterize_svg(&svg)?;
    let composite = composite_images(&tile_map.image, &overlay);

    let output = Path::new(output_path);
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)?;
    }
    composite
        .save(output)
        .with_context(|| format!("Failed to save PNG to {output_path}"))?;

    eprintln!(
        "Saved debug render to {output_path} ({} segments)",
        segments.len()
    );
    Ok(())
}

fn linestring_to_path(coords: &[geo_types::Coord<f64>], tile_map: &TileMap) -> Option<String> {
    let points: Vec<(f64, f64)> = coords.iter().map(|c| tile_map.project(c.x, c.y)).collect();
    if points.len() < 2 {
        return None;
    }
    let mut d = format!("M{:.1},{:.1}", points[0].0, points[0].1);
    for p in &points[1..] {
        d.push_str(&format!(" L{:.1},{:.1}", p.0, p.1));
    }
    Some(d)
}

fn rasterize_svg(svg_content: &str) -> Result<resvg::tiny_skia::Pixmap> {
    let opts = resvg::usvg::Options::default();
    let tree =
        resvg::usvg::Tree::from_str(svg_content, &opts).context("Failed to parse SVG overlay")?;

    let size = tree.size();
    let mut pixmap = resvg::tiny_skia::Pixmap::new(size.width() as u32, size.height() as u32)
        .context("Failed to create pixmap")?;

    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::default(),
        &mut pixmap.as_mut(),
    );
    Ok(pixmap)
}

fn composite_images(
    background: &image::RgbaImage,
    overlay: &resvg::tiny_skia::Pixmap,
) -> image::RgbaImage {
    let mut composite = background.clone();
    let overlay_data = overlay.data();
    let w = composite.width().min(overlay.width());
    let h = composite.height().min(overlay.height());

    for y in 0..h {
        for x in 0..w {
            let idx = (y * overlay.width() + x) as usize * 4;
            let sa = overlay_data[idx + 3] as u32;
            if sa == 0 {
                continue;
            }

            let sr = overlay_data[idx] as u32;
            let sg = overlay_data[idx + 1] as u32;
            let sb = overlay_data[idx + 2] as u32;

            let dst = composite.get_pixel(x, y);
            let inv_sa = 255 - sa;

            composite.put_pixel(
                x,
                y,
                image::Rgba([
                    (sr + dst[0] as u32 * inv_sa / 255).min(255) as u8,
                    (sg + dst[1] as u32 * inv_sa / 255).min(255) as u8,
                    (sb + dst[2] as u32 * inv_sa / 255).min(255) as u8,
                    (sa + dst[3] as u32 * inv_sa / 255).min(255) as u8,
                ]),
            );
        }
    }

    composite
}
