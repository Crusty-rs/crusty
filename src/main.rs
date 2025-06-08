// [main.rs] - KRUST MVP - Hardened & Simplified
use anyhow::{anyhow, Context, Result};
use clap::Parser;
use std::time::Duration;
use tokio::sync::Semaphore;
use tokio::time::timeout;
use tracing::{info, error, debug};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use chrono::{DateTime, Utc};
use std::sync::Arc;
use futures::stream::{FuturesUnordered, StreamExt};

mod modules;
mod ssh_executor;

use crate::ssh_executor::{SshAuth, SshHost};

#[derive(Parser, Debug)]
#[command(name = "krust", version, about = "Fast parallel SSH execution")]
pub struct Cli {
    #[arg(short, long, default_value = "root")]
    pub user: String,
    
    #[arg(short, long)]
    pub password: Option<String>,
    
    #[arg(long)]
    pub ask_pass: bool,
    
    #[arg(short = 'k', long)]
    pub private_key: Option<String>,
    
    #[arg(short, long = "hosts", value_delimiter = ',')]
    pub target_hosts: Vec<String>,
    
    #[arg(short, long)]
    pub inventory: Option<String>,
    
    #[arg(short, long, default_value_t = 10)]
    pub concurrency: usize,
    
    #[arg(long, default_value = "30s", value_parser = parse_duration)]
    pub timeout: Duration,
    
    #[arg(long, default_value_t = 3)]
    pub retries: u8,
    
    #[arg(long)]
    pub json: bool,
    
    #[arg(short, long)]
    pub verbose: bool,
    
    #[arg(required = true, trailing_var_arg = true)]
    pub command: Vec<String>,
}

fn parse_duration(s: &str) -> Result<Duration, String> {
    let s = s.to_lowercase();
    let num_part = s.trim_end_matches(char::is_alphabetic);
    let value = num_part.parse::<u64>()
        .map_err(|_| format!("Invalid duration: {}", s))?;
    
    match s.chars().last() {
        Some('s') => Ok(Duration::from_secs(value)),
        Some('m') => Ok(Duration::from_secs(value * 60)),
        Some('h') => Ok(Duration::from_secs(value * 3600)),
        _ => Ok(Duration::from_secs(value)),
    }
}

#[derive(serde::Serialize, Debug)]
struct HostResult {
    hostname: String,
    stdout: Option<String>,
    stderr: Option<String>,
    exit_code: Option<i32>,
    timestamp: DateTime<Utc>,
}

fn setup_logging(verbose: bool) {
    let filter = if verbose { "debug" } else { "info" };
    
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new(filter))
        .with(tracing_subscriber::fmt::layer()
            .with_target(false)
            .compact())
        .init();
}

