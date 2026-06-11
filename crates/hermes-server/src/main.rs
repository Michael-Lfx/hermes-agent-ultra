use std::net::SocketAddr;
use tracing::info;

/// Parse command line arguments for hermes-ultra.
struct Args {
    /// Profile name to use
    profile: String,
}

impl Args {
    fn parse() -> Self {
        let mut args = std::env::args().skip(1);
        let mut profile = "default".to_string();
        
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--profile" | "-p" => {
                    if let Some(value) = args.next() {
                        profile = value;
                    }
                }
                _ => {}
            }
        }
        
        Self { profile }
    }
}

#[tokio::main]
async fn main() {
    // Initialize tracing
    tracing_subscriber::fmt::init();
    
    // Parse command line arguments
    let args = Args::parse();
    
    // Default address: 127.0.0.1:9119
    let addr: SocketAddr = std::env::var("HERMES_SERVER_ADDR")
        .unwrap_or_else(|_| "127.0.0.1:9119".to_string())
        .parse()
        .expect("Invalid HERMES_SERVER_ADDR");
    
    info!("Starting hermes-server (hermes-ultra) on {}", addr);
    info!("Using profile: {}", args.profile);
    
    if let Err(e) = hermes_server::run_with_profile(addr, &args.profile).await {
        eprintln!("Server error: {}", e);
        std::process::exit(1);
    }
}
