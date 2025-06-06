# Crusty

CLI tool for remote command execution over SSH.  
Still in early development — getting built and improved daily.

## What it does

- Connects to remote Linux hosts over SSH
- Runs shell commands (e.g., `uptime`, updates, reboots)
- Works with username + password auth
- Modular structure — each task is a self-contained module

## Current state

- Output is partially mocked (e.g. "fake output") for now
- Working on real SSH execution and output capture
- Project is personal and evolving — not stable yet

## Roadmap

- [ ] Switch from placeholder to real command results
- [ ] Support SSH keys
- [ ] Retry logic, error types, structured results
- [ ] Parallelism / async improvements
- [ ] CLI improvements and flags

## Usage (WIP)

Basic idea:
crusty --inventory oracle-servers --user root --pass hunter2 --cmd "uptime"

