# krust

Fast parallel SSH command executor. Built in Rust for fast experiments or actions.

## What It Does

`krust` runs shell commands across hundreds of servers simultaneously. Nothing more, nothing less.

- Pure SSH command execution
- True parallel operations with controlled concurrency  
- Smart retry logic for network failures
- Clean output formats (text, JSON, filtered)
- Zero dependencies on remote hosts

## Why

When you need to run `df -h` or 'touch file' on 1000 servers, waiting 10 minutes for serial execution is not worth it. `krust` completes the same task in under 10 seconds.

## Installation

```bash
cargo install krust
```

Or build from source:

```bash
cargo build --release
cp target/release/krust /usr/local/bin/
```

## Basic Usage

```bash
# Check disk space on some hosts
krust -u root -k ~/.ssh/id_rsa --hosts servera,serverb,serverc df -h

# Restart nginx across all production hosts inventory  
krust -u deploy -k ~/.ssh/id_rsa -i prod-hosts.txt 'sudo systemctl restart nginx'

# Get memory stats with JSON output for automation
krust -u root -k ~/.ssh/id_rsa -i db1,db2 --json free -m | jq '.stdout'
```

## Authentication

`krust` intelligently tries authentication methods in order:

1. SSH key (default: `~/.ssh/id_rsa`)
2. SSH agent
3. Password (if provided)

```bash
# Use specific key
krust -u devops --hosts server1 --private-key ~/.ssh/deploy_key uptime

# Interactive password prompt
krust -u sample --hosts legacy1 --ask-pass 'cat /etc/redhat-release'

# Explicit password (use with caution)
krust -u root --inventory old-server --password 'secret123' hostname
```

## Output Formats

### Default Text Output

Clean, colored output optimized for human reading:

```
[10/10] hosts completed

=== EXECUTION SUMMARY ===
Total: 10 | Success: 9 | Failed: 1

✓ SUCCESSFUL HOSTS:
  web1 (127ms):
    Linux web1 5.15.0-92-generic
  web2 (143ms):
    Linux web2 5.15.0-92-generic

✗ FAILED HOSTS:
  web3: Connection timeout
```

### JSON Output

Stream results as NDJSON for real-time processing:

```bash
--json 'kubectl get nodes' | while read line; do
  echo "$line" | jq -r '.hostname + ": " + (.exit_code // "failed")'
done
```

### Pretty JSON

Human-readable JSON with all results:

```bash
--pretty-json --fields hostname,exit_code uptime
```

### Concurrency Control

```bash
# Careful mode: 5 connections at a time
--concurrency 5 'apt update'

# Aggressive mode: 100 parallel connections
--concurrency 100 'nginx -t'
```

### Timeouts and Retries

```bash
# Long-running commands
--timeout 5m 'tar -czf /backup/full.tar.gz /data'

# Unreliable network
--retries 5 --timeout 60s ping -c 1 google.com
```

### Inventory Files

Simple text format, one host per line:

```
# web-servers.txt
web1.example.com
web2.example.com:2222  # custom port
10.0.1.50
# db servers
db1.internal
db2.internal
```

## Production Patterns

### Health Checks

```bash
#!/bin/bash
krust -u root -k ~/path/to/key --inventory prod.txt --json 'curl -sf http://localhost/health' \
  | jq -r 'select(.exit_code != 0) | .hostname' \
  | xargs -I{} alert-team "Health check failed on {}"
```

### Rolling Restarts

```bash
for batch in $(cat hosts.txt | xargs -n 10); do
  krust -u root -k ~/path/to/key --hosts "$batch" --concurrency 5 'sudo systemctl restart app'
  sleep 30
done
```

### Audit Compliance

```bash
krust -u root -k ~/path/to/key--inventory all-servers.txt --json --pretty-json \
  'grep PermitRootLogin /etc/ssh/sshd_config' \
  > ssh-audit-$(date +%Y%m%d).json
```

## Design Philosophy

- **Minimal**: No plugins, modules, or remote dependencies
- **Fast**: Parallel by default, optimized for thousands of hosts
- **Reliable**: Smart retries, proper timeouts, clear error reporting
- **Composable**: Clean JSON output works with standard Unix tools


## Error Handling

`krust` exits with code 1 if any host fails. Parse JSON output for granular error handling:

```bash
if ! krust --hosts critical --json 'systemctl is-active postgresql' > results.json; then
  failed=$(jq -r 'select(.exit_code != 0) | .hostname' results.json)
  echo "PostgreSQL down on: $failed"
fi
```

## Contributing

We value simplicity and performance. Before adding features, ask:

1. Does this keep `krust` minimal?
2. Does it make common tasks easier?
3. Will it work reliably in production?

# krust Examples

Real-world usage patterns for production environments.

## Quick Start

### Basic Commands

```bash
# Single host
krust -u root -k ~/path/to/key --hosts server1.example.com uptime

# Multiple hosts
krust -u root -k ~/path/to/key --hosts web1,web2,web3 'df -h /'

# From inventory file
krust -u root -k ~/path/to/key --inventory production.txt 'free -m'

# Custom SSH port
krust -u root -k ~/path/to/key --hosts server1:2222 hostname
```

### Authentication

```bash
# Default key (~/.ssh/id_rsa or id_ed25519)
krust --hosts prod1 whoami

# Specific key
krust --hosts secure1 --private-key ~/.ssh/deploy_key id

# Password authentication (interactive)
krust --hosts legacy1 --ask-pass 'cat /etc/os-release'

# SSH agent (default if no key found)
ssh-add ~/.ssh/special_key
krust --hosts cluster 'kubectl get nodes'
```

## Output Formats

### Human-Readable (Default)

```bash
$ krust -u root -k ~/path/to/key--hosts web1,web2,db1 'systemctl is-active nginx'

[3/3] hosts completed

=== EXECUTION SUMMARY ===
Total: 3 | Success: 2 | Failed: 1

✓ SUCCESSFUL HOSTS:
  web1 (89ms):
    active
  web2 (92ms):
    active

✗ FAILED HOSTS:
  db1: Unit nginx.service could not be found.
```

### JSON Streaming

Perfect for real-time monitoring:

```bash
krust --u root -k ~/path/to/key --hosts all --json 'curl -s -o /dev/null -w "%{http_code}" http://localhost/health' | \
while read line; do
  host=$(echo "$line" | jq -r .hostname)
  code=$(echo "$line" | jq -r .stdout)
  if [ "$code" != "200" ]; then
    echo "ALERT: $host returned $code"
  fi
done
```

## License
MIT
