mod auction_utils;
mod balances;
mod utils;
mod validators;

use clap::{crate_version, App, Arg, SubCommand};

use crate::{balances::generate_balances_update, validators::generate_validators_update};

fn main() {
    let matches = App::new("Global State Update Generator")
        .version(crate_version!())
        .about("Generates a global state update file based on the supplied parameters")
        .subcommand(
            SubCommand::with_name("validators")
                .about("Generates an update changing the validators set")
                .arg(
                    Arg::with_name("data_dir")
                        .short("d")
                        .long("data-dir")
                        .value_name("PATH")
                        .help("Data storage directory containing the global state database file")
                        .takes_value(true)
                        .required(true),
                )
                .arg(
                    Arg::with_name("hash")
                        .short("s")
                        .long("state-hash")
                        .value_name("HEX_STRING")
                        .help("The global state hash to be used as the base")
                        .takes_value(true)
                        .required(true),
                )
                .arg(
                    Arg::with_name("validator")
                        .short("v")
                        .long("validator")
                        .value_name("KEY,STAKE")
                        .help("A new validator with their stake")
                        .takes_value(true)
                        .required(true)
                        .multiple(true)
                        .number_of_values(1),
                ),
        )
        .subcommand(
            SubCommand::with_name("balances")
                .about("Generates an update changing account balances")
                .arg(
                    Arg::with_name("data_dir")
                        .short("d")
                        .long("data-dir")
                        .value_name("PATH")
                        .help("Data storage directory containing the global state database file")
                        .takes_value(true)
                        .required(true),
                )
                .arg(
                    Arg::with_name("hash")
                        .short("s")
                        .long("state-hash")
                        .value_name("HEX_STRING")
                        .help("The global state hash to be used as the base")
                        .takes_value(true)
                        .required(true),
                )
                .arg(
                    Arg::with_name("from")
                        .short("f")
                        .long("from")
                        .value_name("ACCOUNT_HASH")
                        .help("Source account hash")
                        .takes_value(true)
                        .required(true),
                )
                .arg(
                    Arg::with_name("to")
                        .short("t")
                        .long("to")
                        .value_name("ACCOUNT_HASH")
                        .help("Target account hash")
                        .takes_value(true)
                        .required(true),
                )
                .arg(
                    Arg::with_name("amount")
                        .short("a")
                        .long("amount")
                        .value_name("MOTES")
                        .help("Amount to be transferred")
                        .takes_value(true)
                        .required(true),
                )
                .arg(
                    Arg::with_name("proposer")
                        .short("p")
                        .long("proposer")
                        .value_name("PUBLIC_KEY")
                        .help("Public key of the proposer")
                        .takes_value(true)
                        .required(true),
                ),
        )
        .get_matches();

    match matches.subcommand() {
        ("validators", Some(sub_matches)) => generate_validators_update(sub_matches),
        ("balances", Some(sub_matches)) => generate_balances_update(sub_matches),
        _ => {
            println!("Unknown subcommand.");
        }
    }
}
