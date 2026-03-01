use std::process;

use clap::Parser;

use crate::job::start_server_at;

mod job;
mod socket;
mod state;

#[derive(clap::Parser)]
#[command(
    bin_name = "vacht",
    version = env!("CARGO_PKG_VERSION"),
    long_about = "a dead simple python-v8 bridge",
)]
struct Cli {
    #[command(subcommand)]
    command: CliCommand,
}

#[derive(clap::Subcommand)]
enum CliCommand {
    /// Gets the default name of the local socket depending on the platform.
    Name,

    /// Runs the instance.
    ///
    /// Using `vacht --debug run`, it shows debug information.
    Run(RunOptions),
}

#[derive(clap::Args)]
struct RunOptions {
    /// Enable debug information on all commands.
    #[arg(long)]
    debug: bool,

    /// Specify a custom socket path name.
    #[arg(long, required = false)]
    name: Option<String>,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        CliCommand::Name => {
            let (name, _) = crate::socket::get_name(None)?;
            println!("{}", name);
        }

        CliCommand::Run(run_options) => {
            if run_options.debug {
                tracing_subscriber::fmt::init();
            }

            let rt = tokio::runtime::Runtime::new()?;

            let (printname, name) =
                crate::socket::get_name(run_options.name.as_ref().map(|k| &**k))?;
            if let Err(why) = rt.block_on(start_server_at(printname, name)) {
                tracing::error!("an error occurred while running job:\n{why:#?}");
                eprintln!("error while running job");
                process::exit(1);
                // UNREACHABLE
            }

            process::exit(0); // not mandatory
        }
    }

    Ok(())
}
