# Synclinal de Saou — Trail Coverage Visualizer

Generates a poster-quality PNG showing all OSM trails on the Synclinal de la Foret de Saou (Drome, France), with your GPS traces highlighted over OpenTopoMap tiles.

## Quick start

```bash
# First time: authenticate with Garmin Connect
garmin auth login

# Sync activities + render the map in one command
cargo run -- update
```

This will:
1. Fetch your recent Garmin activities
2. Download GPX files for runs near Saou (skips others automatically)
3. Match GPS traces against OSM trail segments to compute coverage
4. Render covered segments in orange, uncovered in white, over OpenTopoMap tiles
5. Output to `output/synclinal.png`

Re-run `update` after each new activity — it only downloads new GPX files.

## Commands

### `update` — Sync + render (recommended)

```bash
cargo run -- update
cargo run -- update --since 2025-06-01   # sync further back in time
cargo run -- update --zoom 16            # higher detail
```

### `sync` — Download activities from Garmin

```bash
cargo run -- sync
cargo run -- sync --since 2025-01-01
```

### `render` — Render from existing GPX files

```bash
cargo run -- render
cargo run -- render --no-cache           # force re-download of tiles and OSM data
cargo run -- render --tile-provider openstreetmap
```

### `debug` — Visual debug of trail segments

```bash
cargo run -- debug                       # each OSM segment in a different color
```

### Options

| Flag | Default | Description |
|------|---------|-------------|
| `-a, --activities-dir` | `activities` | Directory for GPX files |
| `-s, --since` | `2026-01-01` | Sync activities since date (YYYY-MM-DD) |
| `-o, --output` | `output/synclinal.png` | Output file path |
| `-z, --zoom` | `15` | Tile zoom level |
| `-p, --tile-provider` | `opentopomap` | `opentopomap` or `openstreetmap` |
| `--no-cache` | | Clear cached data before rendering |

## Prerequisites

- Rust toolchain
- [garmin-cli](https://lib.rs/crates/garmin-cli): `cargo install garmin-cli`

## How it works

1. Syncs activities from Garmin Connect, filtering by start coordinates to only download runs near the Synclinal de Saou
2. Parses GPX files and filters track segments by bounding box
3. Fetches OSM trail geometries (paths, tracks, footways) from the Overpass API
4. Splits OSM ways into segments at shared nodes (intersections) for precise per-segment coverage
5. Interpolates GPS tracks (every 2m) and trail segments (every 5m) into point clouds
6. Matches each segment sample point against GPS points within 10m using a spatial grid index
7. Marks a segment as covered if ≥50% of its points match
8. Downloads and stitches OpenTopoMap tiles (contours + hillshading)
9. Renders covered segments in orange with glow, uncovered in white, with stats overlay
10. Composites everything onto the tile background and outputs a print-ready PNG

## License

MIT
