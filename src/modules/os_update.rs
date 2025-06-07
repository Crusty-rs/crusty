use anyhow::{Result, bail};

pub fn build_command(args: &[String]) -> Result<String> {
    let mut security_only = false;
    let mut auto_reboot = false;
    let mut create_snapshot = false;
    let mut exclude_packages: Vec<String> = Vec::new();
    let mut include_packages: Vec<String> = Vec::new();
    let mut pre_update_script: Option<String> = None;
    let mut post_update_script: Option<String> = None;
    let mut dry_run = false;
    let mut update_type = "full".to_string();
    let mut max_download_rate: Option<String> = None;
    let mut backup_configs = false;

    // Parse arguments
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--security-only" => {
                security_only = true;
                update_type = "security".to_string();
                i += 1;
            }
            "--auto-reboot" => {
                auto_reboot = true;
                i += 1;
            }
            "--snapshot" => {
                create_snapshot = true;
                i += 1;
            }
            "--exclude" => {
                if i + 1 < args.len() {
                    exclude_packages.extend(args[i + 1].split(',').map(|s| s.trim().to_string()));
                    i += 2;
                } else {
                    bail!("--exclude requires package names");
                }
            }
            "--include" => {
                if i + 1 < args.len() {
                    include_packages.extend(args[i + 1].split(',').map(|s| s.trim().to_string()));
                    i += 2;
                } else {
                    bail!("--include requires package names");
                }
            }
            "--pre-script" => {
                if i + 1 < args.len() {
                    pre_update_script = Some(args[i + 1].clone());
                    i += 2;
                } else {
                    bail!("--pre-script requires a script path");
                }
            }
            "--post-script" => {
                if i + 1 < args.len() {
                    post_update_script = Some(args[i + 1].clone());
                    i += 2;
                } else {
                    bail!("--post-script requires a script path");
                }
            }
            "--dry-run" => {
                dry_run = true;
                i += 1;
            }
            "--type" => {
                if i + 1 < args.len() {
                    update_type = args[i + 1].clone();
                    i += 2;
                } else {
                    bail!("--type requires a value (full|security|kernel|minimal)");
                }
            }
            "--max-download-rate" => {
                if i + 1 < args.len() {
                    max_download_rate = Some(args[i + 1].clone());
                    i += 2;
                } else {
                    bail!("--max-download-rate requires a value (e.g., 1000k, 5m)");
                }
            }
            "--backup-configs" => {
                backup_configs = true;
                i += 1;
            }
            "--help" => {
                println!("KRUST os-update module
Usage: krust os-update [OPTIONS]

Options:
    --security-only         Only install security updates
    --auto-reboot          Automatically reboot if required
    --snapshot             Create system snapshot before updating (LVM/Btrfs)
    --exclude PACKAGES     Exclude specific packages (comma-separated)
    --include PACKAGES     Include specific packages (comma-separated)
    --pre-script PATH      Run script before updates
    --post-script PATH     Run script after updates
    --dry-run             Show what would be updated without doing it
    --type TYPE           Update type: full, security, kernel, minimal
    --max-download-rate   Limit download speed (e.g., 1000k, 5m)
    --backup-configs      Backup configuration files before update
    --help                Show this help

Update Types:
    full      - All available updates (default)
    security  - Security updates only
    kernel    - Kernel and core system updates
    minimal   - Essential updates only

Examples:
    krust --hosts servers os-update
    krust --hosts prod os-update --security-only --auto-reboot
    krust --hosts db os-update --exclude mysql,postgresql --snapshot
    krust --hosts web os-update --dry-run --type security
    krust --hosts all os-update --backup-configs --max-download-rate 5m");
                return Ok("echo 'Help displayed'".to_string());
            }
            _ => {
                bail!("Unknown argument: {}. Use --help for usage.", args[i]);
            }
        }
    }

    // Validate update type
    if !["full", "security", "kernel", "minimal"].contains(&update_type.as_str()) {
        bail!("Invalid update type: {}. Use: full, security, kernel, minimal", update_type);
    }

    let exclude_clause = if !exclude_packages.is_empty() {
        format!("EXCLUDE_PACKAGES=({})", exclude_packages.iter().map(|p| format!("\"{}\"", p)).collect::<Vec<_>>().join(" "))
    } else {
        "EXCLUDE_PACKAGES=()".to_string()
    };

    let include_clause = if !include_packages.is_empty() {
        format!("INCLUDE_PACKAGES=({})", include_packages.iter().map(|p| format!("\"{}\"", p)).collect::<Vec<_>>().join(" "))
    } else {
        "INCLUDE_PACKAGES=()".to_string()
    };

    let script = format!(r#"
#!/bin/bash
set -e

# Color functions
red() {{ echo -e "\033[31m$1\033[0m"; }}
green() {{ echo -e "\033[32m$1\033[0m"; }}
yellow() {{ echo -e "\033[33m$1\033[0m"; }}
blue() {{ echo -e "\033[34m$1\033[0m"; }}

# Configuration
SECURITY_ONLY={security_only}
AUTO_REBOOT={auto_reboot}
CREATE_SNAPSHOT={create_snapshot}
DRY_RUN={dry_run}
UPDATE_TYPE="{update_type}"
MAX_DOWNLOAD_RATE="{max_rate}"
BACKUP_CONFIGS={backup_configs}
{exclude_clause}
{include_clause}
PRE_SCRIPT="{pre_script}"
POST_SCRIPT="{post_script}"
UPDATE_LOG="/var/log/krust-update.log"
HOSTNAME=$(hostname)

# Logging function
log_message() {{
    echo "$(date '+%Y-%m-%d %H:%M:%S'): $1" | tee -a "$UPDATE_LOG" 2>/dev/null || echo "$(date '+%Y-%m-%d %H:%M:%S'): $1"
}}

green "🚀 KRUST OS Update Starting..."
blue "📋 Configuration:"
echo "  Update Type: $UPDATE_TYPE"
echo "  Security Only: $SECURITY_ONLY"
echo "  Auto Reboot: $AUTO_REBOOT"
echo "  Create Snapshot: $CREATE_SNAPSHOT"
echo "  Dry Run: $DRY_RUN"
echo "  Backup Configs: $BACKUP_CONFIGS"
echo "  Excluded Packages: ${{EXCLUDE_PACKAGES[@]:-None}}"
echo "  Included Packages: ${{INCLUDE_PACKAGES[@]:-All}}"
echo "  Max Download Rate: ${{MAX_DOWNLOAD_RATE:-Unlimited}}"

log_message "KRUST OS update started on $HOSTNAME (type: $UPDATE_TYPE)"

# Pre-update system checks
blue "🔍 Pre-update system checks..."

# Check disk space
DISK_USAGE=$(df / | tail -1 | awk '{{print $5}}' | sed 's/%//')
AVAILABLE_GB=$(df -BG / | tail -1 | awk '{{print $4}}' | sed 's/G//')

if [[ $DISK_USAGE -gt 90 ]]; then
    red "❌ Critical: Disk usage is $DISK_USAGE% - aborting update"
    exit 1
elif [[ $DISK_USAGE -gt 80 ]]; then
    yellow "⚠️  Warning: High disk usage ($DISK_USAGE%)"
fi

if [[ $AVAILABLE_GB -lt 2 ]]; then
    red "❌ Critical: Less than 2GB free space available - aborting update"
    exit 1
fi

blue "💾 Disk space check: $DISK_USAGE% used, ${{AVAILABLE_GB}}GB available"

# Check system load
LOAD=$(cut -d' ' -f1 /proc/loadavg)
LOAD_INT=${{LOAD%.*}}
if [[ $LOAD_INT -gt 10 ]]; then
    yellow "⚠️  High system load: $LOAD - continuing with caution"
fi

# Check if package manager is locked
if command -v apt >/dev/null 2>&1; then
    if fuser /var/lib/dpkg/lock >/dev/null 2>&1; then
        red "❌ Package manager is locked by another process"
        exit 1
    fi
elif command -v yum >/dev/null 2>&1; then
    if [[ -f /var/run/yum.pid ]]; then
        red "❌ YUM is locked by another process"
        exit 1
    fi
fi

# Backup configuration files if requested
if [[ "$BACKUP_CONFIGS" == "true" ]]; then
    yellow "📦 Backing up configuration files..."
    BACKUP_DIR="/var/backups/krust-configs-$(date +%Y%m%d-%H%M%S)"
    mkdir -p "$BACKUP_DIR"
    
    # Common config directories to backup
    CONFIG_DIRS=("/etc" "/opt/*/conf" "/usr/local/etc")
    for dir in "${{CONFIG_DIRS[@]}}"; do
        if [[ -d "$dir" ]]; then
            cp -r "$dir" "$BACKUP_DIR/" 2>/dev/null || true
        fi
    done
    
    green "✅ Configuration backup created: $BACKUP_DIR"
    log_message "Configuration backup created: $BACKUP_DIR"
fi

# Create snapshot if requested
if [[ "$CREATE_SNAPSHOT" == "true" ]]; then
    yellow "📸 Creating system snapshot..."
    SNAPSHOT_NAME="krust-pre-update-$(date +%Y%m%d-%H%M%S)"
    
    # Try LVM snapshot
    if command -v lvcreate >/dev/null 2>&1; then
        ROOT_DEVICE=$(df / | tail -1 | awk '{{print $1}}')
        if [[ "$ROOT_DEVICE" =~ /dev/mapper/ ]]; then
            VG_NAME=$(lvs --noheadings -o vg_name "$ROOT_DEVICE" 2>/dev/null | tr -d ' ')
            LV_NAME=$(lvs --noheadings -o lv_name "$ROOT_DEVICE" 2>/dev/null | tr -d ' ')
            
            if [[ -n "$VG_NAME" && -n "$LV_NAME" ]]; then
                # Calculate snapshot size (10% of root volume or max 5GB)
                LV_SIZE=$(lvs --noheadings --units g -o lv_size "$ROOT_DEVICE" 2>/dev/null | tr -d ' ' | sed 's/g//')
                SNAPSHOT_SIZE=$(echo "$LV_SIZE * 0.1" | bc 2>/dev/null | cut -d. -f1)
                [[ $SNAPSHOT_SIZE -gt 5 ]] && SNAPSHOT_SIZE=5
                [[ $SNAPSHOT_SIZE -lt 1 ]] && SNAPSHOT_SIZE=1
                
                if lvcreate -L"${{SNAPSHOT_SIZE}}G" -s -n "$SNAPSHOT_NAME" "$ROOT_DEVICE" >/dev/null 2>&1; then
                    green "✅ LVM snapshot created: $SNAPSHOT_NAME (${{SNAPSHOT_SIZE}}GB)"
                    log_message "LVM snapshot created: $SNAPSHOT_NAME"
                else
                    yellow "⚠️  LVM snapshot creation failed, continuing..."
                fi
            fi
        fi
    # Try Btrfs snapshot
    elif mount | grep -q btrfs; then
        BTRFS_ROOT=$(mount | grep "on / " | grep btrfs | awk '{{print $1}}')
        if [[ -n "$BTRFS_ROOT" ]]; then
            mkdir -p "/.snapshots"
            if btrfs subvolume snapshot / "/.snapshots/$SNAPSHOT_NAME" >/dev/null 2>&1; then
                green "✅ Btrfs snapshot created: /.snapshots/$SNAPSHOT_NAME"
                log_message "Btrfs snapshot created: $SNAPSHOT_NAME"
            else
                yellow "⚠️  Btrfs snapshot creation failed, continuing..."
            fi
        fi
    # Try filesystem snapshot with rsync as fallback
    else
        SNAPSHOT_DIR="/var/backups/krust-snapshot-$(date +%Y%m%d-%H%M%S)"
        yellow "📁 Creating rsync-based backup snapshot..."
        if mkdir -p "$SNAPSHOT_DIR" && rsync -a --exclude=/proc --exclude=/sys --exclude=/dev --exclude=/var/backups / "$SNAPSHOT_DIR/" >/dev/null 2>&1; then
            green "✅ Rsync snapshot created: $SNAPSHOT_DIR"
            log_message "Rsync snapshot created: $SNAPSHOT_DIR"
        else
            yellow "⚠️  Snapshot creation failed, continuing without snapshot..."
        fi
    fi
fi

# Run pre-update script
if [[ -n "$PRE_SCRIPT" && -f "$PRE_SCRIPT" ]]; then
    blue "🔧 Running pre-update script: $PRE_SCRIPT"
    if bash "$PRE_SCRIPT"; then
        green "✅ Pre-update script completed successfully"
        log_message "Pre-update script completed: $PRE_SCRIPT"
    else
        red "❌ Pre-update script failed"
        log_message "Pre-update script failed: $PRE_SCRIPT"
        exit 1
    fi
fi

# Detect OS and prepare package manager
blue "🔍 Detecting operating system..."

# Function to set download rate limit
set_download_limit() {{
    local rate="$1"
    if [[ -n "$rate" ]]; then
        if command -v trickle >/dev/null 2>&1; then
            echo "trickle -d $rate"
        else
            yellow "⚠️  trickle not available for bandwidth limiting"
            echo ""
        fi
    else
        echo ""
    fi
}}

RATE_LIMITER=$(set_download_limit "$MAX_DOWNLOAD_RATE")

if command -v apt >/dev/null 2>&1; then
    green "📦 Detected Debian/Ubuntu system"
    export DEBIAN_FRONTEND=noninteractive
    
    # Configure APT for better performance and reliability
    APT_CONFIG="/etc/apt/apt.conf.d/99krust-update"
    cat > "$APT_CONFIG" << 'APT_EOF'
APT::Get::Show-Progress "true";
APT::Get::Show-Progress-Size "true";
Dpkg::Progress-Fancy "true";
APT::Color "true";
APT_EOF
    
    blue "🔄 Updating package lists..."
    if [[ "$DRY_RUN" == "true" ]]; then
        yellow "🔍 DRY RUN: Would update package lists"
    else
        $RATE_LIMITER apt update -qq || {{
            red "❌ Failed to update package lists"
            exit 1
        }}
    fi
    
    # Show available updates
    blue "📊 Checking for available updates..."
    UPDATES_AVAILABLE=$(apt list --upgradable 2>/dev/null | wc -l)
    if [[ $UPDATES_AVAILABLE -gt 1 ]]; then
        blue "📈 $((UPDATES_AVAILABLE - 1)) updates available"
        
        if [[ "$DRY_RUN" == "true" ]]; then
            yellow "🔍 DRY RUN: Available updates:"
            apt list --upgradable 2>/dev/null | tail -n +2 | head -20
            if [[ $UPDATES_AVAILABLE -gt 21 ]]; then
                echo "... and $((UPDATES_AVAILABLE - 21)) more"
            fi
        fi
    else
        green "✅ System is already up to date"
        log_message "System already up to date"
        rm -f "$APT_CONFIG"
        exit 0
    fi
    
    if [[ "$DRY_RUN" != "true" ]]; then
        case "$UPDATE_TYPE" in
            "security")
                yellow "🛡️  Installing security updates only..."
                log_message "Starting security updates"
                $RATE_LIMITER apt upgrade -y --with-new-pkgs -o Dpkg::Options::="--force-confdef" -o Dpkg::Options::="--force-confold" || {{
                    red "❌ Security updates failed"
                    exit 1
                }}
                ;;
            "kernel")
                yellow "🔧 Installing kernel and core system updates..."
                log_message "Starting kernel updates"
                $RATE_LIMITER apt upgrade -y linux-* systemd* libc6* --with-new-pkgs -o Dpkg::Options::="--force-confdef" -o Dpkg::Options::="--force-confold" || {{
                    red "❌ Kernel updates failed"
                    exit 1
                }}
                ;;
            "minimal")
                yellow "⚡ Installing essential updates only..."
                log_message "Starting minimal updates"
                $RATE_LIMITER apt upgrade -y --with-new-pkgs -o APT::Get::Upgrade-Allow-New=false -o Dpkg::Options::="--force-confdef" -o Dpkg::Options::="--force-confold" || {{
                    red "❌ Minimal updates failed"
                    exit 1
                }}
                ;;
            *)
                blue "⬆️  Upgrading all packages..."
                log_message "Starting full system update"
                $RATE_LIMITER apt upgrade -y --with-new-pkgs -o Dpkg::Options::="--force-confdef" -o Dpkg::Options::="--force-confold" || {{
                    red "❌ Package upgrade failed"
                    exit 1
                }}
                
                blue "🧹 Performing additional maintenance..."
                apt autoremove -y >/dev/null 2>&1 || true
                apt autoclean >/dev/null 2>&1 || true
                ;;
        esac
    fi
    
    # Clean up APT config
    rm -f "$APT_CONFIG"
    
