use anyhow::{Result, bail};

pub fn build_command(args: &[String]) -> Result<String> {
    if !args.is_empty() {
        bail!("os-update module does not take any arguments.");
    }

    let script = r#"
set -e
echo "Starting OS update process..."

if command -v apt-get > /dev/null; then
    echo "Detected Debian-based system (apt)."
    export DEBIAN_FRONTEND=noninteractive
    apt-get update -y
    apt-get upgrade -y --with-new-pkgs
elif command -v yum > /dev/null; then
    echo "Detected RHEL/CentOS system (yum)."
    yum -y update
elif command -v dnf > /dev/null; then
    echo "Detected Fedora/RHEL system (dnf)."
    dnf -y upgrade
elif command -v pacman > /dev/null; then
    echo "Detected Arch-based system (pacman)."
    pacman -Sy --noconfirm && pacman -Su --noconfirm
elif command -v zypper > /dev/null; then
    echo "Detected SUSE-based system (zypper)."
    zypper refresh && zypper update -y
else
    echo "No supported package manager found on this system."
    exit 1
fi

echo "OS update process completed."
"#;

    Ok(format!("bash -c {}", shell_escape::unix::escape(script.into())))
}

