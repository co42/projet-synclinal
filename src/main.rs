mod config;
mod export;
mod garmin;
mod gpx;
mod grid;
mod matching;
mod osm;
mod render;
mod tiles;

use anyhow::Result;
use clap::{Parser, Subcommand, ValueEnum};

#[derive(Parser)]
#[command(
    name = "synclinal",
    about = "Trail coverage visualizer for the Synclinal de Saou"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Sync activities from Garmin Connect
    Sync {
        /// Directory to store GPX files
        #[arg(short, long, default_value = "activities")]
        activities_dir: String,

        /// Only sync activities since this date (YYYY-MM-DD)
        #[arg(short, long, default_value = "2026-01-01")]
        since: String,
    },

    /// Render trail coverage map
    Render {
        /// Directory containing GPX files
        #[arg(short, long, default_value = "activities")]
        activities_dir: String,

        /// Output file path
        #[arg(short, long, default_value = "output/synclinal.png")]
        output: String,

        /// Tile zoom level
        #[arg(short, long, default_value_t = config::DEFAULT_ZOOM)]
        zoom: u32,

        /// Tile provider
        #[arg(short = 'p', long, default_value = "opentopomap")]
        tile_provider: TileProvider,

        /// Clear cached data before rendering
        #[arg(long)]
        no_cache: bool,
    },

    /// Debug: render map with raw GPS dots overlay
    Debug {
        /// Directory containing GPX files
        #[arg(short, long, default_value = "activities")]
        activities_dir: String,

        /// Output file path
        #[arg(short, long, default_value = "output/debug.png")]
        output: String,

        /// Tile zoom level
        #[arg(short, long, default_value_t = config::DEFAULT_ZOOM)]
        zoom: u32,

        /// Tile provider
        #[arg(short = 'p', long, default_value = "opentopomap")]
        tile_provider: TileProvider,
    },

    /// Export segments and grid data as JSON for the web UI
    Export {
        /// Directory containing GPX files
        #[arg(short, long, default_value = "activities")]
        activities_dir: String,

        /// Output JSON file path
        #[arg(short, long, default_value = "web/data.json")]
        output: String,

        /// Grid cell size in meters
        #[arg(long, default_value_t = 200.0)]
        grid_size: f64,
    },

    /// Sync new activities from Garmin and re-render the map
    Update {
        /// Directory to store GPX files
        #[arg(short, long, default_value = "activities")]
        activities_dir: String,

        /// Only sync activities since this date (YYYY-MM-DD)
        #[arg(short, long, default_value = "2026-01-01")]
        since: String,

        /// Output file path
        #[arg(short, long, default_value = "output/synclinal.png")]
        output: String,

        /// Tile zoom level
        #[arg(short, long, default_value_t = config::DEFAULT_ZOOM)]
        zoom: u32,

        /// Tile provider
        #[arg(short = 'p', long, default_value = "opentopomap")]
        tile_provider: TileProvider,
    },
}

#[derive(Clone, ValueEnum)]
enum TileProvider {
    Openstreetmap,
    Opentopomap,
}

fn resolve_provider(tp: &TileProvider) -> tiles::Provider {
    match tp {
        TileProvider::Openstreetmap => tiles::Provider::OpenStreetMap,
        TileProvider::Opentopomap => tiles::Provider::OpenTopoMap,
    }
}

fn build_client() -> Result<reqwest::Client> {
    Ok(reqwest::Client::builder()
        .user_agent("synclinal-trail-visualizer/0.1")
        .build()?)
}

async fn do_render(
    activities_dir: &str,
    output: &str,
    zoom: u32,
    provider: tiles::Provider,
) -> Result<()> {
    let client = build_client()?;
    let (_trails, segments) = osm::fetch_trails(&client).await?;
    let activities = gpx::load_activities(activities_dir)?;
    let coverage = matching::compute_coverage(&segments, &activities);
    let tile_map = tiles::fetch_and_stitch(&client, zoom, provider).await?;
    render::render_png(&tile_map, &segments, &coverage, output)
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Sync {
            activities_dir,
            since,
        } => {
            garmin::sync(&activities_dir, &since)?;
        }

        Commands::Render {
            activities_dir,
            output,
            zoom,
            tile_provider,
            no_cache,
        } => {
            if no_cache {
                osm::clear_cache();
                tiles::clear_cache();
            }
            do_render(
                &activities_dir,
                &output,
                zoom,
                resolve_provider(&tile_provider),
            )
            .await?;
        }

        Commands::Debug {
            activities_dir: _,
            output,
            zoom,
            tile_provider,
        } => {
            let client = build_client()?;
            let (_trails, segments) = osm::fetch_trails(&client).await?;
            let tile_map =
                tiles::fetch_and_stitch(&client, zoom, resolve_provider(&tile_provider)).await?;
            render::render_debug_png(&tile_map, &segments, &output)?;
        }

        Commands::Export {
            activities_dir,
            output,
            grid_size,
        } => {
            let client = build_client()?;
            let (_trails, segments) = osm::fetch_trails(&client).await?;
            let activities = gpx::load_activities(&activities_dir)?;
            let coverage = matching::compute_coverage(&segments, &activities);
            let grid_result = grid::compute_grid(&segments, &coverage, grid_size);
            export::export_json(&segments, &coverage, &grid_result, &output)?;
        }

        Commands::Update {
            activities_dir,
            since,
            output,
            zoom,
            tile_provider,
        } => {
            garmin::sync(&activities_dir, &since)?;
            do_render(
                &activities_dir,
                &output,
                zoom,
                resolve_provider(&tile_provider),
            )
            .await?;
        }
    }

    Ok(())
}
