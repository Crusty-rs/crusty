use anyhow::{Result, bail};

pub fn build_command(args: &[String]) -> Result<String> {
    if !args.is_empty() {
        bail!("collect-facts module does not take any arguments.");
    }

    let script = r#"
set -e
echo "Collecting system facts..."

OS=$(uname -s)
ARCH=$(uname -m)
KERNEL=$(uname -r)

echo "{"
echo "  \"os\": \"${OS}\","
echo "  \"arch\": \"${ARCH}\","
echo "  \"kernel\": \"${KERNEL}\""
echo "}"
"#;

    Ok(format!("bash -c {}", shell_escape::unix::escape(script.into())))
}

