// [modules/reboot_wait.rs] - Simplified reboot module
use anyhow::Result;

pub fn build_command(args: &[String]) -> Result<String> {
    let mut delay = 5;
    let mut check_only = false;
    let mut force = false;
    
    // Parse args
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--delay" => {
                if let Some(val) = args.get(i + 1) {
                    delay = val.parse().unwrap_or(5);
                    i += 2;
                } else {
                    i += 1;
                }
            }
            "--check" => {
                check_only = true;
                i += 1;
            }
            "--force" => {
                force = true;
                i += 1;
            }
            _ => i += 1,
        }
    }
    
    let script = if check_only {
        // Check mode - just verify if reboot is needed
        r#"#!/bin/bash
REBOOT_NEEDED=false

# Check various indicators
if [[ -f /var/run/reboot-required ]]; then
    echo "Reboot required: /var/run/reboot-required exists"
    REBOOT_NEEDED=true
fi

# Check kernel version mismatch
if command -v uname >/dev/null 2>&1; then
    CURRENT_KERNEL=$(uname -r)
    
    if command -v dpkg >/dev/null 2>&1; then
        LATEST_KERNEL=$(dpkg -l 'linux-image-*' | grep '^ii' | awk '{print $2}' | sed 's/linux-image-//' | sort -V | tail -1)
    elif command -v rpm >/dev/null 2>&1; then
        LATEST_KERNEL=$(rpm -qa kernel | sed 's/kernel-//' | sort -V | tail -1)
    fi
    
    if [[ -n "$LATEST_KERNEL" && "$CURRENT_KERNEL" != "$LATEST_KERNEL" ]]; then
        echo "Reboot required: kernel update ($CURRENT_KERNEL -> $LATEST_KERNEL)"
        REBOOT_NEEDED=true
    fi
fi

if [[ "$REBOOT_NEEDED" == "true" ]]; then
    exit 1
else
    echo "No reboot required"
    exit 0
fi
"#.to_string()
    } else {
        // Reboot mode
        format!(r#"#!/bin/bash
set -e

DELAY={}
FORCE={}

echo "Checking if reboot is required..."

REBOOT_NEEDED=false

# Check indicators
if [[ -f /var/run/reboot-required ]]; then
    REBOOT_NEEDED=true
fi

# Check kernel
CURRENT_KERNEL=$(uname -r)
if command -v dpkg >/dev/null 2>&1; then
    LATEST_KERNEL=$(dpkg -l 'linux-image-*' | grep '^ii' | awk '{{print $2}}' | sed 's/linux-image-//' | sort -V | tail -1)
elif command -v rpm >/dev/null 2>&1; then
    LATEST_KERNEL=$(rpm -qa kernel | sed 's/kernel-//' | sort -V | tail -1)
fi

if [[ -n "$LATEST_KERNEL" && "$CURRENT_KERNEL" != "$LATEST_KERNEL" ]]; then
    REBOOT_NEEDED=true
fi

# Decide whether to reboot
if [[ "$REBOOT_NEEDED" == "true" || "$FORCE" == "true" ]]; then
    echo "Reboot will occur in $DELAY seconds..."
    
    # Show active users
    USERS=$(who | wc -l)
    if [[ $USERS -gt 0 ]]; then
        echo "Warning: $USERS users currently logged in"
        who
    fi
    
    # Sync filesystems
    sync
    
    # Countdown
    for i in $(seq $DELAY -1 1); do
        echo -ne "\rRebooting in $i seconds... "
        sleep 1
    done
    echo ""
    
    # Reboot
    echo "Initiating reboot"
    shutdown -r now
else
    echo "No reboot required"
fi
"#, delay, force)
    };
    
    Ok(script)
}
