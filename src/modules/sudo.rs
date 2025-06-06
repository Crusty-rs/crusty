use anyhow::{bail, Result};
use regex::Regex;

fn is_valid_username(username: &str) -> bool {
    if username.is_empty() || username.len() > 32 {
        return false;
    }
    let re = Regex::new(r"^[A-Za-z_][A-Za-z0-9_-]*$").unwrap();
    re.is_match(username)
}

fn is_valid_at_time(time_str: &str) -> bool {
    if time_str.is_empty() || time_str.contains(['\\', '`', '$', ';', '|']) {
        return false;
    }
    !time_str.trim().is_empty()
}

pub fn build_command(args: &[String]) -> Result<String> {
    if args.is_empty() {
        bail!("Usage: sudo <user> [--nopass] [--expire=TIME]");
    }

    let user = &args[0];
    if !is_valid_username(user) {
        bail!("Invalid username: '{}'", user);
    }

    let mut nopass = false;
    let mut expire_at_str: Option<String> = None;

    for arg in &args[1..] {
        if arg == "--nopass" {
            nopass = true;
        } else if arg.starts_with("--expire=") {
            let parts: Vec<&str> = arg.splitn(2, '=').collect();
            if parts.len() == 2 && !parts[1].is_empty() {
                let time_val = parts[1].to_string();
                if !is_valid_at_time(&time_val) {
                    bail!("Invalid --expire time format: '{}'", time_val);
                }
                expire_at_str = Some(time_val);
            } else {
                bail!("Invalid format for --expire. Expected --expire=<time_spec>");
            }
        } else {
            bail!("Unknown argument for sudo module: '{}'", arg);
        }
    }

    let sudo_privileges = "ALL=(ALL)";
    let sudo_rule = format!(
        "{} {} {}ALL",
        user,
        sudo_privileges,
        if nopass { "NOPASSWD: " } else { "" }
    );

    let sudoers_file = format!("/etc/sudoers.d/{}", user);
    let mut script_lines: Vec<String> = Vec::new();

    script_lines.push("set -e".to_string());
    script_lines.push(format!("SUDOERS_FILE=\"{}\"", sudoers_file));
    let sudo_rule_escaped = sudo_rule.replace('\'', "'\\''");
    script_lines.push(format!("SUDO_RULE='{}'", sudo_rule_escaped));
    script_lines.push("TMP_FILE=$(mktemp)".to_string());
    script_lines.push("trap 'rm -f \"$TMP_FILE\"' EXIT".to_string());
    script_lines.push("echo \"$SUDO_RULE\" > \"$TMP_FILE\"".to_string());
    script_lines.push("chmod 0440 \"$TMP_FILE\"".to_string());
    script_lines.push("visudo -q -c -f \"$TMP_FILE\"".to_string());
    script_lines.push("mv \"$TMP_FILE\" \"$SUDOERS_FILE\"".to_string());
    script_lines.push(format!("echo \"Sudo rule for {} configured in $SUDOERS_FILE.\";", user));

    if let Some(expire_time) = expire_at_str {
        let at_time_escaped = expire_time.replace('\'', "'\\''");
        let remove_cmd = format!("rm -f {}", sudoers_file);
        let remove_cmd_escaped = remove_cmd.replace('\'', "'\\''");
        script_lines.push(format!("echo '{}' | at '{}'", remove_cmd_escaped, at_time_escaped));
        script_lines.push(format!("echo \"Sudo rule for {} will be removed at {}\";", user, expire_time));
    }

    let script = script_lines.join(" && ");
    let final_cmd = format!("bash -c '{}'", script.replace('\'', "'\\''"));
    Ok(final_cmd)
}

