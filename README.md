# EDF — Early Deadline First Scheduling Platform

A complete platform for designing, simulating, and analyzing **real-time safety-critical systems** using the Early Deadline First (EDF) scheduling algorithm. Built with a Rust backend and vanilla JS frontends.

## Overview

The platform consists of three integrated web applications served from a single HTTP server:

| App | Route | Description |
|-----|-------|-------------|
| **Topology Builder** | `/builder/` | Visual editor for designing module topologies with ports, connections, and use cases |
| **EDF Simulator** | `/simulator/` | Configure and run EDF scheduling simulations with Gantt chart visualization |
| **Topology Viewer** | `/viewer/` | Animated playback of scheduling results overlaid on the topology graph |

All three apps share a common data directory (`Topologies/`) and communicate through the MCP server for AI-assisted topology generation.

## Quick Start

```bash
# Prerequisites: Rust toolchain (https://rustup.rs)

# Clone and build
git clone https://github.com/guizmo31/EarlyDeadlineFirst.git
cd EarlyDeadlineFirst
cargo build --release

# Start the server
cargo run --release -p edf-server

# Open http://localhost:8080 (redirects to /simulator/)
```

## Project Structure

```
EarlyDeadlineFirst/
├── edf-core/           # Rust library — EDF scheduling engine
│   └── src/lib.rs      # Algorithm implementation + metrics
├── edf-server/         # Rust binary — HTTP server (actix-web)
│   └── src/main.rs     # REST API + static file server
├── edf-web/            # Simulator frontend (HTML/CSS/JS)
├── edf-builder/        # Topology Builder frontend
│   ├── app.js          # Builder logic (modules, instances, connections, use cases)
│   ├── ws-client.js    # WebSocket bridge for MCP live sync
│   └── mcp-server/     # MCP server for AI agent integration
│       ├── server.js   # MCP tools + HTTP file API + WebSocket bridge
│       └── topology-state.js  # Headless topology engine
├── edf-viewer/         # Topology Viewer frontend
│   └── app.js          # Animated Gantt + topology playback with metrics
├── Topologies/         # Shared topology & config JSON files
└── Cargo.toml          # Rust workspace
```

---

## EDF Core — Scheduling Algorithm

### What is EDF?

**Early Deadline First** is a dynamic-priority preemptive scheduling algorithm. At every scheduling point, the CPU executes the task with the nearest deadline. It is **optimal for uniprocessor systems**: if a task set is schedulable, EDF will find a valid schedule.

### Feasibility Condition (single core)

A set of periodic tasks is schedulable by EDF if and only if total CPU utilization does not exceed 100%:

```
U = Σ (Cᵢ / Tᵢ) ≤ 1.0
```

Where `Cᵢ` is the worst-case execution time and `Tᵢ` is the period of task `i`.

### Multi-Core Global EDF

On a system with `M` cores, the engine implements **global EDF** with a two-pass assignment:

1. **EDF Sort** — All ready jobs are sorted by `(deadline, priority, index)`. Lower deadline = higher urgency. Priority (0 = highest) breaks ties.

2. **Pass 1 — Pinned tasks** — Jobs with core affinity (`pinned_core`) are assigned first to their designated core, in EDF order.

3. **Pass 2 — Free tasks** — Remaining jobs fill available cores in order (Core 0, Core 1, ...), providing natural load balancing.

4. **Preemption** — At every scheduling point, running jobs can be preempted if a higher-urgency job becomes ready.

### Intra-Tick Rescheduling

The scheduler does **not** simply advance tick-by-tick. Within each tick interval, it implements a **sub-tick loop**:

```
for each tick:
    tick_end = current_time + tick_period
    sub_time = current_time
    while sub_time < tick_end:
        sort ready jobs by EDF
        assign jobs to cores
        advance = min(remaining_in_tick, min(job.remaining for assigned jobs))
        execute for 'advance' microseconds
        detect completions → release dependent jobs, update FIFOs
        sub_time += advance
        // re-sort and re-assign on next iteration
```

This means the scheduler **re-evaluates on every job completion**, not just at tick boundaries. This is critical for data-driven chains where downstream tasks should start as soon as their inputs are available.

### Dependency Chains & FIFO Connections

Tasks can declare dependencies on other tasks:
- **Standard (S&H — Sample & Hold)**: Consumer waits for producer to complete in the current period before starting.
- **Double-buffered**: Consumer reads data from the **previous** period (1-period latency), allowing producer and consumer to execute in parallel.

