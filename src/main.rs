// [main.rs] - KRUST - Complete fixed version
use anyhow::{anyhow, Context, Result};
use clap::Parser;
use std::time::Duration;
use tokio::sync::Semaphore;
use tracing::{info, error}; // 👈 FIXED: Removed unused 'warn'
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, Layer}; // 👈 FIXED: Added Layer trait
use uuid::Uuid;
use chrono::{DateTime, Utc};

mod modules;
mod ssh_executor;

use crate::ssh_executor::{SshAuth, SshHost};

#[derive(Parser, Debug)]
#[command(name = "krust", version, about = "Fast remote execution CLI built in Rust")] // 👈 RENAMED to KRUST
pub struct Cli {
    #[arg(short, long, default_value = "root")]
    pub user: String,
    #[arg(short, long)]
    pub password: Option<String>,
    #[arg(long)]
    pub ask_pass: bool,
    #[arg(short = 'k', long)]
    pub private_key: Option<String>,
    #[arg(long)]
    pub cert: Option<String>,
    #[arg(short, long = "hosts", value_delimiter = ',')]
    pub target_hosts: Vec<String>,
    #[arg(short, long)]
    pub inventory: Option<String>,
    #[arg(short, long, default_value_t = 10)]
    pub concurrency: usize,
    #[arg(long, default_value = "30s", value_parser = parse_duration_from_str)]
    pub timeout: Duration,
    #[arg(long, default_value_t = 0)]
    pub retries: u8,
    #[arg(long)]
    pub json_lines: bool,
    #[arg(long)]
    pub batch: bool,
    #[arg(short, long)]
    pub verbose: bool,
    #[arg(required = true, trailing_var_arg = true, name = "MODULE_OR_COMMAND")]
    pub module_or_cmd_and_args: Vec<String>,
}

fn parse_duration_from_str(s: &str) -> Result<Duration, String> {
    let s_lower = s.to_lowercase();
    let trimmed_s = s_lower.trim_end_matches(|c: char| c.is_alphabetic());
    let value = trimmed_s.parse::<u64>().map_err(|_| format!("Invalid duration value in '{}'", s))?;
    match &s_lower {
        s if s.ends_with('s') => Ok(Duration::from_secs(value)),
        s if s.ends_with('m') => Ok(Duration::from_secs(value * 60)),
        s if s.ends_with('h') => Ok(Duration::from_secs(value * 60 * 60)),
        _ => Ok(Duration::from_secs(value)),
    }
}

#[derive(serde::Serialize, Debug)]
struct HostResult {
    request_id: String,
    host: String,
    command: String,
    status: String,
    stdout: Option<String>,
    stderr: Option<String>,
    exit_code: Option<i32>,
    duration_ms: u128,
    retries_attempted: u8,
    error_type: Option<String>,
    timestamp: DateTime<Utc>,
}

fn setup_logging(args: &Cli) -> Result<()> {
    let format_layer = if args.json_lines {
        tracing_subscriber::fmt::layer()
            .json()
            .flatten_event(true)
            .with_current_span(false)
            .boxed() // 👈 FIXED: Now works with Layer trait imported
    } else {
        tracing_subscriber::fmt::layer()
            .with_target(false)
            .with_thread_ids(false)
            .boxed() // 👈 FIXED: Now works with Layer trait imported
    };

    let filter_layer = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| {
            if args.verbose {
                "debug".into()
            } else {
                "info".into()
            }
        });

    tracing_subscriber::registry()
        .with(filter_layer)
        .with(format_layer)
        .init();

    Ok(())
}

