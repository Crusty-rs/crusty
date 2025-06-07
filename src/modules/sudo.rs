
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
        bail!("Usage: sudo <user> [OPTIONS]");
    }

    let user = &args[0];
    if !is_valid_username(user) {
        bail!("Invalid username: '{}'", user);
    }

    let mut nopass = false;
    let mut expire_at_str: Option<String> = None;
    let mut allowed_commands = "ALL".to_string();
    let mut template = "standard".to_string();
    let mut remove_access = false;
    let mut list_access = false;

    // Parse arguments
    for i in 1..args.len() {
        let arg = &args[i];
        
        if arg == "--nopass" {
            nopass = true;
        } else if arg.starts_with("--expire=") {
            let time_val = arg.strip_prefix("--expire=").unwrap().to_string();
            if !is_valid_at_time(&time_val) {
                bail!("Invalid --expire time format: '{}'", time_val);
            }
            expire_at_str = Some(time_val);
        } else if arg.starts_with("--commands=") {
            allowed_commands = arg.strip_prefix("--commands=").unwrap().to_string();
        } else if arg.starts_with("--template=") {
            template = arg.strip_prefix("--template=").unwrap().to_string();
        } else if arg == "--remove" {
            remove_access = true;
        } else if arg == "--list" {
            list_access = true;
        } else if arg == "--help" {
            println!("KRUST sudo module
Usage: krust sudo <user> [OPTIONS]

Options:
    --nopass              Allow passwordless sudo
    --commands=CMDS       Specific commands (comma-separated) or ALL
    --expire=TIME         Expire sudo access at specific time
    --template=TYPE       Use predefined template
    --remove              Remove sudo access for user
    --list                List current sudo privileges for user
    --help               Show this help

Templates:
    standard    - Full sudo access (ALL commands)
    developer   - Dev tools: apt/yum/dnf, systemctl, docker, git, npm, pip, cargo
    operator    - System ops: service control, log viewing, monitoring tools
    readonly    - Read-only: system info, log viewing, process monitoring
    webadmin    - Web server: nginx/apache control, log access, cert management
    dbadmin     - Database: mysql/postgres service control, backup tools

Examples:
    krust --hosts servers sudo alice --nopass
    krust --hosts web sudo bob --template=webadmin --expire='tomorrow 9am'
    krust --hosts db sudo carol --template=dbadmin
    krust --hosts all sudo dave --commands='/bin/systemctl restart nginx,/usr/bin/tail /var/log/*'
    krust --hosts server1 sudo alice --list
    krust --hosts server1 sudo olduser --remove");
            return Ok("echo 'Help displayed'".to_string());
        } else {
            bail!("Unknown argument: {}. Use --help for usage.", arg);
        }
    }

    // Handle list operation
    if list_access {
        let script = format!(r#"
#!/bin/bash
set -e

green() {{ echo -e "\033[32m$1\033[0m"; }}
blue() {{ echo -e "\033[34m$1\033[0m"; }}
yellow() {{ echo -e "\033[33m$1\033[0m"; }}

USER="{user}"

green "🔍 KRUST Sudo Access Check"
blue "👤 User: $USER"

echo ""
blue "📄 Current sudo privileges:"

if sudo -l -U "$USER" >/dev/null 2>&1; then
    sudo -l -U "$USER" 2>/dev/null | grep -A 20 "may run the following commands" || echo "No specific rules found"
else
    yellow "⚠️  User $USER has no sudo privileges or user doesn't exist"
fi

echo ""
blue "📁 Sudoers files containing '$USER':"
grep -l "$USER" /etc/sudoers.d/* 2>/dev/null || echo "No sudoers files found for $USER"

if [[ -f "/etc/sudoers.d/$USER" ]]; then
    echo ""
    blue "📋 Content of /etc/sudoers.d/$USER:"
    cat "/etc/sudoers.d/$USER"
fi
"#, user = user);

        return Ok(format!("bash -c {}", shell_escape::unix::escape(script.into())));
    }

    // Handle remove operation
    if remove_access {
        let script = format!(r#"
#!/bin/bash
set -e

green() {{ echo -e "\033[32m$1\033[0m"; }}
yellow() {{ echo -e "\033[33m$1\033[0m"; }}
blue() {{ echo -e "\033[34m$1\033[0m"; }}
red() {{ echo -e "\033[31m$1\033[0m"; }}

USER="{user}"
SUDOERS_FILE="/etc/sudoers.d/$USER"

green "🗑️  KRUST Sudo Access Removal"
blue "👤 User: $USER"

if [[ -f "$SUDOERS_FILE" ]]; then
    yellow "🔍 Found sudo configuration for $USER"
    echo "Content to be removed:"
    cat "$SUDOERS_FILE"
    echo ""
    
    rm -f "$SUDOERS_FILE"
    green "✅ Sudo access removed for $USER"
    
    # Also remove any at jobs for this user's sudo cleanup
    at -l 2>/dev/null | grep "rm -f $SUDOERS_FILE" | awk '{{print $1}}' | xargs -r atrm 2>/dev/null || true
    
else
    yellow "⚠️  No sudo configuration found for $USER"
fi

# Check if user still has sudo access through other means
if sudo -l -U "$USER" >/dev/null 2>&1; then
    yellow "⚠️  User $USER may still have sudo access through other configurations"
    sudo -l -U "$USER" 2>/dev/null | head -5
else
    green "✅ User $USER has no sudo access"
fi
"#, user = user);

        return Ok(format!("bash -c {}", shell_escape::unix::escape(script.into())));
    }

    // Apply template to determine allowed commands
    let template_commands = match template.as_str() {
        "developer" => {
            "/usr/bin/apt, /usr/bin/apt-get, /usr/bin/yum, /usr/bin/dnf, /usr/bin/pacman, \
             /bin/systemctl, /usr/bin/systemctl, /usr/bin/docker, /usr/bin/git, \
             /usr/bin/npm, /usr/bin/yarn, /usr/bin/pip*, /usr/bin/cargo, \
             /usr/bin/make, /usr/bin/cmake, /usr/bin/gcc, /usr/bin/g++, \
             /usr/local/bin/*, /opt/*/bin/*"
        },
        "operator" => {
            "/bin/systemctl status *, /bin/systemctl start *, /bin/systemctl stop *, \
             /bin/systemctl restart *, /bin/systemctl reload *, /usr/bin/systemctl status *, \
             /usr/bin/systemctl start *, /usr/bin/systemctl stop *, /usr/bin/systemctl restart *, \
             /usr/bin/systemctl reload *, /usr/bin/tail, /usr/bin/less, /usr/bin/grep, \
             /usr/bin/ps, /usr/bin/top, /usr/bin/htop, /usr/bin/netstat, /usr/bin/ss, \
             /usr/bin/iotop, /usr/bin/iostat, /usr/bin/vmstat, /bin/cat /var/log/*, \
             /usr/bin/journalctl, /usr/bin/dmesg"
        },
        "readonly" => {
            "/usr/bin/ps, /usr/bin/top, /usr/bin/htop, /usr/bin/df, /usr/bin/free, \
             /usr/bin/uptime, /usr/bin/who, /usr/bin/w, /usr/bin/last, /usr/bin/lastlog, \
             /bin/cat /var/log/*, /usr/bin/tail /var/log/*, /usr/bin/less /var/log/*, \
             /usr/bin/grep, /usr/bin/journalctl -f, /usr/bin/journalctl -u *, \
             /usr/bin/dmesg, /usr/bin/lscpu, /usr/bin/lsmem, /usr/bin/lsblk, \
             /usr/bin/lsof, /usr/bin/netstat, /usr/bin/ss"
        },
        "webadmin" => {
            "/bin/systemctl status nginx, /bin/systemctl start nginx, /bin/systemctl stop nginx, \
             /bin/systemctl restart nginx, /bin/systemctl reload nginx, \
             /bin/systemctl status apache2, /bin/systemctl start apache2, /bin/systemctl stop apache2, \
             /bin/systemctl restart apache2, /bin/systemctl reload apache2, \
             /bin/systemctl status httpd, /bin/systemctl start httpd, /bin/systemctl stop httpd, \
             /bin/systemctl restart httpd, /bin/systemctl reload httpd, \
             /usr/bin/nginx -t, /usr/sbin/nginx -t, /usr/bin/apache2ctl configtest, \
             /bin/cat /var/log/nginx/*, /bin/cat /var/log/apache2/*, /bin/cat /var/log/httpd/*, \
             /usr/bin/tail /var/log/nginx/*, /usr/bin/tail /var/log/apache2/*, /usr/bin/tail /var/log/httpd/*, \
             /usr/bin/certbot, /usr/bin/openssl"
        },
        "dbadmin" => {
            "/bin/systemctl status mysql, /bin/systemctl start mysql, /bin/systemctl stop mysql, \
             /bin/systemctl restart mysql, /bin/systemctl reload mysql, \
             /bin/systemctl status postgresql, /bin/systemctl start postgresql, /bin/systemctl stop postgresql, \
             /bin/systemctl restart postgresql, /bin/systemctl reload postgresql, \
             /bin/systemctl status mariadb, /bin/systemctl start mariadb, /bin/systemctl stop mariadb, \
             /bin/systemctl restart mariadb, /bin/systemctl reload mariadb, \
             /usr/bin/mysqldump, /usr/bin/pg_dump, /usr/bin/pg_restore, \
             /bin/cat /var/log/mysql/*, /bin/cat /var/log/postgresql/*, \
             /usr/bin/tail /var/log/mysql/*, /usr/bin/tail /var/log/postgresql/*"
        },
        "standard" | _ => "ALL",
    };

    // Use template commands if no specific commands provided
    if allowed_commands == "ALL" && template != "standard" {
        allowed_commands = template_commands.to_string();
    }

    let sudo_rule = format!(
        "{} ALL=({}) {}{}",
        user,
        "ALL",
        if nopass { "NOPASSWD: " } else { "" },
        allowed_commands
    );

    let script = format!(r#"
#!/bin/bash
set -e

# Color functions  
green() {{ echo -e "\033[32m$1\033[0m"; }}
yellow() {{ echo -e "\033[33m$1\033[0m"; }}
blue() {{ echo -e "\033[34m$1\033[0m"; }}
red() {{ echo -e "\033[31m$1\033[0m"; }}

USER="{user}"
SUDOERS_FILE="/etc/sudoers.d/$USER"
SUDO_RULE='{sudo_rule}'
EXPIRE_TIME="{expire_time}"
TEMPLATE="{template}"
NOPASS={nopass}

green "🔐 KRUST Sudo Configuration"
blue "👤 User: $USER"
blue "📋 Template: $TEMPLATE"
blue "🔑 Passwordless: $NOPASS"
if [[ -n "$EXPIRE_TIME" ]]; then
    blue "⏰ Expires: $EXPIRE_TIME"
fi

echo ""
blue "📝 Sudo rule to be applied:"
echo "   $SUDO_RULE"
echo ""

# Check if user exists
if ! id "$USER" >/dev/null 2>&1; then
    yellow "⚠️  Warning: User '$USER' does not exist on this system"
    yellow "   Sudo rule will be created but won't work until user is created"
fi

# Backup existing sudo config if it exists
if [[ -f "$SUDOERS_FILE" ]]; then
    yellow "📋 Existing sudo configuration found for $USER:"
    cat "$SUDOERS_FILE"
    cp "$SUDOERS_FILE" "$SUDOERS_FILE.backup.$(date +%Y%m%d-%H%M%S)"
    yellow "🔄 Backup created: $SUDOERS_FILE.backup.$(date +%Y%m%d-%H%M%S)"
    echo ""
fi

# Create temporary file for validation
TMP_FILE=$(mktemp)
trap 'rm -f "$TMP_FILE"' EXIT

# Write rule to temp file
echo "$SUDO_RULE" > "$TMP_FILE"
chmod 0440 "$TMP_FILE"

# Validate syntax
blue "✅ Validating sudoers syntax..."
if ! visudo -q -c -f "$TMP_FILE"; then
    red "❌ Invalid sudoers syntax!"
    echo "Rule: $SUDO_RULE"
    exit 1
fi

green "✅ Syntax validation passed"

# Apply the rule
blue "📝 Installing sudo rule..."
mv "$TMP_FILE" "$SUDOERS_FILE"

# Set correct permissions
chmod 0440 "$SUDOERS_FILE"
chown root:root "$SUDOERS_FILE"

green "✅ Sudo rule installed successfully"

# Set expiration if requested
if [[ -n "$EXPIRE_TIME" ]]; then
    blue "⏰ Setting up automatic expiration..."
    
    # Create cleanup script
    CLEANUP_SCRIPT="/tmp/sudo_cleanup_$USER.sh"
    cat > "$CLEANUP_SCRIPT" << 'CLEANUP_EOF'
#!/bin/bash
rm -f "$SUDOERS_FILE"
logger "KRUST: Sudo access expired and removed for user $USER"
echo "$(date): Sudo access expired for $USER" >> /var/log/krust-sudo.log
CLEANUP_EOF
    
    # Make it executable
    chmod +x "$CLEANUP_SCRIPT"
    
    # Schedule with at command
    if command -v at >/dev/null 2>&1; then
        echo "$CLEANUP_SCRIPT" | at "$EXPIRE_TIME" 2>/dev/null && {{
            green "✅ Automatic cleanup scheduled for $EXPIRE_TIME"
        }} || {{
            yellow "⚠️  Failed to schedule automatic cleanup with 'at' command"
            yellow "   Please remove sudo access manually at $EXPIRE_TIME"
        }}
    else
        yellow "⚠️  'at' command not available for automatic expiration"
        yellow "   Please remove sudo access manually at $EXPIRE_TIME"
        yellow "   Command: rm -f $SUDOERS_FILE"
    fi
fi

echo ""
green "🎉 Sudo configuration completed successfully!"

# Show current sudo rules for user
blue "📊 Verifying sudo access..."
if sudo -l -U "$USER" >/dev/null 2>&1; then
    green "✅ Sudo access verified for $USER"
    echo ""
    blue "📄 Current sudo privileges:"
    sudo -l -U "$USER" 2>/dev/null | grep -A 10 "may run the following commands" || true
else
    red "❌ Failed to verify sudo access"
fi

# Log the action
echo "$(date): KRUST sudo access granted to $USER (template: $TEMPLATE, nopass: $NOPASS)" >> /var/log/krust-sudo.log 2>/dev/null || true

echo ""
blue "💡 Usage examples for $USER:"
case "$TEMPLATE" in
    "developer")
        echo "   sudo apt update"
        echo "   sudo systemctl restart nginx"  
        echo "   sudo docker ps"
        ;;
    "operator")
        echo "   sudo systemctl status mysql"
        echo "   sudo tail /var/log/syslog"
        echo "   sudo journalctl -u nginx"
        ;;
    "readonly")
        echo "   sudo cat /var/log/auth.log"
        echo "   sudo journalctl -f"
        echo "   sudo dmesg"
        ;;
    "webadmin")
        echo "   sudo systemctl restart nginx"
        echo "   sudo nginx -t"
        echo "   sudo tail /var/log/nginx/error.log"
        ;;
    "dbadmin")
        echo "   sudo systemctl restart mysql"
        echo "   sudo mysqldump database_name"
        echo "   sudo tail /var/log/mysql/error.log"
        ;;
    *)
        echo "   sudo <any_command>"
        ;;
esac
"#,
        user = user,
        sudo_rule = sudo_rule.replace('\'', "'\\''"),
        expire_time = expire_at_str.unwrap_or_default(),
        template = template,
        nopass = nopass
    );

    Ok(format!("bash -c {}", shell_escape::unix::escape(script.into())))
}
