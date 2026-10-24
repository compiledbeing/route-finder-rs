use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{Context, Result};
use clap::{Args, Parser, Subcommand, ValueEnum};

use route_finder::export::RouteJson;
use route_finder::report::{format_comparison, format_locations, format_route};
use route_finder::{
    find_route, load_graph, parse_graph, Algorithm, Graph, Metric, DEFAULT_DATASET,
};

/// Shortest paths on a road network, using BFS, Dijkstra or A*.
#[derive(Parser)]
#[command(name = "route-finder", version)]
struct Cli {
    /// Road network JSON file (default: the bundled Java island dataset)
    #[arg(long, global = true, value_name = "FILE")]
    data: Option<PathBuf>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Find a route between two places
    Route(RouteArgs),
    /// List every location in the dataset
    List,
    /// Start the interactive web map and JSON API
    #[cfg(feature = "server")]
    Serve {
        /// Port to listen on (127.0.0.1 only)
        #[arg(long, default_value_t = 8080)]
        port: u16,
    },
}

#[derive(Args)]
struct RouteArgs {
    /// Start location (id or name, case-insensitive)
    #[arg(long)]
    from: String,

    /// Destination
    #[arg(long)]
    to: String,

    /// Search algorithm; `all` runs the three and prints a comparison
    #[arg(long, value_enum, default_value_t = AlgoArg::Astar)]
    algo: AlgoArg,

    /// What the weighted algorithms minimise
    #[arg(long, value_enum, default_value_t = MetricArg::Distance)]
    metric: MetricArg,

    /// Also write the result(s) to a JSON file
    #[arg(long, value_name = "FILE")]
    json: Option<PathBuf>,
}

#[derive(Clone, Copy, ValueEnum)]
enum AlgoArg {
    Bfs,
    Dijkstra,
    Astar,
    All,
}

impl AlgoArg {
    fn algorithms(self) -> Vec<Algorithm> {
        match self {
            AlgoArg::Bfs => vec![Algorithm::Bfs],
            AlgoArg::Dijkstra => vec![Algorithm::Dijkstra],
            AlgoArg::Astar => vec![Algorithm::AStar],
            AlgoArg::All => Algorithm::ALL.to_vec(),
        }
    }
}

#[derive(Clone, Copy, ValueEnum)]
enum MetricArg {
    Distance,
    Time,
}

impl From<MetricArg> for Metric {
    fn from(arg: MetricArg) -> Self {
        match arg {
            MetricArg::Distance => Metric::Distance,
            MetricArg::Time => Metric::Time,
        }
    }
}

fn main() -> ExitCode {
    match run() {
        Ok(code) => code,
        Err(err) => {
            eprintln!("error: {err:#}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<ExitCode> {
    let cli = Cli::parse();
    let graph = load(cli.data.as_deref())?;

    match cli.command {
        Command::List => {
            print!("{}", format_locations(&graph));
            Ok(ExitCode::SUCCESS)
        }
        Command::Route(args) => run_route(&graph, &args),
        #[cfg(feature = "server")]
        Command::Serve { port } => {
            tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .context("starting the async runtime")?
                .block_on(route_finder::server::serve(graph, port))
                .with_context(|| format!("serving on port {port}"))?;
            Ok(ExitCode::SUCCESS)
        }
    }
}

fn load(data: Option<&Path>) -> Result<Graph> {
    match data {
        Some(path) => load_graph(path).with_context(|| format!("loading {}", path.display())),
        None => parse_graph(DEFAULT_DATASET).context("loading the bundled dataset"),
    }
}

fn run_route(graph: &Graph, args: &RouteArgs) -> Result<ExitCode> {
    let locate = |query: &str| {
        graph
            .find(query)
            .with_context(|| format!("unknown location '{query}' (try `route-finder list`)"))
    };
    let from = locate(&args.from)?;
    let to = locate(&args.to)?;
    let metric = Metric::from(args.metric);

    let results = args
        .algo
        .algorithms()
        .into_iter()
        .map(|algorithm| find_route(graph, from, to, algorithm, metric))
        .collect::<Result<Vec<_>, _>>()?;
    // Never empty: `algorithms()` always returns at least one algorithm.
    let last = results.last().expect("at least one algorithm ran");

    if results.len() > 1 {
        println!("{}", format_comparison(&results));
    }
    print!("{}", format_route(graph, last));

    if let Some(path) = &args.json {
        let views: Vec<RouteJson> = results.iter().map(|r| RouteJson::new(graph, r)).collect();
        let text = match views.as_slice() {
            [single] => serde_json::to_string_pretty(single)?,
            many => serde_json::to_string_pretty(many)?,
        };
        fs::write(path, text + "\n").with_context(|| format!("writing {}", path.display()))?;
        println!("\nSaved JSON to {}", path.display());
    }

    Ok(if last.found() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    })
}