elif command -v dnf >/dev/null 2>&1; then
    green "📦 Detected Fedora/RHEL system (dnf)"
    
    blue "📊 Checking for available updates..."
    if [[ "$DRY_RUN" == "true" ]]; then
        yellow "🔍 DRY RUN: Available updates:"
        dnf check-update 2>/dev/null | grep -v "^$" | tail -n +2 || true
    else
        case "$UPDATE_TYPE" in
            "security")
                yellow "🛡️  Installing security updates only..."
                log_message "Starting security updates"
                $RATE_LIMITER dnf upgrade -y --security || {{
                    red "❌ Security updates failed"
                    exit 1
                }}
                ;;
            "kernel")
                yellow "🔧 Installing kernel and core system updates..."
                log_message "Starting kernel updates"
                $RATE_LIMITER dnf upgrade -y kernel* systemd* glibc* || {{
                    red "❌ Kernel updates failed"
                    exit 1
                }}
                ;;
            "minimal")
                yellow "⚡ Installing essential updates only..."
                log_message "Starting minimal updates"
                $RATE_LIMITER dnf upgrade -y --bugfix || {{
                    red "❌ Minimal updates failed"
                    exit 1
                }}
                ;;
            *)
                blue "⬆️  Upgrading all packages..."
                log_message "Starting full system update"
                $RATE_LIMITER dnf upgrade -y || {{
                    red "❌ Package upgrade failed"
                    exit 1
                }}
                
                blue "🧹 Performing cleanup..."
                dnf autoremove -y >/dev/null 2>&1 || true
                ;;
        esac
    fi
    
