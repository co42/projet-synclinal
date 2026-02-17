mod config;
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
    /// Render trail coverage map
    Render {
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
}

#[derive(Clone, ValueEnum)]
enum TileProvider {
    Openstreetmap,
    Opentopomap,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Render {
            output,
            zoom,
            tile_provider,
            no_cache,
        } => {
            let provider = match tile_provider {
                TileProvider::Openstreetmap => tiles::Provider::OpenStreetMap,
                TileProvider::Opentopomap => tiles::Provider::OpenTopoMap,
            };

            if no_cache {
                osm::clear_cache();
                tiles::clear_cache();
            }

            let client = reqwest::Client::builder()
                .user_agent("synclinal-trail-visualizer/0.1")
                .build()?;

            let trails = osm::fetch_trails(&client).await?;
            let tile_map = tiles::fetch_and_stitch(&client, zoom, provider).await?;
            render::render_png(&tile_map, &trails, &output)?;
        }
    }

    Ok(())
}
