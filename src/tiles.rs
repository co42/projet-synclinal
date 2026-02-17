use anyhow::{Context, Result};
use image::{DynamicImage, GenericImage, RgbaImage};
use std::fs;
use std::path::Path;

use crate::config::*;

#[derive(Debug, Clone, Copy)]
pub enum Provider {
    OpenStreetMap,
    OpenTopoMap,
}

impl Provider {
    fn tile_url(&self, z: u32, x: u32, y: u32) -> String {
        match self {
            Self::OpenStreetMap => format!("https://tile.openstreetmap.org/{z}/{x}/{y}.png"),
            Self::OpenTopoMap => format!("https://tile.opentopomap.org/{z}/{x}/{y}.png"),
        }
    }

    fn cache_subdir(&self) -> &'static str {
        match self {
            Self::OpenStreetMap => "osm",
            Self::OpenTopoMap => "topo",
        }
    }

    fn name(&self) -> &'static str {
        match self {
            Self::OpenStreetMap => "OpenStreetMap",
            Self::OpenTopoMap => "OpenTopoMap",
        }
    }
}

pub fn clear_cache() {
    let path = Path::new(TILE_CACHE_DIR);
    if path.exists() {
        if let Err(e) = fs::remove_dir_all(path) {
            eprintln!("Warning: failed to remove {TILE_CACHE_DIR}: {e}");
        } else {
            eprintln!("Cleared tile cache");
        }
    }
}

pub struct TileMap {
    pub image: RgbaImage,
    pub width: u32,
    pub height: u32,
}

impl TileMap {
    /// Convert WGS84 (lon, lat) to pixel coordinates in the cropped image.
    pub fn project(&self, lon: f64, lat: f64) -> (f64, f64) {
        let x_frac = (lon - BBOX_WEST) / (BBOX_EAST - BBOX_WEST);
        let y_frac = (mercator_y(lat) - mercator_y(BBOX_NORTH))
            / (mercator_y(BBOX_SOUTH) - mercator_y(BBOX_NORTH));
        (x_frac * self.width as f64, y_frac * self.height as f64)
    }
}

pub async fn fetch_and_stitch(
    client: &reqwest::Client,
    zoom: u32,
    provider: Provider,
) -> Result<TileMap> {
    let x_min = lon_to_tile(BBOX_WEST, zoom);
    let x_max = lon_to_tile(BBOX_EAST, zoom);
    let y_min = lat_to_tile(BBOX_NORTH, zoom);
    let y_max = lat_to_tile(BBOX_SOUTH, zoom);

    let tiles_x = x_max - x_min + 1;
    let tiles_y = y_max - y_min + 1;
    eprintln!(
        "Fetching {tiles_x}x{tiles_y} = {} tiles at zoom {zoom} from {}",
        tiles_x * tiles_y,
        provider.name(),
    );

    let mut stitched = RgbaImage::new(tiles_x * TILE_SIZE, tiles_y * TILE_SIZE);

    for ty in y_min..=y_max {
        for tx in x_min..=x_max {
            let tile_img = fetch_tile(client, zoom, tx, ty, provider).await?;
            let px = (tx - x_min) * TILE_SIZE;
            let py = (ty - y_min) * TILE_SIZE;
            stitched
                .copy_from(&tile_img, px, py)
                .context("Failed to stitch tile")?;
        }
    }

    let n = 2_f64.powi(zoom as i32);
    let px_left = ((BBOX_WEST / 360.0 + 0.5) * n - x_min as f64) * TILE_SIZE as f64;
    let px_right = ((BBOX_EAST / 360.0 + 0.5) * n - x_min as f64) * TILE_SIZE as f64;
    let px_top = (mercator_y(BBOX_NORTH) * n - y_min as f64) * TILE_SIZE as f64;
    let px_bottom = (mercator_y(BBOX_SOUTH) * n - y_min as f64) * TILE_SIZE as f64;

    let crop_x = px_left.floor() as u32;
    let crop_y = px_top.floor() as u32;
    let crop_w = (px_right - px_left).ceil() as u32;
    let crop_h = (px_bottom - px_top).ceil() as u32;

    let cropped = DynamicImage::ImageRgba8(stitched)
        .crop_imm(crop_x, crop_y, crop_w, crop_h)
        .to_rgba8();

    eprintln!("Stitched and cropped to {crop_w}x{crop_h} pixels");

    Ok(TileMap {
        width: cropped.width(),
        height: cropped.height(),
        image: cropped,
    })
}

async fn fetch_tile(
    client: &reqwest::Client,
    zoom: u32,
    x: u32,
    y: u32,
    provider: Provider,
) -> Result<RgbaImage> {
    let cache_path = format!(
        "{}/{}/{zoom}/{x}/{y}.png",
        TILE_CACHE_DIR,
        provider.cache_subdir(),
    );
    let cache = Path::new(&cache_path);

    if cache.exists() {
        let img = image::open(cache)
            .with_context(|| format!("Failed to load cached tile {cache_path}"))?;
        return Ok(img.to_rgba8());
    }

    let url = provider.tile_url(zoom, x, y);
    let bytes = client
        .get(&url)
        .send()
        .await
        .with_context(|| format!("Failed to fetch tile {url}"))?
        .bytes()
        .await?;

    if let Some(parent) = cache.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(cache, &bytes)?;

    let img =
        image::load_from_memory(&bytes).with_context(|| format!("Failed to decode tile {url}"))?;
    Ok(img.to_rgba8())
}

fn lon_to_tile(lon: f64, zoom: u32) -> u32 {
    let n = 2_f64.powi(zoom as i32);
    ((lon / 360.0 + 0.5) * n).floor() as u32
}

fn lat_to_tile(lat: f64, zoom: u32) -> u32 {
    let n = 2_f64.powi(zoom as i32);
    (mercator_y(lat) * n).floor() as u32
}

/// Convert latitude to Web Mercator Y fraction (0.0 = top, 1.0 = bottom).
fn mercator_y(lat: f64) -> f64 {
    let lat_rad = lat.to_radians();
    (1.0 - lat_rad.tan().asinh() / std::f64::consts::PI) / 2.0
}
