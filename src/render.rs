use anyhow::{Context, Result};
use std::fs;
use std::path::Path;

use crate::osm::Trail;
use crate::tiles::TileMap;

pub fn render_png(tile_map: &TileMap, trails: &[Trail], output_path: &str) -> Result<()> {
    let w = tile_map.width;
    let h = tile_map.height;

    let svg_content = build_svg_overlay(tile_map, trails, w, h);

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

fn build_svg_overlay(tile_map: &TileMap, trails: &[Trail], w: u32, h: u32) -> String {
    let mut svg = format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}">"##,
    );

    for trail in trails {
        let points: Vec<(f64, f64)> = trail
            .geometry
            .0
            .iter()
            .map(|c| tile_map.project(c.x, c.y))
            .collect();

        if points.len() < 2 {
            continue;
        }

        let mut d = format!("M{:.1},{:.1}", points[0].0, points[0].1);
        for p in &points[1..] {
            d.push_str(&format!(" L{:.1},{:.1}", p.0, p.1));
        }

        svg.push_str(&format!(
            r##"<path d="{d}" fill="none" stroke="#FF4500" stroke-width="2.5" stroke-opacity="0.85" stroke-linecap="round" stroke-linejoin="round"/>"##,
        ));
    }

    // Attribution
    svg.push_str(&format!(
        r##"<text x="{}" y="{}" font-family="sans-serif" font-size="12" fill="white" text-anchor="end" opacity="0.8">© OpenStreetMap contributors</text>"##,
        w - 10,
        h - 10,
    ));

    // Title
    svg.push_str(
        r##"<rect x="10" y="10" width="380" height="40" rx="5" fill="black" fill-opacity="0.6"/>"##,
    );
    svg.push_str(
        r##"<text x="20" y="37" font-family="sans-serif" font-size="18" fill="white" font-weight="bold">Synclinal de Saou — Trail Coverage</text>"##,
    );

    svg.push_str("</svg>");
    svg
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

            // resvg outputs premultiplied alpha
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
