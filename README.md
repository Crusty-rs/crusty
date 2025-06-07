# 🦀 krust - Fast Parallel SSH Command Execution in Rust

**krust** is a blazing-fast, minimal, Rust-based CLI tool for executing commands over SSH across multiple hosts — with structured JSON output and native concurrency. Think `pssh` meets `jq`, but on rocket fuel. It’s designed for ops teams who want raw speed, clarity, and confidence when running remote tasks at scale.

---

## 🚧 Current Focus / Known Issues

### 1. ⚙️ Parallel Execution Works Best with Clean Inventory
- Works reliably when inventory is accurate.
- Fails silently or inconsistently when:
  - **An SSH agent misbehaves**
  - **A node is listed multiple times** (only first seen gets used)
  - **One bad node blocks responses from others** ❗

### 2. 📉 Need to Minimize Output for Performance
- Large outputs from multiple nodes can overwhelm the terminal or CLI pipe.
- Consider trimming logs or output streaming via flag.

### 3. 🧪 Need Battle-Test CLI First
- Core CLI must be hardened against edge cases.
- Retry logic, partial feedback, timeout strategies needed.

### 4. 📦 Modules Later (Pluggable System)
- Start lean with just CLI + JSON.
- Build toward a modular command model (think plugins, typed input/output, pipelines).

---

### 👊 Next Steps

- [ ] Fix agent-related skipping of repeated hosts
- [ ] Improve failure isolation (1 bad node ≠ blocked batch)
- [ ] Output throttling or pagination
- [ ] Stress test under real node load
- [ ] Begin CLI flags refactor for future module system

---

PRs welcome. Stability over flash. 🔨🤖

