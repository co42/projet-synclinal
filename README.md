# Synclinal de Saou — Trail Coverage Visualizer

Generates a poster-quality PNG showing all OSM trails on the Synclinal de la Foret de Saou (Drome, France), with covered trails highlighted over OpenTopoMap tiles.

## Usage

```bash
cargo run -- render
```

### Options

| Flag | Default | Description |
|------|---------|-------------|
| `-o, --output` | `output/synclinal.png` | Output file path |
| `-z, --zoom` | `15` | Tile zoom level |
| `-p, --tile-provider` | `opentopomap` | `opentopomap` or `openstreetmap` |
| `--no-cache` | | Clear cached data before rendering |

### Examples

```bash
# Render with OpenStreetMap tiles
cargo run -- render --tile-provider openstreetmap

# Force re-download of all cached data
cargo run -- render --no-cache

# Higher zoom for more detail
cargo run -- render --zoom 16
```

## How it works

1. Fetches trail geometries (paths, tracks, footways) from the Overpass API
2. Downloads and stitches map tiles from OpenTopoMap (contours + hillshading)
3. Renders trail overlay as SVG, rasterizes it, and composites onto the tile background
4. Outputs a print-ready PNG

## License

MIT
