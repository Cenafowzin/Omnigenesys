//! `mapgen`: generate maps from pipeline files, check them, and print the
//! schema and the operators.
//!
//! Data goes to stdout (or `-o`), everything meant for people to stderr,
//! so piping the map into another program never mixes in a message. Exit
//! code 0 on success, 1 on a pipeline or run error, 2 on bad arguments.

mod preview;
mod report;

use std::fmt::Display;
use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, Subcommand, ValueEnum};
use omnigenesys_core::export::{self, Encoding, Provenance};
use omnigenesys_core::operator::{Dimensions, Registry, Scope};
use omnigenesys_core::spec::{self, Built, MAX_NUMBER_SEED, PipelineSpec};

#[derive(Parser)]
#[command(
    name = "mapgen",
    version,
    about = "Generate maps from Omnigenesys pipeline files"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Run a pipeline and write the map (output v2 JSON).
    Generate(Generate),
    /// Check a pipeline file without running it.
    Validate {
        /// The pipeline file.
        pipeline: PathBuf,
        #[command(flatten)]
        params: Params,
    },
    /// Print the JSON Schema of pipeline files, for editors.
    Schema {
        /// Where to write it; stdout when absent.
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// List the operators a pipeline can use.
    Operators,
}

#[derive(clap::Args)]
struct Generate {
    /// The pipeline file.
    pipeline: PathBuf,
    /// Where to write the map; stdout when absent.
    #[arg(short, long)]
    output: Option<PathBuf>,
    #[command(flatten)]
    params: Params,
    /// Overrides the file's seed. With neither, one is drawn and printed.
    #[arg(long)]
    seed: Option<u64>,
    /// Overrides the file's salt. Prefer the environment variable: a salt
    /// on the command line ends up in the shell history.
    #[arg(long, env = "OMNIGENESYS_SALT", hide_env_values = true)]
    salt: Option<String>,
    /// How layers are written.
    #[arg(long, value_enum, default_value_t = EncodingArg::Auto)]
    encoding: EncodingArg,
    /// Also draw the map as a PNG.
    #[arg(long)]
    png: Option<PathBuf>,
    /// Pixels per cell in the PNG; by default the largest that keeps the
    /// image within 1024 px.
    #[arg(long, requires = "png", value_parser = clap::value_parser!(u32).range(1..=64))]
    scale: Option<u32>,
    /// Colours for the PNG: a JSON object of tile name to "#rrggbb".
    #[arg(long, requires = "png")]
    palette: Option<PathBuf>,
}

#[derive(clap::Args)]
struct Params {
    /// Sets a param, read as its declared type; repeat for more.
    #[arg(long = "param", value_name = "NAME=VALUE", value_parser = name_value)]
    params: Vec<(String, String)>,
}

/// `density=0.3` to `("density", "0.3")`. Split at the first `=`: a string
/// value may contain more.
fn name_value(arg: &str) -> Result<(String, String), String> {
    match arg.split_once('=') {
        Some((name, value)) if !name.is_empty() => Ok((name.to_string(), value.to_string())),
        _ => Err(format!("expected NAME=VALUE, got '{arg}'")),
    }
}

#[derive(Clone, Copy, ValueEnum)]
enum EncodingArg {
    Auto,
    Rle,
    Dense,
}

impl From<EncodingArg> for Encoding {
    fn from(e: EncodingArg) -> Encoding {
        match e {
            EncodingArg::Auto => Encoding::Auto,
            EncodingArg::Rle => Encoding::Rle,
            EncodingArg::Dense => Encoding::Dense,
        }
    }
}

/// A command failed and already said why on stderr.
struct Failed;

fn fail(message: impl Display) -> Failed {
    eprintln!("error: {message}");
    Failed
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let registry = Registry::with_builtins();
    let result = match cli.command {
        Command::Generate(g) => generate(&registry, g),
        Command::Validate { pipeline, params } => validate(&registry, &pipeline, &params),
        Command::Schema { output } => print_schema(&registry, output.as_deref()),
        Command::Operators => operators(&registry),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(Failed) => ExitCode::FAILURE,
    }
}

/// Reads and parses a pipeline file, printing warnings and errors.
fn load(path: &Path, params: &Params) -> Result<(String, PipelineSpec), Failed> {
    let text = fs::read_to_string(path)
        .map_err(|e| fail(format_args!("cannot read {}: {e}", path.display())))?;
    match PipelineSpec::parse_with(&text, &params.params) {
        Ok(parsed) => {
            for w in &parsed.warnings {
                report::print("warning", path, &text, w);
            }
            Ok((text, parsed.spec))
        }
        Err(errors) => Err(report::errors(path, &text, &errors)),
    }
}

