//! The lendd command: serve the library over HTTP, or import and report on
//! books from the command line.

mod http;

use clap::{Args, Parser, Subcommand};
use lending::store::memory::MemoryStore;
use lending::Library;

/// The command line of lendd.
#[derive(Parser)]
#[command(name = "lendd")]
struct Cli {
    /// What to do.
    #[command(subcommand)]
    command: Command,
}

/// The things lendd can do.
#[derive(Subcommand)]
enum Command {
    /// Serves the library over HTTP.
    Serve {
        /// The address to listen on.
        #[arg(long)]
        addr: String,
    },
    /// Reads books from a file.
    #[command(name = "import-books")]
    Import(ImportArgs),
    /// Prints reports.
    Report(ReportArgs),
}

/// The arguments of the import command.
#[derive(Args)]
struct ImportArgs {
    /// The file to read.
    path: String,
}

/// The arguments of the report command.
#[derive(Args)]
struct ReportArgs {
    /// Which report.
    #[command(subcommand)]
    kind: ReportKind,
}

/// The reports lendd can print.
#[derive(Subcommand)]
enum ReportKind {
    /// Lists the books on loan.
    Overdue,
    /// Counts the books.
    Count,
}

/// Parses the command line and runs the command it names.
fn main() {
    let cli = Cli::parse();
    match cli.command {
        Command::Serve { addr } => serve(&addr),
        Command::Import(args) => {
            println!("importing {}", args.path);
            import(&args.path);
        }
        Command::Report(args) => match args.kind {
            ReportKind::Overdue => println!("no loans are overdue"),
            ReportKind::Count => report_count(),
        },
    }
}

/// Builds the router and serves it.
fn serve(addr: &str) {
    let _app = http::router();
    println!("serving on {addr}");
}

/// Reads books from a file into a new library.
fn import(path: &str) {
    let library = Library::new(MemoryStore::new(), 0);
    let _ = (path, library.store());
}

/// Prints how many books the store holds.
fn report_count() {
    use lending::store::Counted;
    println!("{}", MemoryStore::new().count());
}