elif command -v yum >/dev/null 2>&1; then
    green "📦 Detected RHEL/CentOS system (yum)"
    
    blue "📊 Checking for available updates..."
    if [[ "$DRY_RUN" == "true" ]]; then
        yellow "🔍 DRY RUN: Available updates:"
        yum check-update 2>/dev/null | grep -v "^$" || true
    else
        case "$UPDATE_TYPE" in
            "security")
                yellow "🛡️  Installing security updates only..."
                log_message "Starting security updates"
                $RATE_LIMITER yum update -y --security || {{
                    red "❌ Security updates failed"
                    exit 1
                }}
                ;;
            "kernel")
                yellow "🔧 Installing kernel and core system updates..."
                log_message "Starting kernel updates"
                $RATE_LIMITER yum update -y kernel* systemd* glibc* || {{
                    red "❌ Kernel updates failed"
                    exit 1
                }}
                ;;
            *)
                blue "⬆️  Upgrading all packages..."
                log_message "Starting full system update"
                $RATE_LIMITER yum update -y || {{
                    red "❌ Package upgrade failed"
                    exit 1
                }}
                
                blue "🧹 Performing cleanup..."
                yum autoremove -y >/dev/null 2>&1 || true
                ;;
        esac
    fi
    
elif command -v pacman >/dev/null 2>&1; then
    green "📦 Detected Arch Linux system"
    
    blue "📊 Checking for available updates..."
    if [[ "$DRY_RUN" == "true" ]]; then
        yellow "🔍 DRY RUN: Available updates:"
        pacman -Qu 2>/dev/null || echo "No updates available"
    else
        blue "⬆️  Upgrading all packages..."
        log_message "Starting full system update"
        $RATE_LIMITER pacman -Syu --noconfirm || {{
            red "❌ Package upgrade failed"
            exit 1
        }}
        
        blue "🧹 Performing cleanup..."
        pacman -Rns $(pacman -Qtdq) --noconfirm 2>/dev/null || true
    fi
    