async fn execute_with_retries(
    host: SshHost,
    auth: Arc<SshAuth>,
    command: String,
    timeout_duration: Duration,
    max_retries: u8,
) -> HostResult {
    let start = Utc::now();
    let mut last_error = None;
    
    for attempt in 0..=max_retries {
        if attempt > 0 {
            debug!("Retry {}/{} for {}", attempt, max_retries, host.hostname);
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
        
        match timeout(
            timeout_duration,
            tokio::task::spawn_blocking({
                let host = host.clone();
                let auth = Arc::clone(&auth);
                let cmd = command.clone();
                move || ssh_executor::execute_command_on_host(&host, &auth, &cmd)
            })
        ).await {
            Ok(Ok(Ok((output, exit_code)))) => {
                return HostResult {
                    hostname: host.hostname,
                    stdout: Some(output),
                    stderr: None,
                    exit_code: Some(exit_code),
                    timestamp: start,
                };
            }
            Ok(Ok(Err(e))) => {
                last_error = Some(e.to_string());
                // Only retry on connection/auth errors
                if !e.to_string().contains("connection") && 
                   !e.to_string().contains("timeout") {
                    break;
                }
            }
            Ok(Err(e)) => {
                last_error = Some(format!("Task panic: {}", e));
                break;
            }
            Err(_) => {
                last_error = Some("Command timeout".to_string());
            }
        }
    }
    
    HostResult {
        hostname: host.hostname,
        stdout: None,
        stderr: last_error,
        exit_code: None,
        timestamp: start,
    }
}

async fn run_parallel(
    hosts: Vec<SshHost>,
    auth: Arc<SshAuth>,
    command: String,
    semaphore: Arc<Semaphore>,
    timeout: Duration,
    retries: u8,
    stream_output: bool,
) -> Vec<HostResult> {
    let mut tasks = FuturesUnordered::new();
    let total_hosts = hosts.len();
    
    // Launch all tasks
    for host in hosts {
        let sem = Arc::clone(&semaphore);
        let auth = Arc::clone(&auth);
        let cmd = command.clone();
        
        tasks.push(async move {
            let _permit = sem.acquire().await.unwrap();
            execute_with_retries(host, auth, cmd, timeout, retries).await
        });
    }
    
    // Stream results as they complete
    let mut results = Vec::with_capacity(total_hosts);
    let mut completed = 0;
    
    while let Some(result) = tasks.next().await {
        completed += 1;
        
        if stream_output {
            // Print immediately
            if let Ok(json) = serde_json::to_string(&result) {
                println!("{}", json);
            }
        } else {
            // Show progress
            eprint!("\r[{}/{}] hosts completed", completed, total_hosts);
        }
        
        results.push(result);
    }
    
    if !stream_output {
        eprintln!(); // Clear progress line
    }
    
    results
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Cli::parse();
    setup_logging(args.verbose);
    
    // Resolve command
    let (module, module_args) = args.command.split_first()
        .ok_or_else(|| anyhow!("No command provided"))?;
    
    let command = match module.as_str() {
        "sudo" => modules::sudo::build_command(module_args)?,
        "os-update" => modules::os_update::build_command(module_args)?,
        "reboot-wait" => modules::reboot_wait::build_command(module_args)?,
        _ => args.command.join(" "),
    };
    
    debug!("Resolved command: {}", command);
    
    // Get password if needed
    let password = if args.ask_pass && args.password.is_none() {
        Some(rpassword::prompt_password("SSH password: ")?)
    } else {
        args.password
    };
    
    // Build auth
    let auth = Arc::new(SshAuth::new(
        args.user,
        password,
        args.private_key,
        None,
        true,
    )?);
    
    // Parse hosts
    let mut hosts = Vec::new();
    
    // From command line
    for host_spec in &args.target_hosts {
        hosts.push(SshHost::from_target(host_spec, None)?);
    }
    
    // From inventory file
    if let Some(inv_path) = &args.inventory {
        let content = std::fs::read_to_string(inv_path)
            .with_context(|| format!("Failed to read inventory: {}", inv_path))?;
        
        for line in content.lines() {
            let line = line.trim();
            if !line.is_empty() && !line.starts_with('#') {
                hosts.push(SshHost::from_target(line, None)?);
            }
        }
    }
    
    // Deduplicate
    hosts.sort();
    hosts.dedup();
    
    if hosts.is_empty() {
        return Err(anyhow!("No hosts specified"));
    }
    
    info!("Executing on {} hosts with concurrency {}", 
          hosts.len(), args.concurrency);
    
    // Execute
    let semaphore = Arc::new(Semaphore::new(args.concurrency));
    let results = run_parallel(
        hosts,
        auth,
        command,
        semaphore,
        args.timeout,
        args.retries,
        args.json,
    ).await;
    
    // Output results (if not streaming)
    if !args.json {
        let successful = results.iter()
            .filter(|r| r.exit_code == Some(0))
            .count();
        
        info!("Completed: {}/{} successful", successful, results.len());
        
        // Print failures
        for result in &results {
            if result.exit_code != Some(0) {
                error!("{}: {:?}", result.hostname, 
                       result.stderr.as_ref().unwrap_or(&"Unknown error".to_string()));
            }
        }
    }
    
    // Exit with error if any failed
    let all_success = results.iter().all(|r| r.exit_code == Some(0));
    if !all_success {
        std::process::exit(1);
    }
    
    Ok(())
}
