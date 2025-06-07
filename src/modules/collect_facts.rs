use anyhow::{Result, bail};

pub fn build_command(args: &[String]) -> Result<String> {
    let mut output_format = "json";
    let mut csv_file: Option<String> = None;
    let mut include_services = false;
    let mut include_network = true;
    let mut include_disks = true;
    let mut csv_columns: Vec<String> = Vec::new();

    // Parse arguments
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--format" => {
                if i + 1 < args.len() {
                    output_format = &args[i + 1];
                    i += 2;
                } else {
                    bail!("--format requires a value (json|csv)");
                }
            }
            "--csv-output" => {
                if i + 1 < args.len() {
                    csv_file = Some(args[i + 1].clone());
                    output_format = "csv";
                    i += 2;
                } else {
                    bail!("--csv-output requires a filename");
                }
            }
            "--csv-columns" => {
                if i + 1 < args.len() {
                    csv_columns.extend(args[i + 1].split(',').map(|s| s.trim().to_string()));
                    i += 2;
                } else {
                    bail!("--csv-columns requires comma-separated column names");
                }
            }
            "--include-services" => {
                include_services = true;
                i += 1;
            }
            "--no-network" => {
                include_network = false;
                i += 1;
            }
            "--no-disks" => {
                include_disks = false;
                i += 1;
            }
            "--help" => {
                println!("KRUST collect-facts module
Usage: krust collect-facts [OPTIONS]

Options:
    --format FORMAT         Output format: json, csv (default: json)
    --csv-output FILE       Export to CSV file (automatically sets format to csv)
    --csv-columns COLS      Specific CSV columns (comma-separated)
    --include-services      Include running services list
    --no-network           Skip network interface collection
    --no-disks             Skip disk usage collection
    --help                 Show this help

Available CSV columns:
    hostname,ip_address,os_name,os_version,kernel_version,architecture,
    cpu_cores,memory_total_gb,memory_available_gb,uptime_hours,load_average,
    timezone,users_logged_in,package_manager,docker_installed,python_version,
    ssh_port,firewall_status,last_boot_time,collection_timestamp

Examples:
    krust --hosts servers collect-facts
    krust --hosts all collect-facts --csv-output inventory.csv
    krust --hosts prod collect-facts --csv-columns hostname,ip_address,os_name --csv-output prod.csv
    krust --hosts all collect-facts --include-services --format json");
                return Ok("echo 'Help displayed'".to_string());
            }
            _ => {
                bail!("Unknown argument: {}. Use --help for usage.", args[i]);
            }
        }
    }

    // Default CSV columns if not specified
    if output_format == "csv" && csv_columns.is_empty() {
        csv_columns = vec![
            "hostname".to_string(), "ip_address".to_string(), "os_name".to_string(), 
            "os_version".to_string(), "architecture".to_string(), "cpu_cores".to_string(), 
            "memory_total_gb".to_string(), "package_manager".to_string(), "docker_installed".to_string()
        ];
    }

    let csv_filename = csv_file.unwrap_or_else(|| "inventory.csv".to_string());

    let script = format!(r#"
#!/bin/bash
set -e

# Color output functions
red() {{ echo -e "\033[31m$1\033[0m"; }}
green() {{ echo -e "\033[32m$1\033[0m"; }}
yellow() {{ echo -e "\033[33m$1\033[0m"; }}
blue() {{ echo -e "\033[34m$1\033[0m"; }}

OUTPUT_FORMAT="{output_format}"
CSV_FILE="{csv_filename}"
CSV_COLUMNS=({csv_columns})
INCLUDE_SERVICES={include_services}
INCLUDE_NETWORK={include_network}
INCLUDE_DISKS={include_disks}

green "🔍 KRUST Fact Collection Starting..."
blue "📊 Output format: $OUTPUT_FORMAT"
if [[ "$OUTPUT_FORMAT" == "csv" ]]; then
    blue "📁 CSV file: $CSV_FILE"
    blue "📋 CSV columns: ${{CSV_COLUMNS[*]}}"
fi

# Basic system information
HOSTNAME=$(hostname -f 2>/dev/null || hostname)
IP_ADDRESS=$(ip route get 1.1.1.1 2>/dev/null | grep -oP 'src \K\S+' || echo "unknown")
OS_NAME=$(grep '^NAME=' /etc/os-release 2>/dev/null | cut -d'"' -f2 || uname -s)
OS_VERSION=$(grep '^VERSION=' /etc/os-release 2>/dev/null | cut -d'"' -f2 || uname -r)
KERNEL_VERSION=$(uname -r)
ARCHITECTURE=$(uname -m)
CPU_CORES=$(nproc)
UPTIME_HOURS=$(awk '{{printf "%.1f", $1/3600}}' /proc/uptime 2>/dev/null || echo "0")
LOAD_AVERAGE=$(cat /proc/loadavg 2>/dev/null | cut -d' ' -f1-3 || echo "unknown")
TIMEZONE=$(timedatectl show --property=Timezone --value 2>/dev/null || date +%Z)
USERS_LOGGED_IN=$(who | wc -l)
LAST_BOOT_TIME=$(who -b 2>/dev/null | awk '{{print $3" "$4}}' || uptime -s 2>/dev/null || echo "unknown")
SSH_PORT=$(ss -tlnp 2>/dev/null | grep :22 | head -1 | awk '{{print $4}}' | cut -d: -f2 || echo "22")
COLLECTION_TIMESTAMP=$(date -Iseconds)

# Memory information (in GB)
MEMORY_TOTAL_GB=$(awk '/MemTotal/ {{printf "%.1f", $2/1024/1024}}' /proc/meminfo 2>/dev/null || echo "0")
MEMORY_AVAILABLE_GB=$(awk '/MemAvailable/ {{printf "%.1f", $2/1024/1024}}' /proc/meminfo 2>/dev/null || echo "0")

# Package manager detection
PACKAGE_MANAGER="unknown"
if command -v apt >/dev/null 2>&1; then
    PACKAGE_MANAGER="apt"
elif command -v yum >/dev/null 2>&1; then
    PACKAGE_MANAGER="yum"
elif command -v dnf >/dev/null 2>&1; then
    PACKAGE_MANAGER="dnf"
elif command -v pacman >/dev/null 2>&1; then
    PACKAGE_MANAGER="pacman"
elif command -v zypper >/dev/null 2>&1; then
    PACKAGE_MANAGER="zypper"
fi

# Docker detection
DOCKER_INSTALLED="No"
if command -v docker >/dev/null 2>&1; then
    DOCKER_INSTALLED="Yes"
fi

# Python version
PYTHON_VERSION=$(python3 --version 2>/dev/null | cut -d' ' -f2 || echo "Not installed")

# Firewall status
FIREWALL_STATUS="unknown"
if systemctl is-active --quiet ufw 2>/dev/null; then
    FIREWALL_STATUS="ufw-active"
elif systemctl is-active --quiet firewalld 2>/dev/null; then
    FIREWALL_STATUS="firewalld-active"
elif systemctl is-active --quiet iptables 2>/dev/null; then
    FIREWALL_STATUS="iptables-active"
else
    FIREWALL_STATUS="inactive"
fi

blue "📊 Collecting detailed information..."

if [[ "$OUTPUT_FORMAT" == "csv" ]]; then
    # CSV Output Mode
    green "📝 Generating CSV output..."
    
    # Create header if file doesn't exist
    if [[ ! -f "$CSV_FILE" ]]; then
        HEADER=""
        for col in "${{CSV_COLUMNS[@]}}"; do
            if [[ -n "$HEADER" ]]; then
                HEADER="$HEADER,"
            fi
            HEADER="$HEADER$col"
        done
        echo "$HEADER" > "$CSV_FILE"
        blue "📋 Created CSV header in $CSV_FILE"
    fi
    
    # Generate data row
    DATA=""
    for col in "${{CSV_COLUMNS[@]}}"; do
        if [[ -n "$DATA" ]]; then
            DATA="$DATA,"
        fi
        
        case "$col" in
            "hostname") DATA="$DATA$HOSTNAME" ;;
            "ip_address") DATA="$DATA$IP_ADDRESS" ;;
            "os_name") DATA="$DATA\"$OS_NAME\"" ;;
            "os_version") DATA="$DATA\"$OS_VERSION\"" ;;
            "kernel_version") DATA="$DATA$KERNEL_VERSION" ;;
            "architecture") DATA="$DATA$ARCHITECTURE" ;;
            "cpu_cores") DATA="$DATA$CPU_CORES" ;;
            "memory_total_gb") DATA="$DATA$MEMORY_TOTAL_GB" ;;
            "memory_available_gb") DATA="$DATA$MEMORY_AVAILABLE_GB" ;;
            "uptime_hours") DATA="$DATA$UPTIME_HOURS" ;;
            "load_average") DATA="$DATA\"$LOAD_AVERAGE\"" ;;
            "timezone") DATA="$DATA$TIMEZONE" ;;
            "users_logged_in") DATA="$DATA$USERS_LOGGED_IN" ;;
            "package_manager") DATA="$DATA$PACKAGE_MANAGER" ;;
            "docker_installed") DATA="$DATA$DOCKER_INSTALLED" ;;
            "python_version") DATA="$DATA\"$PYTHON_VERSION\"" ;;
            "ssh_port") DATA="$DATA$SSH_PORT" ;;
            "firewall_status") DATA="$DATA$FIREWALL_STATUS" ;;
            "last_boot_time") DATA="$DATA\"$LAST_BOOT_TIME\"" ;;
            "collection_timestamp") DATA="$DATA\"$COLLECTION_TIMESTAMP\"" ;;
            *) DATA="$DATA\"unknown\"" ;;
        esac
    done
    
    # Append data to CSV file
    echo "$DATA" >> "$CSV_FILE"
    
    green "✅ CSV data exported to $CSV_FILE"
    blue "📊 Data row added for $HOSTNAME"
    
    # Show preview if possible
    if command -v column >/dev/null 2>&1 && [[ -f "$CSV_FILE" ]]; then
        echo ""
        blue "📋 CSV Preview (last 3 rows):"
        tail -3 "$CSV_FILE" | column -t -s ','
    fi
    