elif command -v zypper >/dev/null 2>&1; then
    green "📦 Detected SUSE system"
    
    blue "📊 Checking for available updates..."
    if [[ "$DRY_RUN" == "true" ]]; then
        yellow "🔍 DRY RUN: Available updates:"
        zypper list-updates 2>/dev/null || true
    else
        case "$UPDATE_TYPE" in
            "security")
                yellow "🛡️  Installing security updates only..."
                log_message "Starting security updates"
                $RATE_LIMITER zypper patch -y --category security || {{
                    red "❌ Security updates failed"
                    exit 1
                }}
                ;;
            *)
                blue "⬆️  Upgrading all packages..."
                log_message "Starting full system update"
                zypper refresh >/dev/null 2>&1 || true
                $RATE_LIMITER zypper update -y || {{
                    red "❌ Package upgrade failed"
                    exit 1
                }}
                ;;
        esac
    fi
    
else
    red "❌ No supported package manager found!"
    log_message "No supported package manager found"
    exit 1
fi

# Run post-update script
if [[ -n "$POST_SCRIPT" && -f "$POST_SCRIPT" ]]; then
    blue "🔧 Running post-update script: $POST_SCRIPT"
    if bash "$POST_SCRIPT"; then
        green "✅ Post-update script completed successfully"
        log_message "Post-update script completed: $POST_SCRIPT"
    else
        yellow "⚠️  Post-update script failed, continuing..."
        log_message "Post-update script failed: $POST_SCRIPT"
    fi
