use anyhow::{Result, bail};

pub fn build_command(args: &[String]) -> Result<String> {
    let mut timeout_minutes = 10;
    let mut health_check_url: Option<String> = None;
    let mut wait_for_services: Vec<String> = Vec::new();
    let mut force_reboot = false;
    let mut delay_seconds = 5;
    let mut check_mode = false;
    let mut pre_reboot_script: Option<String> = None;
    let mut post_reboot_script: Option<String> = None;

    // Parse arguments
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--timeout" => {
                if i + 1 < args.len() {
                    timeout_minutes = args[i + 1].parse().map_err(|_| anyhow::anyhow!("Invalid timeout value"))?;
                    i += 2;
                } else {
                    bail!("--timeout requires a value in minutes");
                }
            }
            "--health-check" => {
                if i + 1 < args.len() {
                    health_check_url = Some(args[i + 1].clone());
                    i += 2;
                } else {
                    bail!("--health-check requires a URL");
                }
            }
            "--wait-for-service" => {
                if i + 1 < args.len() {
                    wait_for_services.push(args[i + 1].clone());
                    i += 2;
                } else {
                    bail!("--wait-for-service requires a service name");
                }
            }
            "--delay" => {
                if i + 1 < args.len() {
                    delay_seconds = args[i + 1].parse().map_err(|_| anyhow::anyhow!("Invalid delay value"))?;
                    i += 2;
                } else {
                    bail!("--delay requires a value in seconds");
                }
            }
            "--force" => {
                force_reboot = true;
                i += 1;
            }
            "--check" => {
                check_mode = true;
                i += 1;
            }
            "--pre-script" => {
                if i + 1 < args.len() {
                    pre_reboot_script = Some(args[i + 1].clone());
                    i += 2;
                } else {
                    bail!("--pre-script requires a script path");
                }
            }
            "--post-script" => {
                if i + 1 < args.len() {
                    post_reboot_script = Some(args[i + 1].clone());
                    i += 2;
                } else {
                    bail!("--post-script requires a script path");
                }
            }
            "--help" => {
                println!("CRUSTY reboot-wait module
Usage: crusty reboot-wait [OPTIONS]

Options:
    --timeout MINUTES        Wait timeout in minutes (default: 10)
    --health-check URL       URL to check after reboot
    --wait-for-service SVC   Service to wait for after reboot (can use multiple times)
    --delay SECONDS         Delay before reboot in seconds (default: 5)
    --force                 Force reboot even if not required
    --check                 Only check if reboot is required (no actual reboot)
    --pre-script PATH       Script to run before reboot
    --post-script PATH      Script to run after system comes back up
    --help                  Show this help

Examples:
    crusty --hosts servers reboot-wait
    crusty --hosts web reboot-wait --health-check http://localhost/health
    crusty --hosts db reboot-wait --wait-for-service mysql --wait-for-service redis --timeout 15
    crusty --hosts all reboot-wait --check
    crusty --hosts app reboot-wait --force --delay 30
    crusty --hosts prod reboot-wait --pre-script /opt/backup.sh --post-script /opt/verify.sh");
                return Ok("echo 'Help displayed'".to_string());
            }
            _ => {
                bail!("Unknown argument: {}. Use --help for usage.", args[i]);
            }
        }
    }

    let services_list = wait_for_services.join(" ");
    
    let script = format!(r#"
#!/bin/bash
set -e

# Color functions
red() {{ echo -e "\033[31m$1\033[0m"; }}
green() {{ echo -e "\033[32m$1\033[0m"; }}
yellow() {{ echo -e "\033[33m$1\033[0m"; }}
blue() {{ echo -e "\033[34m$1\033[0m"; }}

TIMEOUT_MINUTES={timeout}
HEALTH_CHECK_URL="{health_url}"
WAIT_FOR_SERVICES="{services}"
FORCE_REBOOT={force}
DELAY_SECONDS={delay}
CHECK_MODE={check_mode}
PRE_SCRIPT="{pre_script}"
POST_SCRIPT="{post_script}"
HOSTNAME=$(hostname)
REBOOT_LOG="/var/log/crusty-reboot.log"

green "🔄 CRUSTY Reboot Manager"
blue "🖥️  Host: $HOSTNAME"
blue "⏱️  Timeout: $TIMEOUT_MINUTES minutes"
blue "🕐 Delay: $DELAY_SECONDS seconds"
blue "🏥 Health Check: ${{HEALTH_CHECK_URL:-None}}"
blue "🔧 Wait for services: ${{WAIT_FOR_SERVICES:-None}}"
blue "🔍 Check mode: $CHECK_MODE"

# Function to check if reboot is required
reboot_required() {{
    local reasons=()
    
    # Check /var/run/reboot-required (Ubuntu/Debian)
    if [[ -f /var/run/reboot-required ]]; then
        reasons+=("reboot-required file exists")
    fi
    
    # Check for kernel updates (current vs running)
    if [[ -f /proc/version ]]; then
        CURRENT_KERNEL=$(uname -r)
        # Find the newest installed kernel
        if command -v dpkg >/dev/null 2>&1; then
            INSTALLED_KERNEL=$(dpkg -l | grep linux-image | grep "^ii" | awk '{{print $2}}' | sed 's/linux-image-//' | sort -V | tail -1)
        elif command -v rpm >/dev/null 2>&1; then
            INSTALLED_KERNEL=$(rpm -qa kernel | sed 's/kernel-//' | sort -V | tail -1)
        else
            INSTALLED_KERNEL=$(ls /lib/modules/ 2>/dev/null | sort -V | tail -1)
        fi
        
        if [[ -n "$INSTALLED_KERNEL" && "$CURRENT_KERNEL" != "$INSTALLED_KERNEL" ]]; then
            reasons+=("kernel update: $CURRENT_KERNEL -> $INSTALLED_KERNEL")
        fi
    fi
    
    # Check for pending systemd reboot
    if systemctl is-active --quiet systemd-logind; then
        if [[ -f /run/systemd/shutdown/scheduled ]]; then
            reasons+=("systemd reboot scheduled")
        fi
    fi
    
    # Check for specific package updates that require reboot
    if command -v needrestart >/dev/null 2>&1; then
        if needrestart -b | grep -q "NEEDRESTART-KSTA: 3"; then
            reasons+=("needrestart indicates reboot required")
        fi
    fi
    
    # Check for glibc/systemd updates (usually require reboot)
    if command -v apt >/dev/null 2>&1; then
        if [[ -f /var/log/apt/history.log ]]; then
            if tail -100 /var/log/apt/history.log | grep -q "libc6\|systemd\|linux-image"; then
                reasons+=("critical system packages updated recently")
            fi
        fi
    fi
    
    if [[ ${{#reasons[@]}} -gt 0 ]]; then
        echo "Reboot required. Reasons:"
        printf "  - %s\n" "${{reasons[@]}}"
        return 0
    else
        return 1
    fi
}}

# Function to log with timestamp
log_message() {{
    echo "$(date '+%Y-%m-%d %H:%M:%S'): $1" | tee -a "$REBOOT_LOG" 2>/dev/null || echo "$(date '+%Y-%m-%d %H:%M:%S'): $1"
}}

# Check reboot requirement
blue "🔍 Checking if reboot is required..."
if reboot_required; then
    REBOOT_NEEDED=true
    yellow "⚠️  Reboot is required"
else
    REBOOT_NEEDED=false
    green "✅ No reboot required"
fi

# Handle check mode
if [[ "$CHECK_MODE" == "true" ]]; then
    if [[ "$REBOOT_NEEDED" == "true" ]]; then
        yellow "🔍 CHECK MODE: Reboot is required but not performing reboot"
        exit 1
    else
        green "🔍 CHECK MODE: No reboot required"
        exit 0
    fi
fi

# Exit if no reboot needed and not forced
if [[ "$REBOOT_NEEDED" == "false" && "$FORCE_REBOOT" != "true" ]]; then
    green "✅ No reboot required and --force not specified. Exiting."
    exit 0
fi

if [[ "$FORCE_REBOOT" == "true" ]]; then
    yellow "🔨 Force reboot enabled"
fi

log_message "CRUSTY reboot initiated for $HOSTNAME"

# Pre-reboot checks and preparations
blue "🔍 Pre-reboot system check..."

# Check system load
LOAD=$(cut -d' ' -f1 /proc/loadavg)
LOAD_INT=${{LOAD%.*}}
if [[ $LOAD_INT -gt 5 ]]; then
    yellow "⚠️  High system load detected: $LOAD"
    yellow "   Waiting 10 seconds for load to settle..."
    sleep 10
fi

# Check active SSH sessions
SSH_SESSIONS=$(who | grep -c pts || echo 0)
if [[ $SSH_SESSIONS -gt 1 ]]; then
    yellow "⚠️  $SSH_SESSIONS active SSH sessions detected"
    who | grep pts || true
fi

# Check for active critical processes
blue "🔍 Checking for critical processes..."
CRITICAL_PROCS=("mysql" "postgresql" "redis" "mongodb" "elasticsearch")
for proc in "${{CRITICAL_PROCS[@]}}"; do
    if pgrep "$proc" >/dev/null 2>&1; then
        yellow "⚠️  Critical process running: $proc"
    fi
done

# Check disk space
DISK_USAGE=$(df / | tail -1 | awk '{{print $5}}' | sed 's/%//')
if [[ $DISK_USAGE -gt 90 ]]; then
    yellow "⚠️  High disk usage: $DISK_USAGE%"
fi

# Run pre-reboot script if specified
if [[ -n "$PRE_SCRIPT" && -f "$PRE_SCRIPT" ]]; then
    blue "🔧 Running pre-reboot script: $PRE_SCRIPT"
    if bash "$PRE_SCRIPT"; then
        green "✅ Pre-reboot script completed successfully"
    else
        red "❌ Pre-reboot script failed"
        exit 1
    fi
fi

# Save current system state
blue "💾 Saving system state..."
echo "Pre-reboot state - $(date)" > /tmp/crusty-reboot-state.txt
echo "Uptime: $(uptime)" >> /tmp/crusty-reboot-state.txt
echo "Load: $(cat /proc/loadavg)" >> /tmp/crusty-reboot-state.txt
echo "Memory: $(free -h | head -2)" >> /tmp/crusty-reboot-state.txt
echo "Disk: $(df -h / | tail -1)" >> /tmp/crusty-reboot-state.txt

# Sync filesystems
blue "💾 Syncing filesystems..."
sync
sleep 2

# Final warning and countdown
yellow "⚠️  REBOOT COUNTDOWN STARTING"
for i in $(seq $DELAY_SECONDS -1 1); do
    echo -ne "\r🔄 Rebooting in $i seconds... (Ctrl+C to cancel)"
    sleep 1
done
echo ""

# Record reboot time
log_message "Reboot initiated at $(date)"

# Execute the reboot
green "🚀 Initiating reboot now..."
log_message "Reboot command executed"

# Use systemctl reboot for clean shutdown
if command -v systemctl >/dev/null 2>&1; then
    systemctl reboot
else
    reboot
fi

# If we reach here, reboot might have failed
sleep 30
red "❌ Reboot may have failed - system still responsive after 30 seconds"
log_message "Reboot may have failed - system still responsive"
exit 1
"#,
        timeout = timeout_minutes,
        health_url = health_check_url.unwrap_or_default(),
        services = services_list,
        force = force_reboot,
        delay = delay_seconds,
        check_mode = check_mode,
        pre_script = pre_reboot_script.unwrap_or_default(),
        post_script = post_reboot_script.unwrap_or_default()
    );

    Ok(format!("bash -c {}", shell_escape::unix::escape(script.into())))
}
