#![forbid(unsafe_code)]

use std::env;
use std::error::Error;
use std::io::Write;
use std::process::ExitCode;
use std::sync::Arc;

use clap::{Parser, Subcommand, ValueEnum};
use postmark_mcp::{Config, Environment, PostmarkMcpServer, SendPolicy, ServerDefaults, ToolLog};
use postmark_mcp_client::{HardenedTransport, PostmarkApi, TransportConfig};
use postmark_mcp_core::{GatewayError, ReadsServer, ServerSummary};
use rmcp::ServiceExt;
use rmcp::transport::stdio;
use tracing::level_filters::LevelFilter;

#[derive(Debug, Parser)]
#[command(
    name = "postmark-mcp",
    version = concat!("v", env!("CARGO_PKG_VERSION")),
    about = "MCP server for the Postmark email API"
)]
struct Cli {
    /// Diagnostic log level. Logs go to stderr; stdout carries only MCP traffic.
    #[arg(long, value_enum, default_value_t = LogLevel::Info, global = true)]
    log_level: LogLevel,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Clone, Copy, Debug, Subcommand)]
enum Command {
    /// Validate configuration and check the token against Postmark, then exit.
    Doctor,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum LogLevel {
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}

impl From<LogLevel> for LevelFilter {
    fn from(level: LogLevel) -> Self {
        match level {
            LogLevel::Error => Self::ERROR,
            LogLevel::Warn => Self::WARN,
            LogLevel::Info => Self::INFO,
            LogLevel::Debug => Self::DEBUG,
            LogLevel::Trace => Self::TRACE,
        }
    }
}

#[tokio::main]
async fn main() -> ExitCode {
    let cli = Cli::parse();
    tracing_subscriber::fmt()
        .with_max_level(LevelFilter::from(cli.log_level))
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .init();

    match run(cli.command).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            tracing::error!("{error}");
            ExitCode::FAILURE
        }
    }
}

async fn run(command: Option<Command>) -> Result<(), Box<dyn Error>> {
    // `vars_os`, because `vars` panics on a non-Unicode value in any variable,
    // including ones this server does not read. Such a value is left out here,
    // so a variable this server needs is reported as unset.
    let environment: Environment = env::vars_os()
        .filter_map(|(name, value)| Some((name.into_string().ok()?, value.into_string().ok()?)))
        .collect();
    let config = Config::try_from(&environment)?;

    let mut transport = TransportConfig::new(config.api_base_url.clone(), config.server_token.clone());
    transport.agent_label.clone_from(&config.agent_label);
    let api = Arc::new(PostmarkApi::from(HardenedTransport::try_from(transport)?));

    match command {
        Some(Command::Doctor) => doctor(api.as_ref()).await,
        None => serve(api, &config).await,
    }
}

async fn doctor(api: &PostmarkApi) -> Result<(), Box<dyn Error>> {
    let server = verify(api).await?;
    writeln!(
        std::io::stdout(),
        "Configuration is valid. Token accepted for Postmark server {} ({}).",
        server.name,
        server.id
    )?;
    Ok(())
}

async fn serve(api: Arc<PostmarkApi>, config: &Config) -> Result<(), Box<dyn Error>> {
    if config.skip_verify {
        tracing::warn!("POSTMARK_SKIP_VERIFY is set: the token was not checked against Postmark");
    } else {
        let server = verify(api.as_ref()).await?;
        tracing::info!(server_id = server.id, "token accepted by Postmark");
    }

    let log = ToolLog::open(config.log_file.as_deref(), config.log_email_full)
        .map_err(|error| format!("LOG_FILE could not be opened: {error}"))?;
    if config.read_only {
        tracing::info!("POSTMARK_READ_ONLY is set: the send tools are not available");
    }
    let server = PostmarkMcpServer::new(api, ServerDefaults::from(config))
        .with_send_policy(SendPolicy::from(config))
        .with_tool_log(log);
    server.serve(stdio()).await?.waiting().await?;
    Ok(())
}

async fn verify(api: &PostmarkApi) -> Result<ServerSummary, Box<dyn Error>> {
    api.current_server()
        .await
        .map_err(|error: GatewayError| format!("{error}. {}", error.remediation()).into())
}
