// [main.rs]
use anyhow::{anyhow, Context, Result};
use clap::Parser;
use std::time::Duration;
use tokio::sync::Semaphore;

mod modules;
mod ssh_executor;

use crate::ssh_executor::{SshAuth, SshHost};

#[derive(Parser, Debug)]
#[command(name = "crusty", version, about = "Modular, high-performance remote automation CLI built in Rust.")]
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
    host: String,
    command: String,
    status: String,
    stdout: Option<String>,
    stderr: Option<String>,
    exit_code: Option<i32>,
    duration_ms: u128,
    retries_attempted: u8,
    error_type: Option<String>,
}

async fn run_tasks(args: &Cli, hosts: Vec<SshHost>, command_to_run: String) -> Result<Vec<HostResult>> {
    let mut results = Vec::new();
    let semaphore = std::sync::Arc::new(Semaphore::new(args.concurrency));
    println!("Simulating execution of '{}' on {} hosts (concurrency: {}, timeout: {:?}, retries: {}).",
        command_to_run, hosts.len(), args.concurrency, args.timeout, args.retries
    );

    for host in hosts.into_iter().take(2) {
        let addr = host.hostname.clone();
        results.push(HostResult {
            host: addr.clone(),
            command: command_to_run.clone(),
            status: "success".to_string(),
            stdout: Some(format!("Fake output from {}", addr)),
            stderr: None,
            exit_code: Some(0),
            duration_ms: 500,
            retries_attempted: 0,
            error_type: None,
     });
    }

    if results.is_empty() && !command_to_run.is_empty() {
        results.push(HostResult {
            host: "dummy.host".to_string(),
            command: command_to_run.clone(),
            status: "error".to_string(),
            stdout: None,
            stderr: Some("SSH execution not implemented".to_string()),
            exit_code: None,
            duration_ms: 0,
            retries_attempted: 0,
            error_type: Some("NotImplemented".to_string()),
        });
    }

    Ok(results)
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Cli::parse();

    if args.verbose {
        println!("Crusty CLI Args: {:?}", args);
    }

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
            hosts_list.push(host); // ✅ this is enough

            }
        }
    }

    hosts_list.sort();
    hosts_list.dedup();

    if hosts_list.is_empty() {
        return Err(anyhow!("No target hosts specified. Use --hosts or --inventory."));
    }

    if args.verbose {
        println!("Target hosts:");
        for host in &hosts_list {
            println!("  - {}", host.hostname);
        }
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
        println!("Resolved command to execute: {}", &command_to_run);
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