fi

if [[ "$DRY_RUN" == "true" ]]; then
    green "🔍 DRY RUN COMPLETED - No actual changes were made"
    exit 0
fi

# Post-update checks
blue "🔍 Post-update verification..."

# Check if any services need restarting
if command -v needrestart >/dev/null 2>&1; then
    blue "🔄 Checking for services that need restarting..."
    needrestart -b 2>/dev/null | grep "NEEDRESTART-SVC:" | while read -r line; do
        service=$(echo "$line" | cut -d: -f2)
        yellow "⚠️  Service needs restart: $service"
    done
fi

# Check if reboot is required
REBOOT_REQUIRED=false
REBOOT_REASONS=()

if [[ -f /var/run/reboot-required ]]; then
    REBOOT_REQUIRED=true
    REBOOT_REASONS+=("reboot-required file exists")
fi

# Check for kernel updates
CURRENT_KERNEL=$(uname -r)
if command -v dpkg >/dev/null 2>&1; then
    INSTALLED_KERNEL=$(dpkg -l | grep linux-image | grep "^ii" | awk '{{print $2}}' | sed 's/linux-image-//' | sort -V | tail -1)
elif command -v rpm >/dev/null 2>&1; then
    INSTALLED_KERNEL=$(rpm -qa kernel | sed 's/kernel-//' | sort -V | tail -1)
