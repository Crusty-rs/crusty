// [main.rs] - KRUST - Pure SSH Command Executor
use anyhow::{anyhow, Context, Result};
use clap::Parser;
use std::time::Duration;
use tokio::sync::Semaphore;
use tokio::time::timeout;
use tracing::{info, debug, error};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use chrono::{DateTime, Utc};
use std::sync::Arc;
use futures::stream::{FuturesUnordered, StreamExt};
use std::io::{stdout, IsTerminal};

mod ssh_executor;
use crate::ssh_executor::{SshAuth, SshHost};

#[derive(Parser, Debug)]
#[command(name = "krust", version, about = "Pure parallel SSH command executor")]
pub struct Cli {
    /// SSH username (defaults to current user)
    #[arg(short, long)]
    pub user: Option<String>,
    
    /// SSH password (use --ask-pass for interactive)
    #[arg(short, long)]
    pub password: Option<String>,
    
    /// Prompt for password interactively
    #[arg(long)]
    pub ask_pass: bool,
    
    /// Path to SSH private key
    #[arg(short = 'k', long)]
    pub private_key: Option<String>,
    
    /// Target hosts (comma-separated)
    #[arg(short, long = "hosts", value_delimiter = ',')]
    pub target_hosts: Vec<String>,
    
    /// Read hosts from inventory file
    #[arg(short, long)]
    pub inventory: Option<String>,
    
    /// Maximum concurrent connections
    #[arg(short, long, default_value_t = 10)]
    pub concurrency: usize,
    
    /// Command timeout (e.g., 30s, 5m, 1h)
    #[arg(long, default_value = "30s", value_parser = parse_duration)]
    pub timeout: Duration,
    
    /// Number of retries for failed connections
    #[arg(long, default_value_t = 3)]
    pub retries: u8,
    
    /// Output as NDJSON (one line per host)
    #[arg(long)]
    pub json: bool,
    
    /// Output as pretty-printed JSON
    #[arg(long, conflicts_with = "json")]
    pub pretty_json: bool,
    
    /// Select output fields (comma-separated: hostname,success,stdout,stderr,exit_code,duration_ms)
    #[arg(long, value_delimiter = ',')]
    pub fields: Option<Vec<String>>,
    
    /// Enable verbose logging
    #[arg(short, long)]
    pub verbose: bool,
    
    /// Disable color output (auto-detected for pipes)
    #[arg(long)]
    pub no_color: bool,
    
    /// Command to execute on remote hosts
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
    success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    stdout: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stdout_lines: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stderr: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    exit_code: Option<i32>,
    timestamp: DateTime<Utc>,
    duration_ms: u64,
}

impl HostResult {
    fn filter_fields(&self, fields: &[String]) -> serde_json::Value {
        let mut map = serde_json::Map::new();
        
        for field in fields {
            match field.as_str() {
                "hostname" | "host" => {
                    map.insert("hostname".to_string(), serde_json::json!(self.hostname));
                }
                "success" => {
                    map.insert("success".to_string(), serde_json::json!(self.success));
                }
                "stdout" => {
                    if let Some(ref stdout) = self.stdout {
                        map.insert("stdout".to_string(), serde_json::json!(stdout));
                    }
                }
                "stdout_lines" => {
                    if let Some(ref lines) = self.stdout_lines {
                        map.insert("stdout_lines".to_string(), serde_json::json!(lines));
                    }
                }
                "stderr" => {
                    if let Some(ref stderr) = self.stderr {
                        map.insert("stderr".to_string(), serde_json::json!(stderr));
                    }
                }
                "exit_code" => {
                    if let Some(code) = self.exit_code {
                        map.insert("exit_code".to_string(), serde_json::json!(code));
                    }
                }
                "timestamp" => {
                    map.insert("timestamp".to_string(), serde_json::json!(self.timestamp));
                }
                "duration_ms" | "duration" => {
                    map.insert("duration_ms".to_string(), serde_json::json!(self.duration_ms));
                }
                _ => {}
            }
        }
        
        serde_json::Value::Object(map)
    }
}

