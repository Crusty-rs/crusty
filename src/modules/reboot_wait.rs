use anyhow::{Result, bail};

pub fn build_command(args: &[String]) -> Result<String> {
    if !args.is_empty() {
        bail!("reboot-wait module does not take any arguments.");
    }

    let script = r#"
set -e
echo "Rebooting system..."
reboot &
sleep 5

# Wait for system to reboot and become available again
# This is a placeholder; real implementation should try SSH reconnect logic
echo "Waiting for reboot to complete... (this is a stub)"
"#;

    Ok(format!("bash -c {}", shell_escape::unix::escape(script.into())))
}

