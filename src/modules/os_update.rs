// [modules/os_update.rs] - Simplified OS update module
use anyhow::Result;

pub fn build_command(args: &[String]) -> Result<String> {
    let security_only = args.contains(&"--security-only".to_string());
    let auto_reboot = args.contains(&"--auto-reboot".to_string());
    let dry_run = args.contains(&"--dry-run".to_string());
    
    let script = format!(r#"#!/bin/bash
set -e

SECURITY_ONLY={}
AUTO_REBOOT={}
DRY_RUN={}

echo "Starting OS update (security_only=$SECURITY_ONLY, auto_reboot=$AUTO_REBOOT)"

# Check disk space
DISK_USAGE=$(df / | tail -1 | awk '{{print $5}}' | sed 's/%//')
if [[ $DISK_USAGE -gt 90 ]]; then
    echo "Error: Disk usage is $DISK_USAGE% - aborting" >&2
    exit 1
fi

# Detect package manager and update
if command -v apt-get >/dev/null 2>&1; then
    echo "Detected: Debian/Ubuntu"
    export DEBIAN_FRONTEND=noninteractive
    
    if [[ "$DRY_RUN" == "true" ]]; then
        apt-get update -qq
        apt-get upgrade -s
    else
        apt-get update -qq || exit 1
        
        if [[ "$SECURITY_ONLY" == "true" ]]; then
            apt-get upgrade -y --with-new-pkgs \
                -o Dpkg::Options::="--force-confdef" \
                -o Dpkg::Options::="--force-confold" || exit 1
        else
            apt-get upgrade -y \
                -o Dpkg::Options::="--force-confdef" \
                -o Dpkg::Options::="--force-confold" || exit 1
        fi
        
        apt-get autoremove -y || true
    fi
    
    # Check if reboot required
    if [[ -f /var/run/reboot-required && "$AUTO_REBOOT" == "true" ]]; then
        echo "Reboot required - scheduling in 1 minute"
        shutdown -r +1 "System updated - rebooting"
    fi
    
elif command -v yum >/dev/null 2>&1; then
    echo "Detected: RHEL/CentOS"
    
    if [[ "$DRY_RUN" == "true" ]]; then
        yum check-update || true
    else
        if [[ "$SECURITY_ONLY" == "true" ]]; then
            yum update -y --security || exit 1
        else
            yum update -y || exit 1
        fi
        
        yum autoremove -y || true
    fi
    
    # Check if reboot required
    if [[ "$AUTO_REBOOT" == "true" ]]; then
        if needs-restarting -r >/dev/null 2>&1; then
            : # No reboot needed
        else
            echo "Reboot required - scheduling in 1 minute"
            shutdown -r +1 "System updated - rebooting"
        fi
    fi
    
elif command -v dnf >/dev/null 2>&1; then
    echo "Detected: Fedora"
    
    if [[ "$DRY_RUN" == "true" ]]; then
        dnf check-update || true
    else
        if [[ "$SECURITY_ONLY" == "true" ]]; then
            dnf upgrade -y --security || exit 1
        else
            dnf upgrade -y || exit 1
        fi
        
        dnf autoremove -y || true
    fi
    
    # Check if reboot required
    if [[ "$AUTO_REBOOT" == "true" ]]; then
        if dnf needs-restarting -r >/dev/null 2>&1; then
            : # No reboot needed
        else
            echo "Reboot required - scheduling in 1 minute"
            shutdown -r +1 "System updated - rebooting"
        fi
    fi
    
else
    echo "Error: No supported package manager found" >&2
    exit 1
fi

echo "Update completed successfully"
"#, 
        security_only, 
        auto_reboot,
        dry_run
    );
    
    Ok(script)
}
