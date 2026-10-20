//! rustfolio CLI — thin wrapper around the library.
//!
//! Commands: build, check, scss, verify, serve.

use std::path::{Path, PathBuf};

use anyhow::Result;
use clap::{Parser, Subcommand};
use rustfolio::{build, config, content, highlight, scss, serve, templates, validate};
use tracing_subscriber::EnvFilter;

#[derive(Debug, Parser)]
#[command(
    name = "rustfolio",
    version,
    about = "Build the rustfolio engineering blog: a static site generated in Rust."
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Build the complete static site.
    Build {
        /// Output directory (replaced atomically on success).
        #[arg(long, default_value = "dist")]
        output: PathBuf,
        /// Include posts marked as drafts (for local previews).
        #[arg(long)]
        include_drafts: bool,
    },
    /// Validate configuration, frontmatter, content and SCSS without building.
    Check,
    /// Compile every SCSS entrypoint in strict mode (fails on any error).
    Scss,
    /// Verify a built site: artifacts, internal links, RSS and sitemap.
    Verify {
        /// The directory a previous build produced.
        #[arg(long, default_value = "dist")]
        output: PathBuf,
    },
    /// Serve a built site locally for development.
    Serve {
        /// Directory to serve.
        #[arg(long, default_value = "dist")]
        dir: PathBuf,
        /// Port to listen on.
        #[arg(long, default_value = "8080")]
        port: u16,
    },
}

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("rustfolio=info")),
        )
        .with_target(false)
        .compact()
        .init();

    let cli = Cli::parse();
    match cli.command {
        Command::Build {
            output,
            include_drafts,
        } => build::run(&build::BuildOptions {
            root: PathBuf::from("."),
            output,
            include_drafts,
        }),

        Command::Check => {
            let root = Path::new(".");
            let config = config::load(&root.join("rustfolio.toml"))?;
            let highlighter = highlight::Highlighter::new();
            let static_root = root.join("static");
            let posts = content::load_posts(
                &root.join("content/posts"),
                true, // drafts are validated too
                &highlighter,
                &static_root,
            )?;
            validate::validate_posts(&posts)?;
            let pages =
                content::load_pages(&root.join("content/pages"), &highlighter, &static_root)?;
            let mut styles = scss::compile_all(&root.join("scss"))?;
            styles.push(scss::compile_file(&root.join("scss/main.scss"))?);
            let engine = templates::Engine::load(&root.join("templates"))?;
            engine.render("base.html", serde_json::Map::new())?;
            println!(
                "OK: {} posts ({} drafts), {} pages, {} SCSS entries, templates valid, config `{}` loaded",
                posts.len(),
                posts.iter().filter(|p| p.draft).count(),
                pages.len(),
                styles.len(),
                config.site.title
            );
            Ok(())
        }

        Command::Scss => {
            let compiled = scss::compile_all(Path::new("scss"))?;
            println!(
                "OK: compiled {} SCSS entrypoint(s) in strict mode",
                compiled.len()
            );
            Ok(())
        }

        Command::Verify { output } => {
            let root = Path::new(".");
            let config = config::load(&root.join("rustfolio.toml"))?;
            let highlighter = highlight::Highlighter::new();
            let posts = content::load_posts(
                &root.join("content/posts"),
                false,
                &highlighter,
                &root.join("static"),
            )?;
            validate::verify_site(&output, &config, &posts)?;
            println!(
                "OK: built site verified — {} posts, all internal links and feeds valid",
                posts.len()
            );
            Ok(())
        }

        Command::Serve { dir, port } => serve::serve(&dir, port),
    }
}
