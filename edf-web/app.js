// Copyright (c) 2026 Ivan LE HIN
// Licensed under CC BY-NC-SA 4.0 — Non-commercial use only.
// For commercial licensing, contact the author.
// https://creativecommons.org/licenses/by-nc-sa/4.0/

(function () {
    "use strict";

    const COLORS = [
        "#e94560", "#00b4d8", "#90be6d", "#f9c74f",
        "#f3722c", "#43aa8b", "#577590", "#f8961e",
        "#9b5de5", "#06d6a0", "#ef476f", "#118ab2",
    ];
    const IDLE_COLOR = "#ffffff";
    const MISS_COLOR = "#ff0040";

    let processCounter = 0;
    let lastResult = null;
    let lastSimConfig = null; // config sent to server (for Play EDF)
    let tooltipAbort = null;

    // Topology context from localStorage
    { const saved = localStorage.getItem('edf-topology-name');
      const el = document.getElementById('topology-context');
      if (saved && el) el.textContent = saved; }

    // Use Cases state: loaded from config, toggled by checkboxes
    let useCases = [];       // { name, active, process_names }
    let allProcesses = [];   // full process list from config (never mutated)

    // ---- Use Case management ----

    function renderUseCases() {
        const section = document.getElementById("usecases-section");
        const list = document.getElementById("usecase-list");
        list.innerHTML = "";

        if (useCases.length === 0) {
            section.classList.add("hidden");
            return;
        }
        section.classList.remove("hidden");

        useCases.forEach((uc, idx) => {
            const div = document.createElement("div");
            div.className = "usecase-entry";
            div.innerHTML = `
                <label class="usecase-label">
                    <input type="checkbox" class="uc-check" data-uc-idx="${idx}" ${uc.active ? "checked" : ""}>
                    <span class="uc-name">${uc.name}</span>
                    <span class="uc-count">(${uc.process_names.length} processes)</span>
                </label>
            `;
            const checkbox = div.querySelector(".uc-check");
            checkbox.addEventListener("change", () => {
                useCases[idx].active = checkbox.checked;
                applyUseCaseFilter();
                runSimulation();
            });
            list.appendChild(div);
        });
    }

    function applyUseCaseFilter() {
        // If no use cases defined, show all processes
        if (useCases.length === 0) return;

        // Collect process names from all ACTIVE use cases
        const activeNames = new Set();
        useCases.forEach(uc => {
            if (uc.active) {
                uc.process_names.forEach(n => activeNames.add(n));
            }
        });

        // Also include processes that are NOT in any use case (always visible)
        const allUcProcessNames = new Set();
        useCases.forEach(uc => uc.process_names.forEach(n => allUcProcessNames.add(n)));

        // Show/hide process entries
        const entries = document.querySelectorAll(".process-entry");
        for (const entry of entries) {
            const name = entry.querySelector(".proc-name").value.trim();
            if (!allUcProcessNames.has(name)) {
                // Not in any use case → always visible
                entry.classList.remove("uc-hidden");
            } else if (activeNames.has(name)) {
                entry.classList.remove("uc-hidden");
            } else {
                entry.classList.add("uc-hidden");
            }
        }
    }

    // ---- Process list management ----

    function createProcessEntry(name, period, cpu, color, priority, pinnedCore, dependencies) {
        const assignedColor = color || COLORS[processCounter % COLORS.length];
        const prio = priority !== undefined ? priority : 0;
        const pinned = pinnedCore !== undefined && pinnedCore !== null;
        const pinnedVal = pinned ? pinnedCore : 0;
        const deps = (dependencies && dependencies.length > 0) ? dependencies.join(", ") : "";
        processCounter++;
        const id = processCounter;
        const div = document.createElement("div");
        div.className = "process-entry";
        div.dataset.id = id;
        div.innerHTML = `
            <div class="process-header">
                <input type="color" class="proc-color" value="${assignedColor}" title="Pick color">
                <input type="text" class="proc-name" value="${name}" placeholder="Name">
                <button class="btn-remove" title="Remove">&times;</button>
            </div>
            <div class="process-fields">
                <label>Period (ms):</label>
                <input type="number" class="proc-period" value="${period}" min="1">
                <label>CPU time (ms):</label>
                <input type="number" class="proc-cpu" value="${cpu}" min="1">
                <label>Priority:</label>
                <input type="number" class="proc-priority" value="${prio}" min="0" title="0 = highest priority">
            </div>
            <div class="process-pinning">
                <label class="pin-label">
                    <input type="checkbox" class="proc-pin-check" ${pinned ? "checked" : ""}>
                    Core Pinning
                </label>
                <label class="pin-core-label ${pinned ? "" : "hidden"}">
                    Core:
                    <input type="number" class="proc-pin-core" value="${pinnedVal}" min="0" max="15">
                </label>
            </div>
            <div class="process-deps">
                <label>Dependencies:</label>
                <input type="text" class="proc-deps" value="${deps}" placeholder="e.g. Process-A, Process-B" title="Comma-separated list of process names that must finish before this one starts">
            </div>
        `;
        div.querySelector(".btn-remove").addEventListener("click", () => div.remove());
        const checkbox = div.querySelector(".proc-pin-check");
        const coreLabel = div.querySelector(".pin-core-label");
        checkbox.addEventListener("change", () => {
            coreLabel.classList.toggle("hidden", !checkbox.checked);
        });
        document.getElementById("process-list").appendChild(div);
    }

    function getProcesses() {
        const entries = document.querySelectorAll(".process-entry");
        const procs = [];
        for (const entry of entries) {
            // Skip hidden (filtered out by use case) processes
            if (entry.classList.contains("uc-hidden")) continue;

            const name = entry.querySelector(".proc-name").value.trim() || `P${entry.dataset.id}`;
            const period = parseInt(entry.querySelector(".proc-period").value, 10);
            const cpu = parseInt(entry.querySelector(".proc-cpu").value, 10);
            const color = entry.querySelector(".proc-color").value;
            const priority = parseInt(entry.querySelector(".proc-priority").value, 10) || 0;
            const pinChecked = entry.querySelector(".proc-pin-check").checked;
            const pinCore = parseInt(entry.querySelector(".proc-pin-core").value, 10);
            const pinned_core = pinChecked && !isNaN(pinCore) ? pinCore : null;
            const depsStr = entry.querySelector(".proc-deps").value.trim();
            const dependencies = depsStr ? depsStr.split(",").map(s => s.trim()).filter(Boolean) : [];
            if (isNaN(period) || isNaN(cpu) || period < 1 || cpu < 1) continue;
            procs.push({ name, period_ms: period, cpu_time_ms: cpu, priority, color, pinned_core, dependencies });
        }
        return procs;
    }

    // ---- Simulation ----

    async function runSimulation() {
        const errorBox = document.getElementById("error-box");
        const statsDiv = document.getElementById("stats");
        errorBox.classList.add("hidden");
        statsDiv.classList.add("hidden");

        const tick = parseInt(document.getElementById("tick-period").value, 10);
        const duration = parseInt(document.getElementById("sim-duration").value, 10);
        const numCores = parseInt(document.getElementById("num-cores").value, 10) || 1;
        const fixedPartitioning = document.getElementById("fixed-partitioning").checked;
        const processes = getProcesses();

        if (isNaN(tick) || tick < 1) {
            showError("Tick period must be >= 1 ms");
            return;
        }
        if (isNaN(duration) || duration < 1) {
            showError("Simulation duration must be >= 1 ms");
            return;
        }
        if (processes.length === 0) {
            showError("Add at least one process (or activate a Use Case)");
            return;
        }

        for (const p of processes) {
            if (p.cpu_time_ms > p.period_ms) {
                showError(`Process "${p.name}": CPU time (${p.cpu_time_ms}) exceeds period (${p.period_ms})`);
                return;
            }
            if (p.pinned_core !== null && p.pinned_core >= numCores) {
                showError(`Process "${p.name}": pinned to core ${p.pinned_core} but only ${numCores} core(s) configured`);
                return;
            }
        }

        // Filter dependencies to only include active process names
        const activeNames = new Set(processes.map(p => p.name));
        const filteredProcesses = processes.map(p => ({
            ...p,
            dependencies: p.dependencies ? p.dependencies.filter(d => activeNames.has(d)) : [],
        }));

        const body = {
            tick_period_ms: tick,
            simulation_duration_ms: duration,
            num_cores: numCores,
            fixed_partitioning: fixedPartitioning,
            processes: filteredProcesses,
        };

        try {
            const resp = await fetch("/api/simulate", {
                method: "POST",
                headers: { "Content-Type": "application/json" },
                body: JSON.stringify(body),
            });
            if (!resp.ok) {
                const text = await resp.text();
                showError(`Server error: ${resp.status} — ${text}`);
                return;
            }
            const result = await resp.json();
            lastResult = result;
            lastResult._processes = filteredProcesses;
            lastSimConfig = buildConfig();
            showStats(result);
            drawGantt(result, filteredProcesses, duration);
            localStorage.setItem("edf-has-scheduler-config", "true");
            if (window.edfNavRefresh) window.edfNavRefresh();

            // Auto-save viewer-data.json so Play Scheduler always has fresh data
            const viewerData = {
                config: lastSimConfig,
                result: lastResult,
                topology_file: localStorage.getItem('edf-topology-file') || null,
                scheduling_file: localStorage.getItem('edf-scheduling-file') || null,
            };
            fetch(`/api/topologies/viewer-data.json`, {
                method: "POST",
                headers: { "Content-Type": "application/json" },
                body: JSON.stringify(viewerData, null, 2),
            }).catch(() => {});
            fetch(`${MCP_HTTP}/files/viewer-data.json`, {
                method: "POST",
                headers: { "Content-Type": "application/json" },
                body: JSON.stringify(viewerData, null, 2),
            }).catch(() => {});
        } catch (err) {
            showError(`Network error: ${err.message}`);
        }
    }

    function showError(msg) {
        const errorBox = document.getElementById("error-box");
        errorBox.textContent = msg;
        errorBox.classList.remove("hidden");
    }

    function showSchedulability(result) {
        const box = document.getElementById("schedulability-box");
        const sa = result.schedulability;
        if (!sa) { box.classList.add("hidden"); return; }

        box.classList.remove("hidden");
        box.className = ""; // reset
        const verdictLower = sa.verdict.toLowerCase();
        box.classList.add(`verdict-${verdictLower}`);

        let html = `<h4>Schedulability: ${sa.verdict}</h4>`;
        html += `<p>U = ${(sa.total_utilization * 100).toFixed(1)}%`;
        html += ` — Bound = ${(sa.utilization_bound * 100).toFixed(1)}%`;
        html += ` — U_max = ${(sa.max_individual_utilization * 100).toFixed(1)}%</p>`;

        if (sa.cycle_detected) {
            html += `<p style="color:#ff6b81;font-weight:bold;">Dependency cycle: ${sa.cycle_path.join(' → ')}</p>`;
        }

        if (sa.details && sa.details.length > 0) {
            html += '<ul class="analysis-details">';
            for (const d of sa.details) {
                html += `<li>${d}</li>`;
            }
            html += '</ul>';
        }
        box.innerHTML = html;
    }

    function showStats(result) {
        const statsDiv = document.getElementById("stats");
        statsDiv.classList.remove("hidden");

        // Show schedulability analysis
        showSchedulability(result);

        const numCores = result.num_cores || 1;
        const dur = result.total_duration_ms;

        // Theoretical utilization (sum of cpu/period for all processes)
        const util = (result.cpu_utilization * 100).toFixed(1);
        const utilSpan = document.getElementById("cpu-util");
        utilSpan.textContent = `${util}%`;
        utilSpan.className = "";
        if (result.cpu_utilization > 1) {
            utilSpan.className = "utilization-overload";
            utilSpan.textContent += " (OVERLOADED — not schedulable on 1 core)";
        } else if (result.cpu_utilization > 0.85) {
            utilSpan.className = "utilization-warning";
        }

        document.getElementById("cores-info").textContent = `${numCores}`;

        // Per-core utilization
        const perCoreDiv = document.getElementById("per-core-stats");
        perCoreDiv.innerHTML = "";
        let totalBusy = 0;
        for (let c = 0; c < numCores; c++) {
            const coreBusy = result.schedule
                .filter(e => e.core === c && e.process_name !== "IDLE")
                .reduce((sum, e) => sum + e.duration_ms, 0);
            totalBusy += coreBusy;
            const coreUtil = dur > 0 ? ((coreBusy / dur) * 100).toFixed(1) : "0.0";
            const coreIdle = dur - coreBusy;

            const p = document.createElement("p");
            p.className = "per-core-line";
            const utilClass = coreUtil > 100 ? "utilization-overload" : coreUtil > 85 ? "utilization-warning" : "";
            p.innerHTML = `Core ${c}: <span class="${utilClass}">${coreUtil}%</span> (busy: ${coreBusy} ms, idle: ${coreIdle} ms)`;
            perCoreDiv.appendChild(p);
        }

        // Global CPU used
        const totalSlots = dur * numCores;
        const totalIdle = totalSlots - totalBusy;
        const globalUtil = totalSlots > 0 ? ((totalBusy / totalSlots) * 100).toFixed(1) : "0.0";
        document.getElementById("cpu-total").textContent =
            `${globalUtil}% — ${totalBusy} ms / ${totalSlots} ms (IDLE: ${totalIdle} ms)`;

        // Deadline misses
        const missSpan = document.getElementById("deadline-misses");
        if (result.deadline_misses.length === 0) {
            missSpan.textContent = "None";
            missSpan.style.color = "#90be6d";
        } else {
            missSpan.textContent = `${result.deadline_misses.length} miss(es)`;
            missSpan.style.color = MISS_COLOR;
            const details = result.deadline_misses
                .map(m => `${m.process_name} @ ${m.deadline_ms}ms (${m.remaining_ms}ms left)`)
                .join(", ");
            missSpan.title = details;
        }

        // Budget overruns
        const overrunsDiv = document.getElementById("budget-overruns");
        overrunsDiv.innerHTML = "";
        if (result.budget_overruns && result.budget_overruns.length > 0) {
            overrunsDiv.innerHTML = `<p style="color:#ff9999;font-weight:bold;">WCET Overruns: ${result.budget_overruns.length}</p>` +
                result.budget_overruns.map(o =>
                    `<p class="overrun-item">${o.process_name} @ ${o.time_ms}ms (consumed ${o.consumed_ms}ms / budget ${o.budget_ms}ms)</p>`
                ).join("");
        }

        // Degraded mode events
        const degradedDiv = document.getElementById("degraded-events");
        degradedDiv.innerHTML = "";
        if (result.degraded_mode_events && result.degraded_mode_events.length > 0) {
            degradedDiv.innerHTML = `<p style="color:#ffcc00;font-weight:bold;">Degraded Mode: ${result.degraded_mode_events.length} event(s)</p>` +
                result.degraded_mode_events.map(d =>
                    `<p class="degraded-item">${d.process_name} @ ${d.time_ms}ms — ${d.reason}</p>`
                ).join("");
        }
    }

    // ---- Gantt Chart Drawing ----

    function drawGantt(result, processes, duration) {
        const canvas = document.getElementById("gantt-canvas");
        const container = document.getElementById("chart-container");
        const ctx = canvas.getContext("2d");

        const numCores = result.num_cores || 1;
        const processNames = processes.map(p => p.name);

        // Build row structure: Core rows + separator + Process rows
        const rows = [];
        for (let c = 0; c < numCores; c++) {
            rows.push({ label: `Core ${c}`, type: "core", index: c });
        }
        const separatorAfter = rows.length; // index after which we draw a separator
        for (let pi = 0; pi < processNames.length; pi++) {
            rows.push({ label: processNames[pi], type: "process", index: pi });
        }
        const rowCount = rows.length;

        const LABEL_WIDTH = 100;
        const TOP_MARGIN = 30;
        const BOTTOM_MARGIN = 30;
        const ROW_HEIGHT = 36;
        const SEPARATOR_HEIGHT = 12;
        const MIN_PX_PER_MS = 4;

        const chartWidth = Math.max(
            container.clientWidth - LABEL_WIDTH - 20,
            duration * MIN_PX_PER_MS
        );
        const canvasWidth = LABEL_WIDTH + chartWidth + 20;
        const canvasHeight = TOP_MARGIN + rowCount * ROW_HEIGHT + SEPARATOR_HEIGHT + BOTTOM_MARGIN;

        const dpr = window.devicePixelRatio || 1;
        canvas.width = canvasWidth * dpr;
        canvas.height = canvasHeight * dpr;
        canvas.style.width = canvasWidth + "px";
        canvas.style.height = canvasHeight + "px";
        ctx.scale(dpr, dpr);

        const pxPerMs = chartWidth / duration;

        // Compute Y position for a row (accounts for separator)
        function rowY(rowIdx) {
            let y = TOP_MARGIN + rowIdx * ROW_HEIGHT;
            if (rowIdx >= separatorAfter) y += SEPARATOR_HEIGHT;
            return y;
        }

        // Background
        ctx.fillStyle = "#0f1e3d";
        ctx.fillRect(0, 0, canvasWidth, canvasHeight);

        // Color map (use user-picked colors)
        const colorMap = {};
        processes.forEach((proc) => {
            colorMap[proc.name] = proc.color || COLORS[0];
        });
        colorMap["IDLE"] = IDLE_COLOR;

        // Draw rows
        for (let i = 0; i < rowCount; i++) {
            const y = rowY(i);
            const row = rows[i];

            // Alternating row background
            if (i % 2 === 0) {
                ctx.fillStyle = "#0a1830";
                ctx.fillRect(LABEL_WIDTH, y, chartWidth, ROW_HEIGHT);
            }

            // Row label
            if (row.type === "core") {
                ctx.fillStyle = "#a0c0e0";
            } else {
                ctx.fillStyle = colorMap[row.label] || "#a0a0c0";
            }
            ctx.font = "bold 12px 'Segoe UI', sans-serif";
            ctx.textAlign = "right";
            ctx.textBaseline = "middle";
            ctx.fillText(row.label, LABEL_WIDTH - 10, y + ROW_HEIGHT / 2);

            // Horizontal grid line
            ctx.strokeStyle = "#1a2a4a";
            ctx.lineWidth = 1;
            ctx.beginPath();
            ctx.moveTo(LABEL_WIDTH, y + ROW_HEIGHT);
            ctx.lineTo(LABEL_WIDTH + chartWidth, y + ROW_HEIGHT);
            ctx.stroke();
        }

        // Draw separator line between cores and processes
        const sepY = TOP_MARGIN + separatorAfter * ROW_HEIGHT + SEPARATOR_HEIGHT / 2;
        ctx.strokeStyle = "#e94560";
        ctx.lineWidth = 1;
        ctx.setLineDash([4, 4]);
        ctx.beginPath();
        ctx.moveTo(10, sepY);
        ctx.lineTo(canvasWidth - 10, sepY);
        ctx.stroke();
        ctx.setLineDash([]);
        // Section labels
        ctx.fillStyle = "#e94560";
        ctx.font = "bold 9px 'Segoe UI', sans-serif";
        ctx.textAlign = "left";
        ctx.textBaseline = "bottom";
        ctx.fillText("CORES", 4, TOP_MARGIN - 4);
        ctx.fillText("PROCESSES", 4, rowY(separatorAfter) - 4);

        // Draw time axis ticks
        const totalChartBottom = rowY(rowCount - 1) + ROW_HEIGHT;
        const tickInterval = computeTickInterval(duration, chartWidth);
        ctx.font = "10px 'Segoe UI', sans-serif";
        ctx.fillStyle = "#888";
        ctx.textAlign = "center";
        ctx.textBaseline = "top";
        for (let t = 0; t <= duration; t += tickInterval) {
            const x = LABEL_WIDTH + t * pxPerMs;
            // Vertical grid line
            ctx.strokeStyle = "#1a2a4a";
            ctx.lineWidth = 1;
            ctx.beginPath();
            ctx.moveTo(x, TOP_MARGIN);
            ctx.lineTo(x, totalChartBottom);
            ctx.stroke();

            // Tick label
            ctx.fillStyle = "#888";
            ctx.fillText(`${t}`, x, totalChartBottom + 6);
        }

        // Time axis label
        ctx.fillStyle = "#aaa";
        ctx.font = "11px 'Segoe UI', sans-serif";
        ctx.textAlign = "center";
        ctx.fillText("Time (ms)", LABEL_WIDTH + chartWidth / 2, canvasHeight - 4);

        // Draw schedule blocks on CORE rows
        const padding = 3;
        for (const entry of result.schedule) {
            const name = entry.process_name;
            const coreIdx = entry.core;
            if (coreIdx < 0 || coreIdx >= numCores) continue;

            const x = LABEL_WIDTH + entry.time_ms * pxPerMs;
            const w = entry.duration_ms * pxPerMs;
            const y = rowY(coreIdx) + padding;
            const h = ROW_HEIGHT - padding * 2;

            ctx.fillStyle = colorMap[name] || "#888";
            ctx.beginPath();
            roundRect(ctx, x, y, Math.max(w - 1, 1), h, 3);
            ctx.fill();

            // Text inside block if wide enough
            if (w > 30) {
                ctx.fillStyle = name === "IDLE" ? "#555" : "#fff";
                ctx.font = "bold 10px 'Segoe UI', sans-serif";
                ctx.textAlign = "center";
                ctx.textBaseline = "middle";
                ctx.fillText(`${name} (${entry.duration_ms})`, x + w / 2, y + h / 2);
            } else if (w > 14) {
                ctx.fillStyle = name === "IDLE" ? "#555" : "#fff";
                ctx.font = "bold 9px 'Segoe UI', sans-serif";
                ctx.textAlign = "center";
                ctx.textBaseline = "middle";
                ctx.fillText(`${entry.duration_ms}`, x + w / 2, y + h / 2);
            }
        }

        // Draw schedule blocks on PROCESS rows
        for (const entry of result.schedule) {
            const name = entry.process_name;
            if (name === "IDLE") continue;
            const pi = processNames.indexOf(name);
            if (pi === -1) continue;

            const procRowIdx = separatorAfter + pi;
            const x = LABEL_WIDTH + entry.time_ms * pxPerMs;
            const w = entry.duration_ms * pxPerMs;
            const y = rowY(procRowIdx) + padding;
            const h = ROW_HEIGHT - padding * 2;

            ctx.fillStyle = colorMap[name] || "#888";
            ctx.beginPath();
            roundRect(ctx, x, y, Math.max(w - 1, 1), h, 3);
            ctx.fill();

            // Show core number inside block on process rows
            if (w > 20) {
                ctx.fillStyle = "#fff";
                ctx.font = "bold 10px 'Segoe UI', sans-serif";
                ctx.textAlign = "center";
                ctx.textBaseline = "middle";
                ctx.fillText(`C${entry.core}`, x + w / 2, y + h / 2);
            }
        }

        // Draw period markers on process rows
        for (let pi = 0; pi < processNames.length; pi++) {
            const proc = processes[pi];
            const color = colorMap[proc.name];
            const procRowIdx = separatorAfter + pi;
            ctx.strokeStyle = color;
            ctx.globalAlpha = 0.35;
            ctx.lineWidth = 1;
            ctx.setLineDash([3, 3]);
            for (let t = proc.period_ms; t < duration; t += proc.period_ms) {
                const x = LABEL_WIDTH + t * pxPerMs;
                const y = rowY(procRowIdx);
                ctx.beginPath();
                ctx.moveTo(x, y);
                ctx.lineTo(x, y + ROW_HEIGHT);
                ctx.stroke();
            }
            ctx.setLineDash([]);
            ctx.globalAlpha = 1;
        }

        // Draw deadline miss markers on process rows
        for (const miss of result.deadline_misses) {
            const pi = processNames.indexOf(miss.process_name);
            if (pi === -1) continue;
            const procRowIdx = separatorAfter + pi;
            const x = LABEL_WIDTH + miss.deadline_ms * pxPerMs;
            const y = rowY(procRowIdx);

            ctx.strokeStyle = MISS_COLOR;
            ctx.lineWidth = 2;
            ctx.beginPath();
            ctx.moveTo(x, y + 2);
            ctx.lineTo(x, y + ROW_HEIGHT - 2);
            ctx.stroke();

            // Triangle marker
            ctx.fillStyle = MISS_COLOR;
            ctx.beginPath();
            ctx.moveTo(x - 5, y + 2);
            ctx.lineTo(x + 5, y + 2);
            ctx.lineTo(x, y + 10);
            ctx.closePath();
            ctx.fill();
        }

        // Build legend
        buildLegend(processNames, colorMap);

        // Setup tooltip (pass row info for both sections)
        setupTooltip(canvas, result, rows, separatorAfter, LABEL_WIDTH, TOP_MARGIN, ROW_HEIGHT, SEPARATOR_HEIGHT, pxPerMs);
    }

    function computeTickInterval(duration, chartWidth) {
        const targetTicks = Math.max(5, Math.min(30, chartWidth / 60));
        let interval = duration / targetTicks;
        const niceIntervals = [1, 2, 5, 10, 20, 25, 50, 100, 200, 500, 1000];
        for (const nice of niceIntervals) {
            if (nice >= interval) return nice;
        }
        return Math.ceil(interval / 1000) * 1000;
    }

    function roundRect(ctx, x, y, w, h, r) {
        ctx.moveTo(x + r, y);
        ctx.lineTo(x + w - r, y);
        ctx.quadraticCurveTo(x + w, y, x + w, y + r);
        ctx.lineTo(x + w, y + h - r);
        ctx.quadraticCurveTo(x + w, y + h, x + w - r, y + h);
        ctx.lineTo(x + r, y + h);
        ctx.quadraticCurveTo(x, y + h, x, y + h - r);
        ctx.lineTo(x, y + r);
        ctx.quadraticCurveTo(x, y, x + r, y);
    }

    function buildLegend(processNames, colorMap) {
        const legend = document.getElementById("legend");
        legend.innerHTML = "";
        for (const name of processNames) {
            const item = document.createElement("div");
            item.className = "legend-item";
            item.innerHTML = `<span class="legend-color" style="background:${colorMap[name]}"></span>${name}`;
            legend.appendChild(item);
        }
        // IDLE
        const idle = document.createElement("div");
        idle.className = "legend-item";
        idle.innerHTML = `<span class="legend-color" style="background:${IDLE_COLOR};border:1px solid #444"></span>IDLE`;
        legend.appendChild(idle);
        // Deadline miss
        const miss = document.createElement("div");
        miss.className = "legend-item";
        miss.innerHTML = `<span class="legend-color" style="background:${MISS_COLOR}"></span>Deadline Miss`;
        legend.appendChild(miss);
    }

    function setupTooltip(canvas, result, rows, separatorAfter, labelW, topM, rowH, sepH, pxPerMs) {
        const tooltip = document.getElementById("tooltip");

        // Abort previous listeners if any
        if (tooltipAbort) tooltipAbort.abort();
        tooltipAbort = new AbortController();
        const signal = tooltipAbort.signal;

        function getRowFromY(my) {
            // Check core section first
            const coreBottom = topM + separatorAfter * rowH;
            if (my >= topM && my < coreBottom) {
                return Math.floor((my - topM) / rowH);
            }
            // Process section (after separator)
            const procTop = coreBottom + sepH;
            if (my >= procTop) {
                const pi = Math.floor((my - procTop) / rowH);
                const idx = separatorAfter + pi;
                if (idx < rows.length) return idx;
            }
            return -1;
        }

        canvas.addEventListener("mousemove", (e) => {
            const rect = canvas.getBoundingClientRect();
            const mx = (e.clientX - rect.left);
            const my = (e.clientY - rect.top);

            const timeMs = (mx - labelW) / pxPerMs;
            const rowIdx = getRowFromY(my);

            if (timeMs < 0 || rowIdx < 0 || rowIdx >= rows.length) {
                tooltip.classList.add("hidden");
                return;
            }

            const row = rows[rowIdx];
            let entry = null;

            if (row.type === "core") {
                entry = result.schedule.find(e =>
                    e.core === row.index &&
                    timeMs >= e.time_ms &&
                    timeMs < e.time_ms + e.duration_ms
                );
            } else {
                const procName = row.label;
                entry = result.schedule.find(e =>
                    e.process_name === procName &&
                    timeMs >= e.time_ms &&
                    timeMs < e.time_ms + e.duration_ms
                );
            }

            if (entry) {
                tooltip.classList.remove("hidden");
                tooltip.innerHTML = `
                    <strong>${entry.process_name}</strong><br>
                    Core: ${entry.core}<br>
                    Start: ${entry.time_ms} ms<br>
                    Duration: ${entry.duration_ms} ms<br>
                    End: ${entry.time_ms + entry.duration_ms} ms
                `;
                tooltip.style.left = (e.clientX + 12) + "px";
                tooltip.style.top = (e.clientY + 12) + "px";
            } else {
                tooltip.classList.add("hidden");
            }
        }, { signal });

        canvas.addEventListener("mouseleave", () => {
            tooltip.classList.add("hidden");
        }, { signal });
    }

    // ---- Save / Load Config ----

    function buildConfig() {
        // Get ALL processes (including hidden ones) for saving
        const entries = document.querySelectorAll(".process-entry");
        const processes = [];
        for (const entry of entries) {
            const name = entry.querySelector(".proc-name").value.trim() || `P${entry.dataset.id}`;
            const period = parseInt(entry.querySelector(".proc-period").value, 10);
            const cpu = parseInt(entry.querySelector(".proc-cpu").value, 10);
            const color = entry.querySelector(".proc-color").value;
            const priority = parseInt(entry.querySelector(".proc-priority").value, 10) || 0;
            const pinChecked = entry.querySelector(".proc-pin-check").checked;
            const pinCore = parseInt(entry.querySelector(".proc-pin-core").value, 10);
            const pinned_core = pinChecked && !isNaN(pinCore) ? pinCore : null;
            const depsStr = entry.querySelector(".proc-deps").value.trim();
            const dependencies = depsStr ? depsStr.split(",").map(s => s.trim()).filter(Boolean) : [];
            if (isNaN(period) || isNaN(cpu) || period < 1 || cpu < 1) continue;
            processes.push({
                name,
                period_ms: period,
                cpu_time_ms: cpu,
                priority,
                pinned_core,
                color,
                dependencies: dependencies.length > 0 ? dependencies : undefined,
            });
        }

        const config = {
            tick_period_ms: parseInt(document.getElementById("tick-period").value, 10) || 1,
            simulation_duration_ms: parseInt(document.getElementById("sim-duration").value, 10) || 120,
            num_cores: parseInt(document.getElementById("num-cores").value, 10) || 1,
            fixed_partitioning: document.getElementById("fixed-partitioning").checked,
            processes,
        };

        // Include use cases if any
        if (useCases.length > 0) {
            config.use_cases = useCases;
        }

        return config;
    }

    const MCP_HTTP = "http://localhost:6590";

    async function saveConfig() {
        const config = buildConfig();
        // Add topology reference from localStorage
        const topoFile = localStorage.getItem('edf-topology-file');
        const topoName = localStorage.getItem('edf-topology-name');
        const topoVersion = localStorage.getItem('edf-topology-version');
        if (topoFile) config.topology_file = topoFile;
        if (topoName) config.topology_name = topoName;
        if (topoVersion) config.topology_version = topoVersion;

        // Determine scheduling filename — prompt if not yet named
        let schedFile = localStorage.getItem('edf-scheduling-file');
        if (!schedFile) {
            const defaultName = topoName || 'config';
            const schedName = prompt('Scheduling config name:', defaultName);
            if (schedName === null) return;
            const schedVersion = prompt('Scheduling version:', '1.0');
            if (schedVersion === null) return;
            const clean = (schedName.trim() || 'config').replace(/\s+/g, '-');
            const ver = schedVersion.trim() || '1.0';
            schedFile = `scheduling_${clean}_v${ver}.json`;
            config.scheduling_name = schedName.trim() || 'config';
            config.scheduling_version = ver;
            localStorage.setItem('edf-scheduling-file', schedFile);
        }

        const json = JSON.stringify(config, null, 2);
        // Save to both MCP and edf-server
        const saves = [];
        saves.push(
            fetch(`${MCP_HTTP}/files/${encodeURIComponent(schedFile)}`, {
                method: "POST",
                headers: { "Content-Type": "application/json" },
                body: json,
            }).catch(() => null)
        );
        saves.push(
            fetch(`/api/topologies/${encodeURIComponent(schedFile)}`, {
                method: "POST",
                headers: { "Content-Type": "application/json" },
                body: json,
            }).catch(() => null)
        );
        const results = await Promise.all(saves);
        if (results.some(r => r && r.ok)) {
            showToast(`Saved: ${schedFile}`);
            localStorage.setItem("edf-has-scheduler-config", "true");
            if (window.edfNavRefresh) window.edfNavRefresh();
        } else {
            // Both servers failed — download as file
            const blob = new Blob([json], { type: "application/json" });
            const url = URL.createObjectURL(blob);
            const a = document.createElement("a");
            a.href = url;
            a.download = schedFile;
            a.click();
            URL.revokeObjectURL(url);
            showToast("Config downloaded (servers unavailable)");
        }
    }

    function showToast(msg, duration) {
        duration = duration || 2500;
        let t = document.getElementById("edf-toast");
        if (!t) {
            t = document.createElement("div");
            t.id = "edf-toast";
            t.style.cssText = "position:fixed;bottom:24px;left:50%;transform:translateX(-50%);background:#16213e;color:#e0e0e0;border:1px solid #533483;border-radius:6px;padding:10px 20px;font-size:14px;z-index:9999;opacity:0;transition:opacity 0.3s;";
            document.body.appendChild(t);
        }
        t.textContent = msg;
        t.style.opacity = "1";
        clearTimeout(t._timer);
        t._timer = setTimeout(function () { t.style.opacity = "0"; }, duration);
    }

    function loadConfig(config) {
        document.getElementById("tick-period").value = config.tick_period_ms || 1;
        document.getElementById("sim-duration").value = config.simulation_duration_ms || 120;
        document.getElementById("num-cores").value = config.num_cores || 1;
        document.getElementById("fixed-partitioning").checked = !!config.fixed_partitioning;

        // Restore topology reference from scheduling config into localStorage
        if (config.topology_file) localStorage.setItem('edf-topology-file', config.topology_file);
        if (config.topology_name) localStorage.setItem('edf-topology-name', config.topology_name);
        if (config.topology_version) localStorage.setItem('edf-topology-version', config.topology_version);

        // Clear existing processes
        document.getElementById("process-list").innerHTML = "";
        processCounter = 0;

        // Store all processes
        allProcesses = config.processes || [];

        // Recreate processes from config
        if (allProcesses.length > 0) {
            for (const p of allProcesses) {
                createProcessEntry(
                    p.name,
                    p.period_ms,
                    p.cpu_time_ms,
                    p.color,
                    p.priority,
                    p.pinned_core,
                    p.dependencies
                );
            }
        }

        // Load use cases
        useCases = (config.use_cases || []).map(uc => ({
            name: uc.name,
            active: uc.active !== undefined ? uc.active : true,
            process_names: uc.process_names || [],
        }));
        renderUseCases();
        applyUseCaseFilter();
    }

    function handleLoadFile(e) {
        const file = e.target.files[0];
        if (!file) return;
        const reader = new FileReader();
        reader.onload = (ev) => {
            try {
                const config = JSON.parse(ev.target.result);
                loadConfig(config);
            } catch (err) {
                showError(`Invalid JSON file: ${err.message}`);
            }
        };
        reader.readAsText(file);
        // Reset input so the same file can be re-loaded
        e.target.value = "";
    }

    // ---- Init ----

    function init() {
        // Default processes matching the example
        createProcessEntry("Process-A", 10, 2);
        createProcessEntry("Process-B", 30, 10);
        createProcessEntry("Process-C", 60, 20);

        document.getElementById("add-process-btn").addEventListener("click", () => {
            createProcessEntry(`Process-${String.fromCharCode(65 + processCounter)}`, 20, 5);
        });

        document.getElementById("simulate-btn").addEventListener("click", runSimulation);
        document.getElementById("save-btn").addEventListener("click", saveConfig);

        // Load button: open modal with server file list
        const $loadOverlay = document.getElementById("load-modal-overlay");
        const $loadSelect = document.getElementById("load-server-select");
        const $loadInfo = document.getElementById("load-server-info");

        document.getElementById("load-btn").addEventListener("click", async () => {
            $loadSelect.innerHTML = '<option value="">Loading...</option>';
            $loadInfo.textContent = "";
            $loadOverlay.classList.remove("hidden");
            $loadOverlay.style.display = "flex";
            try {
                const resp = await fetch(`${MCP_HTTP}/files/`);
                if (!resp.ok) throw new Error(`HTTP ${resp.status}`);
                const data = await resp.json();
                $loadSelect.innerHTML = "";
                if (data.files.length === 0) {
                    $loadSelect.innerHTML = '<option value="">(no files)</option>';
                } else {
                    data.files.forEach(function (f) {
                        const opt = document.createElement("option");
                        opt.value = f.name;
                        opt.textContent = f.name + " (" + (f.size / 1024).toFixed(1) + " KB)";
                        $loadSelect.appendChild(opt);
                    });
                }
                $loadInfo.textContent = "Directory: " + data.dir;
            } catch {
                $loadSelect.innerHTML = '<option value="">(server unavailable)</option>';
                $loadInfo.textContent = 'Use "Local file..." to load from disk';
            }
        });

        function closeLoadModal() {
            $loadOverlay.classList.add("hidden");
            $loadOverlay.style.display = "none";
        }

        document.getElementById("btn-load-srv-cancel").addEventListener("click", closeLoadModal);
        $loadOverlay.addEventListener("click", function (e) {
            if (e.target === $loadOverlay) closeLoadModal();
        });

        document.getElementById("btn-load-srv-local").addEventListener("click", function () {
            closeLoadModal();
            document.getElementById("load-file-input").click();
        });

        document.getElementById("btn-load-srv-ok").addEventListener("click", async function () {
            const filename = $loadSelect.value;
            if (!filename) return;
            try {
                const resp = await fetch(`${MCP_HTTP}/files/${encodeURIComponent(filename)}`);
                if (!resp.ok) throw new Error(`HTTP ${resp.status}`);
                const config = await resp.json();
                loadConfig(config);
                closeLoadModal();
                showToast("Loaded: " + filename);
            } catch (err) {
                showError("Load failed: " + err.message);
            }
        });

        document.getElementById("load-file-input").addEventListener("change", handleLoadFile);

        // Allow Enter key to trigger simulation
        document.getElementById("config-panel").addEventListener("keydown", (e) => {
            if (e.key === "Enter") runSimulation();
        });

    }

    init();

    // ---- Load from topology file: generate processes from instances ----

    function loadFromTopology(topoData, params) {
        const instances = topoData.instances || [];
        const modules = topoData.modules || [];
        const connections = topoData.connections || [];
        const ucDefs = topoData.useCases || [];
        const chainConstraints = params.chains !== '0';

        // Clear scheduling file — we're starting fresh from a topology
        localStorage.removeItem('edf-scheduling-file');
        localStorage.removeItem('edf-has-scheduler-config');

        // Set scheduling params from URL or defaults
        document.getElementById("tick-period").value = params.tick || 1;
        document.getElementById("sim-duration").value = params.duration || 1000;
        document.getElementById("num-cores").value = params.cores || 1;
        document.getElementById("fixed-partitioning").checked = params.fixed !== '0';

        // Clear existing processes
        document.getElementById("process-list").innerHTML = "";
        processCounter = 0;

        // Build processes from topology instances
        const COLORS_LIST = [
            "#e94560", "#00b4d8", "#90be6d", "#f9c74f",
            "#f3722c", "#43aa8b", "#577590", "#f8961e",
            "#9b5de5", "#06d6a0", "#ef476f", "#118ab2",
        ];
        instances.forEach((inst, idx) => {
            const mod = modules.find(m => m.id === inst.moduleId);
            const wcet_us = mod ? mod.wcet_us : 0;
            const period_us = inst.activation === 'PERIODIC' ? inst.period_us :
                              inst.activation === 'SPORADIC' ? inst.min_interarrival_us : 10000;
            const period_ms = Math.max(1, Math.round(period_us / 1000));
            const cpu_ms = Math.max(1, Math.round(wcet_us / 1000));
            const color = COLORS_LIST[idx % COLORS_LIST.length];

            // Compute dependencies from connections
            let deps = [];
            if (chainConstraints) {
                const incomingConns = connections.filter(c => c.toInstanceId === inst.id);
                deps = incomingConns
                    .map(c => {
                        const fromInst = instances.find(i => i.id === c.fromInstanceId);
                        return fromInst ? fromInst.name : null;
                    })
                    .filter(Boolean);
            }

            createProcessEntry(inst.name, period_ms, cpu_ms, color, 0, null, deps);
        });

        // Load use cases
        useCases = ucDefs.map(uc => ({
            name: uc.name,
            active: uc.active !== undefined ? uc.active : true,
            process_names: uc.instanceIds
                .map(iid => instances.find(i => i.id === iid))
                .filter(Boolean)
                .map(i => i.name),
        }));
        allProcesses = [];
        renderUseCases();
        applyUseCaseFilter();
    }

    // ---- Auto-load from URL params or localStorage ----

    const urlParams = new URLSearchParams(window.location.search);
    const autoloadUrl = urlParams.get('autoload');
    const topologyParam = urlParams.get('topology');

    if (topologyParam) {
        // Load from topology file (from Builder "Launch Scheduler")
        fetch(`/api/topologies/${encodeURIComponent(topologyParam)}`)
            .then(resp => {
                if (!resp.ok) throw new Error(`HTTP ${resp.status}`);
                return resp.json();
            })
            .then(topoData => {
                loadFromTopology(topoData, {
                    tick: urlParams.get('tick'),
                    duration: urlParams.get('duration'),
                    cores: urlParams.get('cores'),
                    fixed: urlParams.get('fixed'),
                    chains: urlParams.get('chains'),
                });
                showToast(`Loaded topology: ${topoData.name || topologyParam}`);
                setTimeout(() => runSimulation(), 100);
            })
            .catch(err => {
                showError(`Failed to load topology: ${err.message}`);
            });
    } else if (autoloadUrl) {
        // Load from scheduling config URL (legacy autoload)
        fetch(autoloadUrl)
            .then(resp => {
                if (!resp.ok) throw new Error(`HTTP ${resp.status}`);
                return resp.json();
            })
            .then(config => {
                loadConfig(config);
                setTimeout(() => runSimulation(), 100);
            })
            .catch(err => {
                showError(`Auto-load failed: ${err.message}`);
            });
    } else {
        // No URL params: try restoring last scheduling config, then topology
        const savedSchedFile = localStorage.getItem('edf-scheduling-file');
        const savedTopoFile = localStorage.getItem('edf-topology-file');
        if (savedSchedFile) {
            fetch(`/api/topologies/${encodeURIComponent(savedSchedFile)}`)
                .then(resp => {
                    if (!resp.ok) throw new Error(`HTTP ${resp.status}`);
                    return resp.json();
                })
                .then(config => {
                    loadConfig(config);
                    showToast(`Restored: ${savedSchedFile}`);
                    setTimeout(() => runSimulation(), 100);
                })
                .catch(() => {
                    // Scheduling file not found — try loading from topology
                    if (savedTopoFile) {
                        fetch(`/api/topologies/${encodeURIComponent(savedTopoFile)}`)
                            .then(r => r.ok ? r.json() : Promise.reject())
                            .then(topoData => {
                                loadFromTopology(topoData, {});
                                showToast(`Loaded topology: ${topoData.name || savedTopoFile}`);
                                setTimeout(() => runSimulation(), 100);
                            })
                            .catch(() => {});
                    }
                });
        } else if (savedTopoFile) {
            // No scheduling config but topology exists — load processes from topology
            fetch(`/api/topologies/${encodeURIComponent(savedTopoFile)}`)
                .then(resp => {
                    if (!resp.ok) throw new Error(`HTTP ${resp.status}`);
                    return resp.json();
                })
                .then(topoData => {
                    loadFromTopology(topoData, {});
                    showToast(`Loaded topology: ${topoData.name || savedTopoFile}`);
                    setTimeout(() => runSimulation(), 100);
                })
                .catch(() => {});
        }
    }
})();