else
    INSTALLED_KERNEL=$(ls /lib/modules/ 2>/dev/null | sort -V | tail -1)
fi

if [[ -n "$INSTALLED_KERNEL" && "$CURRENT_KERNEL" != "$INSTALLED_KERNEL" ]]; then
    REBOOT_REQUIRED=true
    REBOOT_REASONS+=("kernel update: $CURRENT_KERNEL -> $INSTALLED_KERNEL")
fi

# Final status and reboot handling
if [[ "$REBOOT_REQUIRED" == "true" ]]; then
    yellow "⚠️  System reboot required!"
    printf "   Reasons: %s\n" "${{REBOOT_REASONS[@]}}"
    
    if [[ "$AUTO_REBOOT" == "true" ]]; then
        yellow "🔄 Auto-reboot enabled. Rebooting in 10 seconds..."
        log_message "Auto-reboot triggered after update"
        
        # Brief countdown
        for i in $(seq 10 -1 1); do
            echo -ne "\r⏰ Rebooting in $i seconds..."
            sleep 1
        done
        echo ""
        
        green "🚀 Initiating reboot..."
        sync
        reboot
    else
        yellow "🔄 Please reboot the system manually to complete the update process"
        log_message "Manual reboot required after update"
    fi
else
    green "✅ Updates completed successfully. No reboot required."
    log_message "Updates completed successfully, no reboot required"
fi

log_message "KRUST OS update completed on $HOSTNAME"
green "🎉 OS update process completed!"

# Show update summary
blue "📊 Update Summary:"
echo "  Hostname: $HOSTNAME"
echo "  Update Type: $UPDATE_TYPE"
echo "  Timestamp: $(date)"
echo "  Reboot Required: $REBOOT_REQUIRED"
echo "  Log File: $UPDATE_LOG"
"#,
        security_only = security_only,
        auto_reboot = auto_reboot,
        create_snapshot = create_snapshot,
        dry_run = dry_run,
        update_type = update_type,
        max_rate = max_download_rate.unwrap_or_default(),
        backup_configs = backup_configs,
        exclude_clause = exclude_clause,
        include_clause = include_clause,
        pre_script = pre_update_script.unwrap_or_default(),
        post_script = post_update_script.unwrap_or_default()
    );

    Ok(format!("bash -c {}", shell_escape::unix::escape(script.into())))
}