### Fixed Partitioning

When `fixed_partitioning` is enabled, once a task starts on a core, it stays there for all subsequent executions (no migration). This models real-world AUTOSAR-style core allocation.

### Configuration Complexity

An EDF configuration requires careful tuning of many parameters:

| Parameter | Scope | Description |
|-----------|-------|-------------|
| `tick_period_ms` | Global | Scheduling granularity. Lower = more precise but higher overhead |
| `simulation_duration_ms` | Global | How long to simulate |
| `num_cores` | Global | Number of CPU cores (1–16) |
| `fixed_partitioning` | Global | Disable task migration between cores |
| `period_ms` | Per-process | Activation period |
| `cpu_time_ms` | Per-process | WCET — worst-case execution time per job |
| `priority` | Per-process | Static priority for EDF tie-breaking (0 = highest) |
| `pinned_core` | Per-process | Force execution on a specific core |
| `dependencies` | Per-process | List of processes that must complete first |
| `double_buffer_deps` | Per-process | Dependencies using double buffering |

This is why the **Topology Builder** exists — it provides a visual way to design complex multi-rate systems and automatically generates the EDF configuration.

### Metrics Output

The simulator computes comprehensive metrics:

- **Per-process**: Best/Worst/Avg response time, jitter, slack (positive = margin, negative = overrun)
- **Per-chain**: End-to-end latency from root sensor to leaf actuator (best/worst/avg)
- **Deadline misses**: Every instance where a job failed to complete before its deadline

---

## Topology Builder

The Builder is a visual editor for designing safety-critical software architectures.

### Concepts

- **Module**: A reusable software component template with typed input/output ports, WCET/BCET, ASIL level, and resource requirements.
- **Instance**: A placed copy of a module on the canvas, with its own activation pattern (PERIODIC, DATA_DRIVEN, SPORADIC) and timing parameters.
- **Connection**: A data flow link from an output port of one instance to an input port of another. Can be standard (S&H) or double-buffered.
- **Use Case**: A logical grouping of instances that form a functional chain (e.g., "Lane Keeping Assist").

### Workflow

1. **Create modules** in the left panel (define ports, timing, ASIL)
2. **Drag modules** onto the canvas to create instances
3. **Connect ports** by dragging from output to input
4. **Group into Use Cases** for organizational clarity
5. **Configure** activation patterns and periods in the properties panel
6. **Launch EDF Scheduling** to generate config and open the simulator

### MCP Server (AI Integration)

The Builder includes an MCP (Model Context Protocol) server that enables AI agents like Claude to create and manipulate topologies programmatically. It operates in **dual mode**:

- **Headless**: Full topology manipulation without a browser
- **Live sync**: Changes are pushed to the browser in real-time when connected

Start the MCP server: `node edf-builder/mcp-server/server.js`

---

## EDF Simulator

Interactive simulation and visualization of EDF scheduling.

### Features

- Configure tick period, duration, number of cores
- Add/remove processes with individual timing parameters
- Core pinning and fixed partitioning options
- Real-time Gantt chart rendering (per-core and per-process views)
- Visual deadline miss indicators
- CPU utilization statistics
- Save/load configurations from server or local files

### API

```
POST /api/simulate — Run EDF simulation (JSON body: SchedulerConfig)
GET  /api/health   — Health check
```

---

## Topology Viewer

Animated playback of scheduling simulation results overlaid on the topology graph.

### Features

- **Split view**: Topology graph + Gantt chart side by side
- **Full topology view**: Expanded topology with animated node execution
- **Metrics tab**: Summary cards, per-process response time table, chain latency table, deadline miss list
- **Playback controls**: Play/pause, speed (1x–64x), scrub timeline, loop mode
- **Visual feedback**: Executing nodes glow with per-core colors, connections animate data flow, FIFO/double-buffer badges

---

## Building

### Prerequisites

- **Rust** (stable) — [rustup.rs](https://rustup.rs/)
- **Node.js** (for MCP server) — [nodejs.org](https://nodejs.org/)
- On Windows with GNU toolchain: MinGW (`scoop install mingw`)

### Commands

```bash
# Build everything
cargo build --release

# Run tests (18 tests including intra-tick rescheduling, metrics, chains)
cargo test -p edf-core

# Start the server
cargo run --release -p edf-server

# Install MCP server dependencies (optional, for AI integration)
cd edf-builder/mcp-server && npm install
```

## License

MIT