fn setup_logging(args: &Cli) {
    // If JSON output is requested, only show errors to avoid mixing with JSON
    let filter = if args.json || args.pretty_json {
        "error"
    } else if args.verbose {
        "debug"
    } else {
        "info"
    };
    
    let fmt_layer = tracing_subscriber::fmt::layer()
        .with_target(false)
        .compact();
    
    // Apply color settings
    let fmt_layer = if args.no_color || !stdout().is_terminal() || args.json || args.pretty_json {
        fmt_layer.with_ansi(false)
    } else {
        fmt_layer.with_ansi(true)
    };
    
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new(filter))
        .with(fmt_layer)
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
            tokio::time::sleep(Duration::from_millis(500 * attempt as u64)).await;
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
                let duration_ms = (Utc::now() - start).num_milliseconds() as u64;
                let stdout_lines = if output.contains('\n') {
                    Some(output.lines().map(|s| s.to_string()).collect())
                } else {
                    None
                };
                
                return HostResult {
                    hostname: host.hostname,
                    success: exit_code == 0,
                    stdout: Some(output),
                    stdout_lines,
                    stderr: None,
                    exit_code: Some(exit_code),
                    timestamp: start,
                    duration_ms,
                };
            }
            Ok(Ok(Err(e))) => {
                last_error = Some(e.to_string());
                // Retry on connection/network errors
                let err_str = e.to_string().to_lowercase();
                if !err_str.contains("connection") && 
                   !err_str.contains("timeout") &&
                   !err_str.contains("network") &&
                   !err_str.contains("handshake") {
                    error!("Non-retryable error for {}: {}", host.hostname, e);
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
    
    let duration_ms = (Utc::now() - start).num_milliseconds() as u64;
    HostResult {
        hostname: host.hostname,
        success: false,
        stdout: None,
        stdout_lines: None,
        stderr: last_error,
        exit_code: None,
        timestamp: start,
        duration_ms,
    }
}

async fn run_parallel(
    hosts: Vec<SshHost>,
    auth: Arc<SshAuth>,
    command: String,
    semaphore: Arc<Semaphore>,
    timeout: Duration,
    retries: u8,
    args: &Cli,
) -> (Vec<HostResult>, i32) {
    let mut tasks = FuturesUnordered::new();
    let total_hosts = hosts.len();
    let use_json = args.json || args.pretty_json;
    let use_color = !args.no_color && stdout().is_terminal() && !use_json;
    
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
    
    // Track results for summary
    let mut results = Vec::with_capacity(total_hosts);
    let mut completed = 0;
    let mut failed_count = 0;
    
    // Clear line for progress updates
    if !use_json {
        eprint!("\r\x1b[K");
    }
    
    while let Some(result) = tasks.next().await {
        completed += 1;
        if !result.success {
            failed_count += 1;
        }
        
        if args.json {
            // Stream NDJSON immediately
            let output = if let Some(ref fields) = args.fields {
                result.filter_fields(fields)
            } else {
                serde_json::to_value(&result).unwrap()
            };
            
            if let Ok(json) = serde_json::to_string(&output) {
                println!("{}", json);
            }
        } else if args.pretty_json {
            // Collect for pretty printing later
        } else {
            // Stream text output immediately
            print_single_result(&result, use_color);
            
            // Update progress
            if completed < total_hosts {
                if use_color {
                    eprint!("\r\x1b[K\x1b[90m[{}/{}] completed, {} failed\x1b[0m", 
                           completed, total_hosts, failed_count);
                } else {
                    eprint!("\r[{}/{}] completed, {} failed", 
                           completed, total_hosts, failed_count);
                }
            }
        }
        
        results.push(result);
    }
    
    if !use_json {
        eprintln!("\r\x1b[K"); // Clear progress line
    }
    
    // Return results and exit code
    let exit_code = if failed_count > 0 { 1 } else { 0 };
    (results, exit_code)
}

fn print_single_result(result: &HostResult, use_color: bool) {
    if result.success {
        if use_color {
            print!("\x1b[32m✓\x1b[0m \x1b[1m{}\x1b[0m ", result.hostname);
        } else {
            print!("OK {} ", result.hostname);
        }
        
        if use_color {
            print!("\x1b[90m({}ms)\x1b[0m", result.duration_ms);
        } else {
            print!("({}ms)", result.duration_ms);
        }
        
        if let Some(ref output) = result.stdout {
            let lines: Vec<&str> = output.lines().collect();
            if lines.len() == 1 && lines[0].len() < 80 {
                // Single short line - print inline
                println!(": {}", lines[0]);
            } else if lines.is_empty() {
                println!();
            } else {
                // Multi-line or long output
                println!(":");
                for line in lines.iter().take(5) {
                    println!("  {}", line);
                }
                if lines.len() > 5 {
                    if use_color {
                        println!("  \x1b[90m... {} more lines\x1b[0m", lines.len() - 5);
                    } else {
                        println!("  ... {} more lines", lines.len() - 5);
                    }
                }
            }
        } else {
            println!();
        }
    } else {
        // Failed result
        if use_color {
            print!("\x1b[31m✗\x1b[0m \x1b[1m{}\x1b[0m", result.hostname);
        } else {
            print!("FAIL {} ", result.hostname);
        }
        
        if let Some(ref err) = result.stderr {
            let err_preview = if err.len() > 100 {
                format!("{}...", &err[..100])
            } else {
                err.to_string()
            };
            
            if use_color {
                println!(": \x1b[31m{}\x1b[0m", err_preview);
            } else {
                println!(": {}", err_preview);
            }
        } else {
            println!(": Unknown error");
        }
    }
}


#[tokio::main]
async fn main() -> Result<()> {
    let args = Cli::parse();
    setup_logging(&args);
    
    let command = args.command.join(" ");
    debug!("Command to execute: {}", command);
    
    // Get system username for default
    let system_user = std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_else(|_| "root".to_string());
    
    let ssh_user = args.user.as_ref().unwrap_or(&system_user);
    
    // Get password if needed
    let password = if args.ask_pass && args.password.is_none() {
        Some(rpassword::prompt_password("SSH password: ")?)
    } else {
        args.password.clone()
    };
    
    // Build auth
    let auth = Arc::new(SshAuth::new(
        ssh_user.to_string(),
        password,
        args.private_key.clone(),
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
    
    // Deduplicate and sort
    hosts.sort();
    hosts.dedup();
    
    if hosts.is_empty() {
        return Err(anyhow!("No hosts specified"));
    }
    
    if !args.json && !args.pretty_json {
        info!("Executing on {} hosts with concurrency {} as user {}", 
              hosts.len(), args.concurrency, ssh_user);
    }
    
    // Execute
    let semaphore = Arc::new(Semaphore::new(args.concurrency));
    let (results, exit_code) = run_parallel(
        hosts,
        auth,
        command,
        semaphore,
        args.timeout,
        args.retries,
        &args,
    ).await;
    
    // Output final summary or pretty JSON
    if args.pretty_json {
        // Pretty print all results at once
        let output: Vec<_> = if let Some(ref fields) = args.fields {
            results.iter().map(|r| r.filter_fields(fields)).collect()
        } else {
            results.iter().map(|r| serde_json::to_value(r).unwrap()).collect()
        };
        
        if let Ok(json) = serde_json::to_string_pretty(&output) {
            println!("{}", json);
        }
    } else if !args.json {
        // Print summary for text output
        let use_color = !args.no_color && stdout().is_terminal();
        print_summary(&results, use_color);
    }
    
    std::process::exit(exit_code);
}

fn print_summary(results: &[HostResult], use_color: bool) {
    let total = results.len();
    let successful = results.iter().filter(|r| r.success).count();
    let failed = total - successful;
    
    println!();
    if use_color {
        println!("\x1b[1mSummary:\x1b[0m {} total, \x1b[32m{} succeeded\x1b[0m, \x1b[31m{} failed\x1b[0m", 
                 total, successful, failed);
    } else {
        println!("Summary: {} total, {} succeeded, {} failed", total, successful, failed);
    }
}