else
    # JSON Output Mode (default)
    green "📝 Generating JSON output..."
    
    # Start JSON output
    echo "{{"
    echo "  \"hostname\": \"$HOSTNAME\","
    echo "  \"ip_address\": \"$IP_ADDRESS\","
    echo "  \"os_name\": \"$OS_NAME\","
    echo "  \"os_version\": \"$OS_VERSION\","
    echo "  \"kernel_version\": \"$KERNEL_VERSION\","
    echo "  \"architecture\": \"$ARCHITECTURE\","
    echo "  \"cpu_cores\": $CPU_CORES,"
    echo "  \"memory_total_gb\": $MEMORY_TOTAL_GB,"
    echo "  \"memory_available_gb\": $MEMORY_AVAILABLE_GB,"
    echo "  \"uptime_hours\": $UPTIME_HOURS,"
    echo "  \"load_average\": \"$LOAD_AVERAGE\","
    echo "  \"timezone\": \"$TIMEZONE\","
    echo "  \"users_logged_in\": $USERS_LOGGED_IN,"
    echo "  \"package_manager\": \"$PACKAGE_MANAGER\","
    echo "  \"docker_installed\": \"$DOCKER_INSTALLED\","
    echo "  \"python_version\": \"$PYTHON_VERSION\","
    echo "  \"ssh_port\": $SSH_PORT,"
    echo "  \"firewall_status\": \"$FIREWALL_STATUS\","
    echo "  \"last_boot_time\": \"$LAST_BOOT_TIME\","
    echo "  \"collection_timestamp\": \"$COLLECTION_TIMESTAMP\","

    # Disk usage information
    if [[ "$INCLUDE_DISKS" == "true" ]]; then
        echo "  \"disk_usage\": ["
        df -h --output=source,target,size,used,avail,pcent,fstype 2>/dev/null | tail -n +2 | while IFS= read -r line; do
            if [[ "$line" =~ ^/dev ]]; then
                read -r device mount total used avail percent fstype <<< "$line"
                total_gb=$(echo "$total" | sed 's/[^0-9.]//g')
                used_gb=$(echo "$used" | sed 's/[^0-9.]//g')
                avail_gb=$(echo "$avail" | sed 's/[^0-9.]//g')
                percent_num=$(echo "$percent" | sed 's/%//')
                
                echo "    {{"
                echo "      \"device\": \"$device\","
                echo "      \"mount_point\": \"$mount\","
                echo "      \"total_gb\": ${{total_gb:-0}},"
                echo "      \"used_gb\": ${{used_gb:-0}},"
                echo "      \"available_gb\": ${{avail_gb:-0}},"
                echo "      \"usage_percent\": ${{percent_num:-0}},"
                echo "      \"filesystem\": \"$fstype\""
                echo "    }},"
            fi
        done | sed '$ s/,$//'
        echo "  ],"
    else
        echo "  \"disk_usage\": [],"
    fi

    # Network interfaces
    if [[ "$INCLUDE_NETWORK" == "true" ]]; then
        echo "  \"network_interfaces\": ["
        ip -j addr show 2>/dev/null | jq -r '.[] | select(.ifname != "lo") | "\(.ifname)|\(.addr_info[0].local // "no-ip")|\(.address)|\(.operstate)"' 2>/dev/null | while IFS='|' read -r name ip mac state; do
            echo "    {{"
            echo "      \"name\": \"$name\","
            echo "      \"ip_address\": \"$ip\","
            echo "      \"mac_address\": \"$mac\","
            echo "      \"state\": \"$state\","
            echo "      \"speed\": null"
            echo "    }},"
        done | sed '$ s/,$//' 2>/dev/null || echo "    {{\"name\": \"eth0\", \"ip_address\": \"$IP_ADDRESS\", \"mac_address\": \"unknown\", \"state\": \"UP\", \"speed\": null}}"
        echo "  ],"
    else
        echo "  \"network_interfaces\": [],"
    fi

    # Running services (if requested)
    if [[ "$INCLUDE_SERVICES" == "true" ]]; then
        echo "  \"services_running\": ["
        systemctl list-units --type=service --state=running --no-pager --no-legend 2>/dev/null | head -10 | awk '{{print "\\"" $1 "\\""}}'  | paste -sd ',' || echo "\\"unknown\\""
        echo "  ],"
    else
        echo "  \"services_running\": [],"
    fi

    echo "  \"fact_collection_complete\": true"
    echo "}}"
fi

green "✅ Fact collection completed successfully!"
"#,
        output_format = output_format,
        csv_filename = csv_filename,
        csv_columns = csv_columns.iter().map(|c| format!("\"{}\"", c)).collect::<Vec<_>>().join(" "),
        include_services = include_services,
        include_network = include_network,
        include_disks = include_disks
    );

    Ok(format!("bash -c {}", shell_escape::unix::escape(script.into())))
}
