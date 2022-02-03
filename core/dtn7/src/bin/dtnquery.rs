use clap::{Parser, Subcommand};

/// A simple Bundle Protocol 7 Query Utility for Delay Tolerant Networking
#[derive(Parser, Debug)]
#[clap(version, author, long_about = None)]
struct Args {
    /// Local web port (default = 3000)
    #[clap(short, long, default_value_t = 3000)]
    port: u16,

    /// Use IPv6
    #[clap(short = '6', long)]
    ipv6: bool,

    #[clap(subcommand)]
    cmd: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// List registered endpoint IDs
    Eids,
    /// List known peers
    Peers,
    /// List bundles on node
    Bundles {
        /// Verbose output includes bundle destination
        #[clap(short, long)]
        verbose: bool,
    },
    /// List bundles status in store
    Store,
    /// General dtnd info
    Info,
    /// Local node id
    Nodeid,
}

fn main() {
    let args = Args::parse();
    let port = if let Ok(env_port) = std::env::var("DTN_WEB_PORT") {
        env_port // string is fine no need to parse number
    } else {
        args.port.to_string()
    };

    let localhost = if args.ipv6 { "[::1]" } else { "127.0.0.1" };

    let url = match &args.cmd {
        Commands::Eids => {
            println!("Listing registered endpoint IDs:");
            format!("http://{}:{}/status/eids", localhost, port)
        }
        Commands::Peers => {
            println!("Listing of known peers:");
            format!("http://{}:{}/status/peers", localhost, port)
        }
        Commands::Bundles { verbose } => {
            println!("Listing of bundles in store:");
            if *verbose {
                format!("http://{}:{}/status/bundles", localhost, port)
            } else {
                format!("http://{}:{}/status/bundles_dest", localhost, port)
            }
        }
        Commands::Store => {
            println!("Listing of bundles status in store:");
            format!("http://{}:{}/status/store", localhost, port)
        }
        Commands::Info => {
            println!("Daemon info:");
            format!("http://{}:{}/status/info", localhost, port)
        }
        Commands::Nodeid => {
            println!("Local node ID:");
            format!("http://{}:{}/status/nodeid", localhost, port)
        }
    };
    let res = attohttpc::get(url)
        .send()
        .expect("error connecting to local dtnd")
        .text()
        .unwrap();
    println!("{}", res);
}