async fn run_tasks(args: &Cli, hosts: Vec<SshHost>, command_to_run: String) -> Result<Vec<HostResult>> {
    let mut results = Vec::new();
    let request_id = Uuid::new_v4().to_string();
    let _semaphore = std::sync::Arc::new(Semaphore::new(args.concurrency));
    
    info!(
        request_id = %request_id,
        command = %command_to_run,
        host_count = hosts.len(),
        concurrency = args.concurrency,
        timeout = ?args.timeout,
        retries = args.retries,
        "Starting execution batch"
    );

    let mut final_password = args.password.clone();
    if args.ask_pass && final_password.is_none() {
        final_password = Some(
            rpassword::prompt_password("Enter SSH password: ")
                .context("Failed to read password from prompt")?
        );
    }

    let auth = SshAuth::new(
        args.user.clone(),
        final_password,
        args.private_key.clone(),
        args.cert.clone(),
        true,
    )?;

    for host in hosts.into_iter().take(2) {
        let start_time = std::time::Instant::now();
        let addr = host.hostname.clone();
        
        info!(
            request_id = %request_id,
            host = %addr,
            "Starting SSH execution"
        );
        
        match ssh_executor::execute_command_on_host(&host, &auth, &command_to_run, args.timeout, args.retries) {
            Ok(real_output) => {
                let duration = start_time.elapsed();
                
                info!(
                    request_id = %request_id,
                    host = %addr,
                    duration_ms = duration.as_millis(),
                    exit_code = 0,
                    "Command executed successfully"
                );
                
                results.push(HostResult {
                    request_id: request_id.clone(),
                    host: addr,
                    command: command_to_run.clone(),
                    status: "success".to_string(),
                    stdout: Some(real_output),
                    stderr: None,
                    exit_code: Some(0),
                    duration_ms: duration.as_millis(),
                    retries_attempted: 0,
                    error_type: None,
                    timestamp: Utc::now(),
                });
            }
            Err(e) => {
                let duration = start_time.elapsed();
                
                error!(
                    request_id = %request_id,
                    host = %addr,
                    error = %e,
                    duration_ms = duration.as_millis(),
                    "SSH execution failed"
                );
                
                results.push(HostResult {
                    request_id: request_id.clone(),
                    host: addr,
                    command: command_to_run.clone(),
                    status: "error".to_string(),
                    stdout: None,
                    stderr: Some(e.to_string()),
                    exit_code: None,
                    duration_ms: duration.as_millis(),
                    retries_attempted: 0,
                    error_type: Some("SSHError".to_string()),
                    timestamp: Utc::now(),
                });
            }
        }
    }

    if results.is_empty() && !command_to_run.is_empty() {
        results.push(HostResult {
            request_id: request_id.clone(),
            host: "dummy.host".to_string(),
            command: command_to_run.clone(),
            status: "error".to_string(),
            stdout: None,
            stderr: Some("No hosts available for execution".to_string()),
            exit_code: None,
            duration_ms: 0,
            retries_attempted: 0,
            error_type: Some("NoHosts".to_string()),
            timestamp: Utc::now(),
        });
    }

    Ok(results)
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Cli::parse();

    setup_logging(&args)?;

    if args.verbose {
        info!("KRUST CLI Args: {:?}", args);
    }

    let mut final_password = args.password.clone();
    if args.ask_pass && final_password.is_none() {
        final_password = Some(
            rpassword::prompt_password("Enter SSH password: ")
                .context("Failed to read password from prompt")?
        );
    }

    let _auth = SshAuth::new(
        args.user.clone(),
        final_password,
        args.private_key.clone(),
        args.cert.clone(),
        true,
    )?;

    let mut hosts_list: Vec<SshHost> = args.target_hosts.iter()
        .map(|h| SshHost::from_target(h, None))
        .collect::<Result<Vec<SshHost>>>()
        .context("Invalid target host format")?;

    if let Some(inventory_path) = &args.inventory {
        let inventory_content = std::fs::read_to_string(inventory_path)
            .with_context(|| format!("Failed to read inventory file: {}", inventory_path))?;
        for line in inventory_content.lines() {
            let trimmed_line = line.trim();
            if !trimmed_line.is_empty() && !trimmed_line.starts_with('#') {
                let host = SshHost::from_target(trimmed_line, None)?;
                hosts_list.push(host);
            }
        }
    }

    hosts_list.sort();
    hosts_list.dedup();

    if hosts_list.is_empty() {
        return Err(anyhow!("No target hosts specified. Use --hosts or --inventory."));
    }

    if args.verbose {
        info!("Target hosts: {:?}", hosts_list.iter().map(|h| &h.hostname).collect::<Vec<_>>());
    }

    let (first_arg, remaining_args) = args.module_or_cmd_and_args
        .split_first()
        .ok_or_else(|| anyhow!("No module or command provided. Use '--help' for usage."))?;

    let command_to_run = match first_arg.as_str() {
        "sudo" => modules::sudo::build_command(remaining_args)
            .context("Failed to build 'sudo' module command")?,
        "os-update" => modules::os_update::build_command(remaining_args)
            .context("Failed to build 'os-update' module command")?,
        "reboot-wait" => modules::reboot_wait::build_command(remaining_args)
            .context("Failed to build 'reboot-wait' module command")?,
        "collect-facts" => modules::collect_facts::build_command(remaining_args)
            .context("Failed to build 'collect-facts' module command")?,
        _ => args.module_or_cmd_and_args.join(" "),
    };

    if args.verbose {
        info!("Resolved command to execute: {}", &command_to_run);
    }

    let results = run_tasks(&args, hosts_list, command_to_run).await?;

    if args.json_lines {
        for result in results {
            if let Ok(json_string) = serde_json::to_string(&result) {
                println!("{}", json_string);
            } else {
                eprintln!("[ERROR] Failed to serialize result for host: {}", result.host);
            }
        }
    } else if args.batch {
        let json_output = serde_json::to_string_pretty(&results)
            .context("Failed to serialize batch results to JSON")?;
        println!("{}", json_output);
    } else {
        let json_output = serde_json::to_string_pretty(&results)
            .context("Failed to serialize results to JSON")?;
        println!("{}", json_output);
    }

    Ok(())
}