fn build(
    registry: &Registry,
    path: &Path,
    text: &str,
    spec: &PipelineSpec,
) -> Result<Built, Failed> {
    spec.build(registry)
        .map_err(|errors| report::errors(path, text, &errors))
}

fn generate(registry: &Registry, g: Generate) -> Result<(), Failed> {
    // Read before the map is made: a bad palette fails in a moment.
    let palette = match &g.palette {
        Some(path) => preview::Palette::load(path).map_err(fail)?,
        None => preview::Palette::default(),
    };
    let (text, spec) = load(&g.pipeline, &g.params)?;
    let Built {
        grid,
        tiles,
        pipeline,
    } = build(registry, &g.pipeline, &text, &spec)?;

    // The command line wins over the file; with neither, draw one and say
    // which, so the map can be made again.
    let seed = match g.seed.or(spec.seed) {
        Some(seed) => seed,
        None => {
            let seed = random_seed();
            eprintln!("seed: {seed}");
            seed
        }
    };
    let salt = g.salt.unwrap_or_else(|| spec.salt.clone());
    let made = pipeline
        .run_attempts(&grid, &salt, seed, spec.attempts)
        .map_err(fail)?;
    if made.attempt > 0 {
        eprintln!(
            "attempt {} of {} made the map; the ones before failed at: {}",
            made.attempt + 1,
            spec.attempts,
            made.failures
                .iter()
                .map(|f| f.step_id.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        );
    }
    let grid = made.grid;
    let provenance = Provenance {
        attempt: made.attempt,
        ..spec.provenance(seed, &salt)
    };
    let encoding = g.encoding.into();
    match &g.output {
        Some(path) => {
            let file = File::create(path)
                .map_err(|e| fail(format_args!("cannot create {}: {e}", path.display())))?;
            export::write_json(file, &grid, &tiles, &provenance, encoding).map_err(fail)?;
        }
        None => {
            export::write_json(io::stdout().lock(), &grid, &tiles, &provenance, encoding)
                .map_err(fail)?;
        }
    }
    if let Some(path) = &g.png {
        for name in palette.unknown(&tiles) {
            eprintln!("warning: the palette colours '{name}', which is not a tile of this map");
        }
        preview::write(path, &grid, &tiles, &palette, g.scale).map_err(fail)?;
    }
    Ok(())
}

fn validate(registry: &Registry, path: &Path, params: &Params) -> Result<(), Failed> {
    let (text, spec) = load(path, params)?;
    build(registry, path, &text, &spec)?;
    let enabled = spec.steps.iter().filter(|s| s.enabled).count();
    eprintln!(
        "ok: {}: {} steps ({enabled} enabled)",
        path.display(),
        spec.steps.len()
    );
    Ok(())
}

fn print_schema(registry: &Registry, output: Option<&Path>) -> Result<(), Failed> {
    let mut text = serde_json::to_string_pretty(&spec::schema(registry)).map_err(fail)?;
    text.push('\n');
    match output {
        Some(path) => fs::write(path, text)
            .map_err(|e| fail(format_args!("cannot write {}: {e}", path.display()))),
        None => io::stdout().lock().write_all(text.as_bytes()).map_err(fail),
    }
}

fn operators(registry: &Registry) -> Result<(), Failed> {
    let mut out = io::stdout().lock();
    let width = registry.names().map(str::len).max().unwrap_or(0);
    for name in registry.names() {
        let Some(entry) = registry.get(name) else {
            continue;
        };
        let dimensions = match entry.dimensions {
            Dimensions::TwoD => "2d",
            Dimensions::ThreeD => "3d",
            Dimensions::Any => "2d/3d",
        };
        let scope = match entry.scope {
            Scope::Local => "local",
            Scope::Global => "global",
        };
        writeln!(out, "{name:<width$}  {dimensions:<6} {scope}").map_err(fail)?;
    }
    Ok(())
}

/// A seed nobody chose. Not cryptographic, only different on every run:
/// std seeds `RandomState` from the operating system. Kept within
/// [`MAX_NUMBER_SEED`] so it can be pasted into a file as a plain number.
fn random_seed() -> u64 {
    use std::hash::{BuildHasher, Hasher};
    use std::time::{SystemTime, UNIX_EPOCH};
    let mut h = std::collections::hash_map::RandomState::new().build_hasher();
    let now = SystemTime::now().duration_since(UNIX_EPOCH);
    h.write_u128(now.map_or(0, |d| d.as_nanos()));
    h.finish() & MAX_NUMBER_SEED
}
