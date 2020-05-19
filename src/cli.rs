//! Command-line option parsing.
//!
//! Most configuration is done through the configuration, which is the only required command-line
//! argument. However some configuration values can be overwritten for convenience's sake.
use std::{io, io::Write, path};
use structopt::StructOpt;

use crate::{config, reactor};

// Note: The docstring on `Cli` is the help shown when calling the binary with `--help`.
#[derive(Debug, StructOpt)]
/// CasperLabs blockchain node.
pub enum Cli {
    /// Generate a configuration file from defaults and dump it to stdout.
    GenerateConfig {},
    /// Run the validator node.
    ///
    /// Loads the configuration values from the given configuration file or uses defaults if not
    /// given, then launches the reactor.
    Validator {
        #[structopt(short, long, env)]
        /// Path to configuration file.
        config: Option<path::PathBuf>,
    },
}

impl Cli {
    /// Execute selected CLI command.
    pub async fn run(self) -> anyhow::Result<()> {
        match self {
            Cli::GenerateConfig {} => {
                let cfg_str = config::to_string(&Default::default())?;
                io::stdout().write_all(cfg_str.as_bytes())?;

                Ok(())
            }
            Cli::Validator { config } => {
                // We load the specified config, if any, otherwise use defaults.
                let cfg = config
                    .map(config::load_from_file)
                    .transpose()?
                    .unwrap_or_default();

                cfg.log.setup_logging()?;

                reactor::launch::<reactor::validator::Reactor>(cfg).await
            }
        }
    }
}
