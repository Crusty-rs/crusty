# krust 🦀

A blazing-fast, minimal parallel SSH runner — built in **Rust** for real-world automation, reliability, and control.

---

## 🚀 Overview

**krust** is a Rust-based parallel SSH command runner built for modern DevOps workflows. It executes shell commands over SSH across many machines **concurrently** with real-time output streaming, minimal latency, and zero bloat.

---

## ✅ What’s New

- True async parallelism using `FuturesUnordered` + `Semaphore`
- Real streaming output with **NDJSON**
- Hardened SSH support: agent, timeout, retries
- Improved error handling & retry logic
- Clean minimal JSON structure for automation
- Simpler, focused module system (`sudo`, `os-update`, `reboot-wait`)
- No memory accumulation over large operations
- Clear exit codes and better CLI help

---

## 🛠️ Upcoming Focus Areas

We're actively improving:

- 🔄 Battle-tested retry behavior (handle flaky networks better)
- 🧪 Module unit testing & script validation
- 🚀 SSH connection speed improvements (timeouts, retries, pooling ideas)
- 📚 Better documentation on flags, module usage, and auth setup
- 🧰 User experience (UX) improvements (clear errors, flag help, progress)
- 🧩 Thinking of new modules (feedback welcome)
- 📤 Support for people who want JSON: just add `--json` for streaming NDJSON

---

## 📦 CLI Examples

# Basic parallel execution
krust --hosts web1,web2,web3 --concurrency 20 uptime

# With inventory file
krust --inventory hosts.txt --user deploy 'systemctl status nginx'

# Streaming JSON output
krust --hosts prod --json 'df -h' | jq -r '.hostname + ": " + .stdout'

# Using built-in modules
krust --hosts all sudo alice --nopass
krust --hosts prod os-update --security-only
krust --hosts db reboot-wait --check

# Custom timeout & retries
krust --hosts flaky --retries 5 --timeout 60s 'curl http://localhost/health'

---

## 🔐 SSH Features

- DNS resolution with timeout
- Per-host read & write timeout control
- SSH agent + key file auth
- Password auth (optional, secure)
- Retry logic for transient failures

---

## 📈 Output Format

If `--json` is passed, krust prints each result as **one line of JSON** (NDJSON), like:

{"hostname":"host1","stdout":"ok","stderr":null,"exit_code":0,"timestamp":"..."}

No memory bloat: results are streamed immediately.

---

## 📁 Module Architecture

Modules are easy to extend — just define `build_command()` in `modules/your_module.rs` and register in `modules/mod.rs`.

Current modules:

- `sudo`: grant sudoer access to a user
- `os-update`: run apt/yum updates
- `reboot-wait`: delay or check reboot requirement

---

## 🧪 Test & Dev Roadmap

- Write `#[test]` for all module logic
- Start `bat`-style CLI regression tests
- Split tests: `unit`, `integration`, `ssh-mock`

---

## 🤝 Contributing

Got feedback? Found a bug? Want a new module?
Open an issue or PR. Let's build krust into a truly powerful DevOps hammer 🔨🤖🔧

---

> Made in Rust. Runs fast. Talks SSH. Solves problems.

