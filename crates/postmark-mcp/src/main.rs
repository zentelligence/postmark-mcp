#![forbid(unsafe_code)]

use std::env;
use std::error::Error;
use std::io::Write;
use std::process::ExitCode;
use std::sync::Arc;

use clap::{CommandFactory, Parser, Subcommand, ValueEnum};
use postmark_mcp::{Config, Environment, PostmarkMcpServer, ServerDefaults, ServerPolicy, Settings, Sources, ToolLog};
use postmark_mcp_client::{HardenedTransport, PostmarkApi, TransportConfig};
use postmark_mcp_core::{GatewayError, ReadsServer, ServerSummary};
use rmcp::ServiceExt;
use rmcp::service::ServerInitializeError;
use rmcp::transport::stdio;
use tracing::level_filters::LevelFilter;

#[derive(Parser)]
#[command(name = "postmark-mcp", version, about = "MCP server for the Postmark email API")]
struct Cli {
    /// Diagnostic log level. Logs go to stderr; stdout carries only MCP traffic.
    #[arg(long, value_enum, default_value_t = LogLevel::Info, global = true)]
    log_level: LogLevel,

    #[command(flatten)]
    settings: Settings,

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
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(error) => return refuse_command_line(&error),
    };
    tracing_subscriber::fmt()
        .with_max_level(LevelFilter::from(cli.log_level))
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .init();

    match run(cli.command, cli.settings).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            tracing::error!("{error}");
            ExitCode::FAILURE
        }
    }
}

/// Reports a command line that could not be read. The parser quotes what it
/// could not read, which can be the token itself, as when a flag and its value
/// are given as one argument, so its message is withheld whenever the token flag
/// appears.
fn refuse_command_line(error: &clap::Error) -> ExitCode {
    use clap::error::{ContextKind, ErrorKind};

    let mentions_token = env::args_os().any(|argument| {
        let argument = argument.to_string_lossy();
        argument.starts_with("--server-token") && !argument.starts_with("--server-token-file")
    });
    if matches!(error.kind(), ErrorKind::DisplayHelp | ErrorKind::DisplayVersion) || !mentions_token {
        // Help and version print to stdout and succeed. Other errors print as
        // usual, since without the token flag they cannot hold the token.
        let _ = error.print();
        return ExitCode::from(u8::try_from(error.exit_code()).unwrap_or(2));
    }
    // The flag at fault is named separately from any value, so it is safe to
    // repeat, and tells the operator where to look.
    let culprit = error
        .get(ContextKind::InvalidArg)
        .and_then(|context| known_flag(&context.to_string()))
        .map(|flag| format!(" The problem is with {flag}."))
        .unwrap_or_default();
    let _ = writeln!(
        std::io::stderr(),
        "error: the command line could not be read.{culprit} Give --server-token and its value as two separate \
         arguments, or use --server-token-file. The details are withheld because they may include the token."
    );
    ExitCode::from(2)
}

/// The defined flag a parser message refers to, if any. For an unknown argument
/// the parser reports the raw text, which can hold the token, so only a name that
/// matches one of this server's own flags is ever repeated, and nothing after it.
fn known_flag(reported: &str) -> Option<String> {
    let first = reported.split([' ', '=']).next()?;
    Cli::command()
        .get_arguments()
        .filter_map(|argument| argument.get_long())
        .map(|long| format!("--{long}"))
        .find(|flag| flag == first)
}

/// Upper bound on a token file, well above any real token.
const MAX_TOKEN_FILE_BYTES: u64 = 4096;

/// Reads the token from a file. Errors name the flag and the path, never anything
/// read from the file.
async fn read_token_file(path: &std::path::Path) -> Result<String, Box<dyn Error>> {
    let unreadable = |error: std::io::Error| {
        format!(
            "--server-token-file {} could not be read: {}",
            path.display(),
            error.kind()
        )
    };
    let metadata = tokio::fs::metadata(path).await.map_err(unreadable)?;
    if metadata.len() > MAX_TOKEN_FILE_BYTES {
        return Err(format!("--server-token-file {} is too large to be a token", path.display()).into());
    }
    let contents = tokio::fs::read_to_string(path).await.map_err(unreadable)?;
    let token = contents.trim();
    if token.is_empty() {
        return Err(format!("--server-token-file {} is empty", path.display()).into());
    }
    Ok(token.to_owned())
}

async fn run(command: Option<Command>, mut settings: Settings) -> Result<(), Box<dyn Error>> {
    // Read here, so configuration itself stays free of I/O. The error names the
    // flag and the path, never anything read from the file.
    if let Some(path) = settings.server_token_file.take() {
        settings.server_token = Some(read_token_file(&path).await?);
    }

    // `vars_os`, because `vars` panics on a non-Unicode value in any variable,
    // including ones this server does not read. Such a value is left out here,
    // so a variable this server needs is reported as unset.
    let environment: Environment = env::vars_os()
        .filter_map(|(name, value)| Some((name.into_string().ok()?, value.into_string().ok()?)))
        .collect();
    let config = Config::try_from(Sources {
        settings: &settings,
        environment: &environment,
    })?;

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
        tracing::warn!("--skip-verify is set: the token was not checked against Postmark");
    } else {
        let server = verify(api.as_ref()).await?;
        tracing::info!(server_id = server.id, "token accepted by Postmark");
    }

    let log = ToolLog::open(config.log_file.as_deref(), config.log_email_full)
        .map_err(|error| format!("--log-file (or LOG_FILE) could not be opened: {error}"))?;
    let server = PostmarkMcpServer::new(api, ServerDefaults::from(config))
        .with_policy(ServerPolicy::from(config))
        .with_tool_log(log);
    let running = match server.serve(stdio()).await {
        Ok(running) => running,
        // Input ended before the client chose a lifecycle, either at once or
        // after only `server/discover`, which does not choose one. Hosts that
        // start every configured server and close those a session never used do
        // exactly this, so it is a normal shutdown, as closing after use is.
        Err(ServerInitializeError::ConnectionClosed(_)) => {
            tracing::info!("the client closed the connection without using the server");
            return Ok(());
        }
        Err(error) => return Err(error.into()),
    };
    running.waiting().await?;
    Ok(())
}

async fn verify(api: &PostmarkApi) -> Result<ServerSummary, Box<dyn Error>> {
    api.current_server()
        .await
        .map_err(|error: GatewayError| format!("{error}. {}", error.remediation()).into())
}
