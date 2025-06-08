// [modules/sudo.rs] - Simplified sudo module
use anyhow::{bail, Result};

pub fn build_command(args: &[String]) -> Result<String> {
    if args.is_empty() {
        bail!("Usage: sudo <user> [--nopass]");
    }
    
    let user = &args[0];
    let nopass = args.contains(&"--nopass".to_string());
    
    // Validate username (basic check)
    if user.is_empty() || user.len() > 32 {
        bail!("Invalid username length: {}", user);
    }
    
    if !user.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '-') {
        bail!("Invalid username characters: {}", user);
    }
    
    let rule = if nopass {
        format!("{} ALL=(ALL) NOPASSWD: ALL", user)
    } else {
        format!("{} ALL=(ALL) ALL", user)
    };
    
    // Simple shell script that validates and installs the sudo rule
    let script = format!(r#"#!/bin/bash
set -e

USER='{}'
RULE='{}'
SUDOERS_FILE="/etc/sudoers.d/$USER"

# Check if user exists
if ! id "$USER" >/dev/null 2>&1; then
    echo "Warning: User '$USER' does not exist" >&2
fi

# Create temp file for validation
TMP_FILE=$(mktemp)
echo "$RULE" > "$TMP_FILE"
chmod 0440 "$TMP_FILE"

# Validate syntax
if ! visudo -q -c -f "$TMP_FILE" 2>/dev/null; then
    rm -f "$TMP_FILE"
    echo "Error: Invalid sudoers syntax" >&2
    exit 1
fi

# Install the rule
mv "$TMP_FILE" "$SUDOERS_FILE"
chmod 0440 "$SUDOERS_FILE"
chown root:root "$SUDOERS_FILE"

echo "Sudo access granted to $USER"
"#, user, rule);
    
    Ok(script)
}
