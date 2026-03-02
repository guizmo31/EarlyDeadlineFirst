// Copyright (c) 2026 Ivan LE HIN
// Licensed under CC BY-NC-SA 4.0 — Non-commercial use only.
// For commercial licensing, contact the author.
// https://creativecommons.org/licenses/by-nc-sa/4.0/

(function () {
    "use strict";

    const MCP_HTTP = "http://localhost:6590";
    const CORE_COLORS = ["#e94560", "#00b4d8", "#90be6d", "#f9c74f", "#f3722c", "#43aa8b", "#577590", "#f8961e"];
    const IDLE_COLOR = "#ffffff";
    const MISS_COLOR = "#ff0040";

    // ---- State ----
    let viewerData = null;   // { config, result }
    let topology = null;     // { modules, instances, connections, useCases }
    let colorMap = {};       // process_name -> color
    let processMap = {};     // process_name -> process config object

    // Node elements per tab: { split: {name -> el}, full: {name -> el} }
    let nodeEls = { split: {}, full: {} };
    let activeTab = "split";
    let fullTopoRendered = false;

    // Playback
    let currentTime = 0;
    let playing = false;
    let looping = true;
    let speed = 1;
    let lastFrameTs = null;
    let totalDuration = 0;
    let playbackDurationS = 5; // seconds for one full loop at 1x

    // Gantt rendering state
    let ganttImg = null;     // offscreen canvas with static gantt
    const LABEL_W = 100;
    const TOP_M = 30;
    const ROW_H = 36;
    const SEP_H = 12;
    const MIN_PX = 4;
    let pxPerMs = MIN_PX;
    let chartW = 0;
    let chartH = 0;
    let numCores = 1;
    let separatorAfter = 0;
    let processOrder = [];   // ordered process names for rows

    // Topology rendering state (per tab)
    let topoState = {
        split: { zoom: 1, panX: 0, panY: 0 },
        full:  { zoom: 1, panX: 0, panY: 0 }
    };

    // FIFO state for multi-rate connections
    let fifoEdges = []; // { fromName, toName, depth, connId }
    let fifoFills = {}; // "fromName|toName" -> current fill count

    // ================================================================
    // DATA LOADING
    // ================================================================

    async function loadData() {
        // Load viewer-data.json
        try {
            const resp = await fetch(`${MCP_HTTP}/files/viewer-data.json`);
            if (!resp.ok) throw new Error(`HTTP ${resp.status}`);
            viewerData = await resp.json();
        } catch (err) {
            document.getElementById("header-info").textContent = "No viewer data: " + err.message;
            return;
        }

        // Load topology.json (optional)
        try {
            const resp = await fetch(`${MCP_HTTP}/files/topology.json`);
            if (resp.ok) {
                topology = await resp.json();
            }
        } catch { /* topology is optional */ }

        init();
    }

    // ================================================================
    // INIT
    // ================================================================

    function init() {
        const config = viewerData.config;
        const result = viewerData.result;
        totalDuration = result.total_duration_ms;
        numCores = result.num_cores;

        // Build color map from config processes
        (config.processes || []).forEach((p, i) => {
            colorMap[p.name] = p.color || CORE_COLORS[i % CORE_COLORS.length];
            processMap[p.name] = p;
        });

        // Header info
        document.getElementById("header-info").textContent =
            `Duration: ${totalDuration}ms | Cores: ${numCores} | Utilization: ${(result.cpu_utilization * 100).toFixed(1)}%`;

        // Setup scrub bar
        const scrub = document.getElementById("scrub");
        scrub.max = Math.round(totalDuration);

        const hasTopo = topology && topology.instances && topology.instances.length > 0;

        // Render topology in split tab
        if (hasTopo) {
            renderTopology("split", "topo-canvas", "topo-svg", "topo-viewport");
        } else {
            document.getElementById("topology-panel").classList.add("hidden-panel");
        }

        // Full tab topology will be rendered lazily on first tab switch

        renderGanttStatic();
        renderUseCases(config.use_cases || []);
        renderMetrics(result);
        setupControls();
        setupTabs();
        setupSplitDivider();

        // Hide full topology tab button if no topology
        if (!hasTopo) {
            const fullTabBtn = document.querySelector('.tab-btn[data-tab="topo-full"]');
            if (fullTabBtn) fullTabBtn.style.display = "none";
        }

        // Auto-play
        togglePlay();
    }

    // ================================================================
    // TAB SWITCHING
    // ================================================================

    function setupTabs() {
        document.querySelectorAll(".tab-btn").forEach(btn => {
            btn.addEventListener("click", function () {
                const tab = this.dataset.tab;
                switchTab(tab);
            });
        });
    }

    function switchTab(tab) {
        activeTab = tab;

        // Update tab buttons
        document.querySelectorAll(".tab-btn").forEach(b => b.classList.remove("active"));
        document.querySelector(`.tab-btn[data-tab="${tab}"]`).classList.add("active");

        // Show/hide content
        document.querySelectorAll(".tab-content").forEach(c => c.classList.remove("active"));
        document.getElementById("tab-" + tab).classList.add("active");

        // Render full topology lazily on first switch (needs to be visible for sizing)
        if (tab === "topo-full" && topology && topology.instances) {
            if (!fullTopoRendered) {
                fullTopoRendered = true;
                // Wait for layout to settle then render
                requestAnimationFrame(() => {
                    requestAnimationFrame(() => {
                        renderTopology("full", "topo-full-canvas", "topo-full-svg", "topo-full-viewport");
                        updateDisplay();
                    });
                });
                return;
            }
            requestAnimationFrame(() => {
                autoFitTopology("full", "topo-full-viewport", topology.instances);
                updateDisplay();
            });
            return;
        }

        updateDisplay();
    }

    // ================================================================
    // TOPOLOGY RENDERING
    // ================================================================

    function renderTopology(tabKey, canvasId, svgId, viewportId) {
        const $canvas = document.getElementById(canvasId);
        const $svg = document.getElementById(svgId);
        $canvas.innerHTML = "";
        $svg.innerHTML = "";
        $svg.classList.add("topo-svg-layer");

        const instances = topology.instances || [];
        const modules = topology.modules || [];
        const connections = topology.connections || [];
        const useCases = topology.useCases || [];

        // Render use case bounding boxes first (behind nodes)
        useCases.forEach(uc => {
            const memberInsts = uc.instanceIds
                .map(id => instances.find(i => i.id === id))
                .filter(Boolean);
            if (memberInsts.length === 0) return;

            let minX = Infinity, minY = Infinity, maxX = -Infinity, maxY = -Infinity;
            memberInsts.forEach(inst => {
                minX = Math.min(minX, inst.x);
                minY = Math.min(minY, inst.y);
                maxX = Math.max(maxX, inst.x + 160);
                maxY = Math.max(maxY, inst.y + 80);
            });
            const pad = 25;
            const div = document.createElement("div");
            div.className = "topo-usecase";
            div.style.left = (minX - pad) + "px";
            div.style.top = (minY - pad) + "px";
            div.style.width = (maxX - minX + pad * 2) + "px";
            div.style.height = (maxY - minY + pad * 2) + "px";
            div.innerHTML = `<span class="topo-usecase-label">${esc(uc.name)}</span>`;
            $canvas.appendChild(div);
        });

        // Render instance nodes
        nodeEls[tabKey] = {};
        instances.forEach(inst => {
            const mod = modules.find(m => m.id === inst.moduleId);
            const div = document.createElement("div");
            div.className = "topo-node";
            div.style.left = inst.x + "px";
            div.style.top = inst.y + "px";
            div.dataset.name = inst.name;

            const asil = mod ? mod.asil_level || "QM" : "QM";
            const asilClass = "asil-" + asil.toLowerCase();
            const inPorts = mod ? mod.input_ports || [] : [];
            const outPorts = mod ? mod.output_ports || [] : [];

            let portsHtml = '<div class="topo-node-body">';
            portsHtml += '<div class="topo-ports-col">';
            inPorts.forEach(p => {
                portsHtml += `<div class="topo-port input"><span class="topo-port-dot"></span><span>${esc(p.port_name)}</span></div>`;
            });
            portsHtml += '</div><div class="topo-ports-col">';
            outPorts.forEach(p => {
                portsHtml += `<div class="topo-port output"><span>${esc(p.port_name)}</span><span class="topo-port-dot"></span></div>`;
            });
            portsHtml += '</div></div>';

            div.innerHTML = `
                <div class="topo-node-header">
                    <span>${esc(inst.name)}</span>
                    <span class="asil-badge ${asilClass}">${asil}</span>
                </div>
                ${portsHtml}
            `;
            $canvas.appendChild(div);
            nodeEls[tabKey][inst.name] = div;
        });

        // Render connections (SVG bezier) + FIFO badges
        // Build FIFO edges on first render (split tab)
        if (tabKey === "split") fifoEdges = [];

        connections.forEach((conn, ci) => {
            const fromInst = instances.find(i => i.id === conn.fromInstanceId);
            const toInst = instances.find(i => i.id === conn.toInstanceId);
            if (!fromInst || !toInst) return;

            const fromMod = modules.find(m => m.id === fromInst.moduleId);
            const toMod = modules.find(m => m.id === toInst.moduleId);
            const outPorts = fromMod ? fromMod.output_ports || [] : [];
            const inPorts = toMod ? toMod.input_ports || [] : [];

            const fromEl = nodeEls[tabKey][fromInst.name];
            const toEl = nodeEls[tabKey][toInst.name];
            if (!fromEl || !toEl) return;

            const fw = fromEl.offsetWidth || 160;
            const fh = fromEl.offsetHeight || 60;
            const tw = toEl.offsetWidth || 160;
            const th = toEl.offsetHeight || 60;

            const outCount = outPorts.length || 1;
            const inCount = inPorts.length || 1;
            const portIdx = conn.fromPort || 0;
            const toPortIdx = conn.toPort || 0;

            const x1 = fromInst.x + fw;
            const y1 = fromInst.y + 28 + (portIdx + 0.5) * ((fh - 28) / outCount);
            const x2 = toInst.x;
            const y2 = toInst.y + 28 + (toPortIdx + 0.5) * ((th - 28) / inCount);

            const path = document.createElementNS("http://www.w3.org/2000/svg", "path");
            const dx = Math.abs(x2 - x1) * 0.5;
            path.setAttribute("d", `M${x1},${y1} C${x1 + dx},${y1} ${x2 - dx},${y2} ${x2},${y2}`);
            path.dataset.from = fromInst.name;
            path.dataset.to = toInst.name;
            $svg.appendChild(path);

            // Compute connection type and FIFO depth
            const fromProc = processMap[fromInst.name];
            const toProc = processMap[toInst.name];
            let depth = 1;
            let connType = 'same-rate'; // 'slow-consumer', 'fast-consumer', 'same-rate'
            if (fromProc && toProc && fromProc.period_ms > 0 && toProc.period_ms > 0) {
                if (toProc.period_ms > fromProc.period_ms) {
                    connType = 'slow-consumer';
                    depth = Math.round(toProc.period_ms / fromProc.period_ms);
                } else if (fromProc.period_ms > toProc.period_ms) {
                    connType = 'fast-consumer';
                }
            }

            // Check if this connection uses double buffering
            const isDB = toProc && toProc.double_buffer_deps &&
                         toProc.double_buffer_deps.includes(fromInst.name);

            // Show badge for: slow-consumer (FIFO), fast-consumer (S&H), or double-buffer
            if (connType !== 'same-rate' || isDB) {
                // Register edge for animation (once, on split render)
                if (tabKey === "split") {
                    fifoEdges.push({
                        fromName: fromInst.name, toName: toInst.name,
                        depth, connType, isDB, connIdx: ci
                    });
                }

                const midX = (x1 + x2) / 2;
                const midY = (y1 + y2) / 2;
                const g = document.createElementNS("http://www.w3.org/2000/svg", "g");
                g.setAttribute("class", "fifo-badge");
                g.dataset.from = fromInst.name;
                g.dataset.to = toInst.name;

                const color = connType === 'fast-consumer' ? '#4fc3f7' : '#f9a825';
                const rect = document.createElementNS("http://www.w3.org/2000/svg", "rect");
                rect.setAttribute("class", "fifo-badge-bg");
                rect.setAttribute("rx", "4");
                rect.setAttribute("ry", "4");
                rect.setAttribute("fill", "#1a1a2e");
                rect.setAttribute("stroke", color);
                rect.setAttribute("stroke-width", "1.5");

                const text = document.createElementNS("http://www.w3.org/2000/svg", "text");
                text.setAttribute("class", "fifo-badge-label");
                text.setAttribute("fill", color);
                text.setAttribute("font-size", "11");
                text.setAttribute("font-weight", "700");
                text.setAttribute("text-anchor", "middle");
                text.setAttribute("dominant-baseline", "central");
                text.setAttribute("x", midX);
                text.setAttribute("y", midY);

                // Initial label
                let label;
                if (connType === 'slow-consumer') label = `0/${depth}`;
                else if (connType === 'fast-consumer') label = 'S&H';
                else label = 'DB';
                if (isDB && connType !== 'same-rate') label += ' DB';
                text.textContent = label;

                const tw2 = Math.max(36, label.length * 8 + 12);
                rect.setAttribute("x", midX - tw2 / 2);
                rect.setAttribute("y", midY - 9);
                rect.setAttribute("width", tw2);
                rect.setAttribute("height", 18);

                g.appendChild(rect);
                g.appendChild(text);
                g.style.pointerEvents = "none";
                $svg.appendChild(g);
            }
        });

        // Auto-fit
        autoFitTopology(tabKey, viewportId, instances);
    }

    function autoFitTopology(tabKey, viewportId, instances) {
        if (instances.length === 0) return;
        const vp = document.getElementById(viewportId);
        const vpW = vp.clientWidth;
        const vpH = vp.clientHeight;
        if (vpW === 0 || vpH === 0) return; // not visible yet

        let minX = Infinity, minY = Infinity, maxX = -Infinity, maxY = -Infinity;
        instances.forEach(inst => {
            minX = Math.min(minX, inst.x);
            minY = Math.min(minY, inst.y);
            maxX = Math.max(maxX, inst.x + 180);
            maxY = Math.max(maxY, inst.y + 80);
        });

        const contentW = maxX - minX + 40;
        const contentH = maxY - minY + 40;
        const zoom = Math.max(0.2, Math.min(vpW / contentW, vpH / contentH, 1.5));
        const panX = (vpW - contentW * zoom) / 2 - minX * zoom + 20;
        const panY = (vpH - contentH * zoom) / 2 - minY * zoom + 20;

        topoState[tabKey] = { zoom, panX, panY };
        applyTopoTransform(tabKey);
    }

    function applyTopoTransform(tabKey) {
        const s = topoState[tabKey];
        const t = `translate(${s.panX}px, ${s.panY}px) scale(${s.zoom})`;
        if (tabKey === "split") {
            document.getElementById("topo-canvas").style.transform = t;
            document.getElementById("topo-svg").style.transform = t;
        } else {
            document.getElementById("topo-full-canvas").style.transform = t;
            document.getElementById("topo-full-svg").style.transform = t;
        }
    }

    // ================================================================
    // METRICS RENDERING
    // ================================================================

    function renderMetrics(result) {
        const pm = result.process_metrics || [];
        const cm = result.chain_metrics || [];
        const misses = result.deadline_misses || [];

        // Summary cards
        const summaryEl = document.getElementById("metrics-summary-content");
        const totalJobs = pm.reduce((s, p) => s + p.num_jobs, 0);
        const totalDone = pm.reduce((s, p) => s + p.num_completions, 0);
        const missCount = misses.length;
        const worstSlack = pm.filter(p => p.worst_slack_ms != null)
            .map(p => p.worst_slack_ms)
            .reduce((min, v) => Math.min(min, v), Infinity);
        const worstJitter = pm.map(p => p.jitter_ms).reduce((max, v) => Math.max(max, v), 0);

        summaryEl.innerHTML = `
            ${metricCard("CPU Utilization", (result.cpu_utilization * 100).toFixed(1) + "%",
                result.cpu_utilization <= 0.7 ? "good" : result.cpu_utilization <= 0.9 ? "warn" : "bad")}
            ${metricCard("Total Jobs", totalJobs, "good")}
            ${metricCard("Completed", totalDone, totalDone === totalJobs ? "good" : "warn")}
            ${metricCard("Deadline Misses", missCount, missCount === 0 ? "good" : "bad")}
            ${metricCard("Worst Slack", worstSlack === Infinity ? "N/A" : worstSlack + "ms",
                worstSlack > 0 ? "good" : worstSlack === 0 ? "warn" : "bad")}
            ${metricCard("Max Jitter", worstJitter + "ms",
                worstJitter <= 1 ? "good" : worstJitter <= 3 ? "warn" : "bad")}
        `;

        // Process table
        const tbody = document.querySelector("#metrics-process-table tbody");
        tbody.innerHTML = "";
        pm.forEach(p => {
            const color = colorMap[p.name] || "#888";
            const slackClass = p.worst_slack_ms == null ? "" :
                p.worst_slack_ms < 0 ? "slack-negative" :
                p.worst_slack_ms <= 2 ? "slack-tight" : "slack-ok";
            const tr = document.createElement("tr");
            tr.innerHTML = `
                <td class="metric-cell-name" style="color:${color}">${esc(p.name)}</td>
                <td>${p.period_ms}ms</td>
                <td>${p.cpu_time_ms}ms</td>
                <td>${p.num_jobs}</td>
                <td class="${p.num_completions === p.num_jobs ? '' : 'metric-cell-bad'}">${p.num_completions}</td>
                <td>${p.best_response_ms != null ? p.best_response_ms + "ms" : "-"}</td>
                <td>${p.worst_response_ms != null ? p.worst_response_ms + "ms" : "-"}</td>
                <td>${p.avg_response_ms ? p.avg_response_ms.toFixed(1) + "ms" : "-"}</td>
                <td>${p.jitter_ms}ms</td>
                <td class="${slackClass}">${p.best_slack_ms != null ? p.best_slack_ms + "ms" : "-"}</td>
                <td class="${slackClass}">${p.worst_slack_ms != null ? p.worst_slack_ms + "ms" : "-"}</td>
            `;
            tbody.appendChild(tr);
        });

        // Chain table
        const ctbody = document.querySelector("#metrics-chain-table tbody");
        ctbody.innerHTML = "";
        if (cm.length === 0) {
            const tr = document.createElement("tr");
            tr.innerHTML = '<td colspan="4" style="color:#8899aa">No dependency chains detected</td>';
            ctbody.appendChild(tr);
        } else {
            cm.forEach(c => {
                const tr = document.createElement("tr");
                const chainStr = c.chain.map(n => {
                    const color = colorMap[n] || "#888";
                    return `<span style="color:${color}">${esc(n)}</span>`;
                }).join(' → ');
                tr.innerHTML = `
                    <td>${chainStr}</td>
                    <td>${c.best_e2e_ms != null ? c.best_e2e_ms + "ms" : "-"}</td>
                    <td>${c.worst_e2e_ms != null ? c.worst_e2e_ms + "ms" : "-"}</td>
                    <td>${c.avg_e2e_ms ? c.avg_e2e_ms.toFixed(1) + "ms" : "-"}</td>
                `;
                ctbody.appendChild(tr);
            });
        }

        // Deadline misses
        const missEl = document.getElementById("metrics-misses-content");
        if (misses.length === 0) {
            missEl.innerHTML = '<div class="miss-none">No deadline misses — all jobs completed on time.</div>';
        } else {
            missEl.innerHTML = misses.map(m =>
                `<div class="miss-item">${esc(m.process_name)} missed deadline at ${m.deadline_ms}ms (${m.remaining_ms}ms remaining)</div>`
            ).join("");
        }
    }

    function metricCard(label, value, cls) {
        return `<div class="metric-card"><div class="metric-label">${label}</div><div class="metric-value ${cls || ''}">${value}</div></div>`;
    }

    // ================================================================
    // GANTT CHART (static render to offscreen canvas, cursor on main)
    // ================================================================

    function renderGanttStatic() {
        const result = viewerData.result;
        const schedule = result.schedule || [];
        const container = document.getElementById("gantt-container");

        // Determine process order
        const procSet = new Set();
        schedule.forEach(e => { if (e.process_name !== "IDLE") procSet.add(e.process_name); });
        processOrder = Array.from(procSet);
        const configOrder = (viewerData.config.processes || []).map(p => p.name);
        processOrder.sort((a, b) => {
            const ia = configOrder.indexOf(a);
            const ib = configOrder.indexOf(b);
            return (ia === -1 ? 999 : ia) - (ib === -1 ? 999 : ib);
        });

        separatorAfter = numCores;
        const totalRows = numCores + processOrder.length;
        chartW = Math.max(container.clientWidth - LABEL_W - 20, totalDuration * MIN_PX);
        pxPerMs = chartW / totalDuration;
        chartH = TOP_M + totalRows * ROW_H + SEP_H + 30;

        // Offscreen canvas for static content
        const off = document.createElement("canvas");
        off.width = LABEL_W + chartW + 20;
        off.height = chartH;
        const ctx = off.getContext("2d");

        // Background
        ctx.fillStyle = "#0f1e3d";
        ctx.fillRect(0, 0, off.width, off.height);

        // Draw grid
        ctx.strokeStyle = "#1a2e50";
        ctx.lineWidth = 0.5;
        const gridStep = pxPerMs >= 4 ? 1 : pxPerMs >= 2 ? 5 : 10;
        for (let t = 0; t <= totalDuration; t += gridStep) {
            const x = LABEL_W + t * pxPerMs;
            ctx.beginPath();
            ctx.moveTo(x, TOP_M);
            ctx.lineTo(x, chartH - 30);
            ctx.stroke();
        }

        // Time axis labels
        ctx.fillStyle = "#667788";
        ctx.font = "10px 'Segoe UI'";
        ctx.textAlign = "center";
        const labelStep = pxPerMs >= 2 ? 10 : 50;
        for (let t = 0; t <= totalDuration; t += labelStep) {
            ctx.fillText(t + "", LABEL_W + t * pxPerMs, chartH - 16);
        }

        // Row labels
        ctx.textAlign = "right";
        ctx.font = "bold 11px 'Segoe UI'";
        for (let c = 0; c < numCores; c++) {
            ctx.fillStyle = CORE_COLORS[c % CORE_COLORS.length];
            ctx.fillText("Core " + c, LABEL_W - 8, rowY(c) + ROW_H / 2 + 4);
        }
        ctx.font = "11px 'Segoe UI'";
        processOrder.forEach((name, i) => {
            ctx.fillStyle = colorMap[name] || "#aaa";
            ctx.fillText(name, LABEL_W - 8, rowY(numCores + i) + ROW_H / 2 + 4);
        });

        // Separator line
        const sepY = TOP_M + numCores * ROW_H + SEP_H / 2;
        ctx.strokeStyle = "#e94560";
        ctx.lineWidth = 1;
        ctx.setLineDash([4, 4]);
        ctx.beginPath();
        ctx.moveTo(LABEL_W, sepY);
        ctx.lineTo(LABEL_W + chartW, sepY);
        ctx.stroke();
        ctx.setLineDash([]);

        // Draw execution blocks
        const pad = 3;
        schedule.forEach(entry => {
            if (entry.process_name === "IDLE") return;
            const x = LABEL_W + entry.time_ms * pxPerMs;
            const w = entry.duration_ms * pxPerMs;
            const color = colorMap[entry.process_name] || "#888";

            // Core row
            const cy = rowY(entry.core) + pad;
            drawBlock(ctx, x, cy, w, ROW_H - pad * 2, color, entry.process_name, entry.duration_ms);

            // Process row
            const pi = processOrder.indexOf(entry.process_name);
            if (pi !== -1) {
                const py = rowY(numCores + pi) + pad;
                drawBlock(ctx, x, py, w, ROW_H - pad * 2, color, "C" + entry.core, entry.duration_ms);
            }
        });

        // Period markers for process rows
        ctx.setLineDash([3, 3]);
        ctx.lineWidth = 0.5;
        processOrder.forEach((name, i) => {
            const proc = processMap[name];
            if (!proc) return;
            ctx.strokeStyle = colorMap[name] || "#888";
            for (let t = proc.period_ms; t < totalDuration; t += proc.period_ms) {
                const x = LABEL_W + t * pxPerMs;
                const y = rowY(numCores + i);
                ctx.beginPath();
                ctx.moveTo(x, y);
                ctx.lineTo(x, y + ROW_H);
                ctx.stroke();
            }
        });
        ctx.setLineDash([]);

        // Deadline misses
        (result.deadline_misses || []).forEach(miss => {
            const pi = processOrder.indexOf(miss.process_name);
            if (pi === -1) return;
            const x = LABEL_W + miss.deadline_ms * pxPerMs;
            const y = rowY(numCores + pi);
            ctx.strokeStyle = MISS_COLOR;
            ctx.lineWidth = 2;
            ctx.beginPath();
            ctx.moveTo(x, y);
            ctx.lineTo(x, y + ROW_H);
            ctx.stroke();
            ctx.fillStyle = MISS_COLOR;
            ctx.beginPath();
            ctx.moveTo(x, y);
            ctx.lineTo(x - 4, y - 6);
            ctx.lineTo(x + 4, y - 6);
            ctx.closePath();
            ctx.fill();
        });

        ganttImg = off;

        // Set main canvas size
        const mainCanvas = document.getElementById("gantt-canvas");
        mainCanvas.width = off.width;
        mainCanvas.height = off.height;

        drawGanttFrame();
    }

    function rowY(idx) {
        let y = TOP_M + idx * ROW_H;
        if (idx >= separatorAfter) y += SEP_H;
        return y;
    }

    function drawBlock(ctx, x, y, w, h, color, label, dur) {
        const r = Math.min(4, w / 2);
        ctx.fillStyle = color;
        ctx.beginPath();
        ctx.moveTo(x + r, y);
        ctx.lineTo(x + w - r, y);
        ctx.quadraticCurveTo(x + w, y, x + w, y + r);
        ctx.lineTo(x + w, y + h - r);
        ctx.quadraticCurveTo(x + w, y + h, x + w - r, y + h);
        ctx.lineTo(x + r, y + h);
        ctx.quadraticCurveTo(x, y + h, x, y + h - r);
        ctx.lineTo(x, y + r);
        ctx.quadraticCurveTo(x, y, x + r, y);
        ctx.closePath();
        ctx.fill();

        if (w > 14) {
            ctx.fillStyle = "#000";
            ctx.font = "bold 10px 'Segoe UI'";
            ctx.textAlign = "center";
            const text = w > 40 ? label + " (" + dur + ")" : dur + "";
            ctx.fillText(text, x + w / 2, y + h / 2 + 3, w - 4);
        }
    }

    function drawGanttFrame() {
        const canvas = document.getElementById("gantt-canvas");
        const ctx = canvas.getContext("2d");

        ctx.drawImage(ganttImg, 0, 0);

        // Draw time cursor
        const cursorX = LABEL_W + currentTime * pxPerMs;
        ctx.strokeStyle = "#e94560";
        ctx.lineWidth = 2;
        ctx.beginPath();
        ctx.moveTo(cursorX, TOP_M);
        ctx.lineTo(cursorX, chartH - 30);
        ctx.stroke();

        // Cursor head triangle
        ctx.fillStyle = "#e94560";
        ctx.beginPath();
        ctx.moveTo(cursorX, TOP_M);
        ctx.lineTo(cursorX - 5, TOP_M - 8);
        ctx.lineTo(cursorX + 5, TOP_M - 8);
        ctx.closePath();
        ctx.fill();
    }

    // ================================================================
    // USE CASES
    // ================================================================

    function renderUseCases(useCases) {
        const container = document.getElementById("controls-usecases");
        container.innerHTML = "";
        if (!useCases || useCases.length === 0) return;

        useCases.forEach((uc, i) => {
            const label = document.createElement("label");
            const cb = document.createElement("input");
            cb.type = "checkbox";
            cb.checked = uc.active !== false;
            cb.addEventListener("change", () => {});
            label.appendChild(cb);
            label.appendChild(document.createTextNode(" " + uc.name));
            container.appendChild(label);
        });
    }

    // ================================================================
    // PLAYBACK ENGINE
    // ================================================================

    function togglePlay() {
        playing = !playing;
        const btn = document.getElementById("btn-play");
        btn.innerHTML = playing ? "&#10074;&#10074;" : "&#9654;";
        btn.classList.toggle("active", playing);
        if (playing) {
            lastFrameTs = null;
            requestAnimationFrame(tick);
        }
    }

    function stopPlayback() {
        playing = false;
        currentTime = 0;
        lastFrameTs = null;
        document.getElementById("btn-play").innerHTML = "&#9654;";
        document.getElementById("btn-play").classList.remove("active");
        updateDisplay();
    }

    function tick(now) {
        if (!playing) return;
        if (lastFrameTs !== null) {
            const deltaMs = now - lastFrameTs;
            const baseRate = totalDuration / (playbackDurationS * 1000); // sim ms per real ms at 1x
            currentTime += deltaMs * baseRate * speed;

            if (currentTime >= totalDuration) {
                if (looping) {
                    currentTime = currentTime % totalDuration;
                } else {
                    currentTime = totalDuration;
                    playing = false;
                    document.getElementById("btn-play").innerHTML = "&#9654;";
                    document.getElementById("btn-play").classList.remove("active");
                }
            }
        }
        lastFrameTs = now;
        updateDisplay();
        if (playing) requestAnimationFrame(tick);
    }

    function updateDisplay() {
        // Update scrub bar
        document.getElementById("scrub").value = Math.round(currentTime);
        document.getElementById("time-display").textContent =
            Math.round(currentTime) + " / " + totalDuration + " ms";

        // Redraw Gantt with cursor (only if split tab visible)
        if (activeTab === "split") {
            drawGanttFrame();

            // Auto-scroll Gantt to keep cursor visible
            const container = document.getElementById("gantt-container");
            const cursorX = LABEL_W + currentTime * pxPerMs;
            const scrollLeft = container.scrollLeft;
            const visibleW = container.clientWidth;
            if (cursorX < scrollLeft + LABEL_W || cursorX > scrollLeft + visibleW - 20) {
                container.scrollLeft = Math.max(0, cursorX - visibleW / 3);
            }
        }

        // Update topology highlights in both tabs
        updateTopologyHighlights();
    }

    function computeFifoFills(timeMs) {
        // Replay schedule completions up to timeMs to compute FIFO fill levels
        if (fifoEdges.length === 0) return;
        const schedule = viewerData.result.schedule || [];

        // Reset fills — double-buffered edges start pre-filled
        fifoFills = {};
        fifoEdges.forEach(e => {
            fifoFills[e.fromName + "|" + e.toName] = e.isDB ? e.depth : 0;
        });

        // Build a map of which FIFO edges each process is a producer or consumer of
        const producerEdges = {}; // processName -> [fifoEdge]
        const consumerEdges = {}; // processName -> [fifoEdge]
        fifoEdges.forEach(e => {
            if (!producerEdges[e.fromName]) producerEdges[e.fromName] = [];
            producerEdges[e.fromName].push(e);
            if (!consumerEdges[e.toName]) consumerEdges[e.toName] = [];
            consumerEdges[e.toName].push(e);
        });

        // Track per-process accumulated CPU time within each period
        const procPeriods = {};
        fifoEdges.forEach(e => {
            if (processMap[e.fromName]) procPeriods[e.fromName] = processMap[e.fromName];
            if (processMap[e.toName]) procPeriods[e.toName] = processMap[e.toName];
        });

        const jobAccum = {};
        Object.keys(procPeriods).forEach(name => {
            const p = procPeriods[name];
            jobAccum[name] = { accumulated: 0, nextRelease: 0, period: p.period_ms, cpuTime: p.cpu_time_ms };
        });

        const sortedEntries = schedule
            .filter(e => e.process_name !== "IDLE" && procPeriods[e.process_name])
            .sort((a, b) => a.time_ms - b.time_ms || (a.time_ms + a.duration_ms) - (b.time_ms + b.duration_ms));

        for (const entry of sortedEntries) {
            const endT = entry.time_ms + entry.duration_ms;
            if (endT > timeMs) break;

            const name = entry.process_name;
            const ja = jobAccum[name];
            if (!ja) continue;

            while (ja.nextRelease + ja.period <= entry.time_ms) {
                ja.nextRelease += ja.period;
                ja.accumulated = 0;
            }

            ja.accumulated += entry.duration_ms;

            if (ja.accumulated >= ja.cpuTime) {
                // Producer completed: increment FIFO counters for consumers
                if (producerEdges[name]) {
                    producerEdges[name].forEach(fe => {
                        const key = fe.fromName + "|" + fe.toName;
                        fifoFills[key] = Math.min(fifoFills[key] + 1, fe.depth);
                    });
                }

                // Consumer completed: consume FIFO based on connection type
                if (consumerEdges[name]) {
                    consumerEdges[name].forEach(fe => {
                        const key = fe.fromName + "|" + fe.toName;
                        if (fe.connType === 'fast-consumer') {
                            // Sample-and-hold: don't consume, counter stays >= 1
                        } else {
                            // Same-rate or slow consumer: consume by depth
                            fifoFills[key] = Math.max(0, fifoFills[key] - fe.depth);
                        }
                    });
                }

                ja.accumulated = 0;
                ja.nextRelease += ja.period;
            }
        }
    }

    function updateFifoBadges() {
        if (fifoEdges.length === 0) return;
        ["split", "full"].forEach(tabKey => {
            const svgId = tabKey === "split" ? "topo-svg" : "topo-full-svg";
            const badges = document.querySelectorAll(`#${svgId} .fifo-badge`);
            badges.forEach(g => {
                const from = g.dataset.from;
                const to = g.dataset.to;
                const key = from + "|" + to;
                const edge = fifoEdges.find(e => e.fromName === from && e.toName === to);
                if (!edge) return;
                const fill = fifoFills[key] || 0;
                const depth = edge.depth;

                const text = g.querySelector("text");
                const rect = g.querySelector("rect");

                if (edge.connType === 'slow-consumer') {
                    // Show fill/depth with color gradient
                    const ratio = fill / depth;
                    const dbSuffix = edge.isDB ? ' DB' : '';
                    if (text) text.textContent = `${fill}/${depth}${dbSuffix}`;
                    let color;
                    if (ratio >= 1) color = "#90be6d"; // green = full/ready
                    else if (ratio >= 0.5) color = "#f9c74f"; // yellow
                    else color = "#f9a825"; // orange = low
                    if (text) text.setAttribute("fill", color);
                    if (rect) rect.setAttribute("stroke", color);
                } else if (edge.connType === 'fast-consumer') {
                    // S&H: show state (waiting / active)
                    const dbSuffix = edge.isDB ? ' DB' : '';
                    const active = fill >= 1;
                    const label = active ? `S&H \u2713${dbSuffix}` : `S&H \u2026${dbSuffix}`;
                    const color = active ? "#90be6d" : "#4fc3f7";
                    if (text) { text.textContent = label; text.setAttribute("fill", color); }
                    if (rect) rect.setAttribute("stroke", color);
                } else {
                    // Same-rate with DB
                    const active = fill >= 1;
                    const label = active ? "DB \u2713" : "DB \u2026";
                    const color = active ? "#90be6d" : "#4fc3f7";
                    if (text) { text.textContent = label; text.setAttribute("fill", color); }
                    if (rect) rect.setAttribute("stroke", color);
                }

                // Resize badge rect to fit label
                if (text && rect) {
                    const label = text.textContent;
                    const tw = Math.max(36, label.length * 8 + 12);
                    const midX = parseFloat(text.getAttribute("x"));
                    rect.setAttribute("x", midX - tw / 2);
                    rect.setAttribute("width", tw);
                }
            });
        });
    }

    function updateTopologyHighlights() {
        if (!topology) return;
        const schedule = viewerData.result.schedule || [];

        // Find executing entries at currentTime
        const executing = {};
        schedule.forEach(e => {
            if (e.process_name !== "IDLE" &&
                e.time_ms <= currentTime &&
                currentTime < e.time_ms + e.duration_ms) {
                executing[e.process_name] = e.core;
            }
        });

        const isPlaying = playing || currentTime > 0;

        // Update nodes in both tabs
        ["split", "full"].forEach(tabKey => {
            const els = nodeEls[tabKey];
            Object.keys(els).forEach(name => {
                const el = els[name];
                // Remove all core classes
                for (let c = 0; c < 8; c++) el.classList.remove("core-" + c);
                el.classList.remove("executing");
                el.classList.remove("dimmed");

                if (name in executing) {
                    el.classList.add("executing");
                    el.classList.add("core-" + executing[name]);
                } else if (isPlaying) {
                    el.classList.add("dimmed");
                }
            });

            // Update connection highlights
            const svgId = tabKey === "split" ? "topo-svg" : "topo-full-svg";
            const svgPaths = document.querySelectorAll(`#${svgId} path`);
            svgPaths.forEach(path => {
                const from = path.dataset.from;
                const to = path.dataset.to;
                const active = (from in executing) || (to in executing);
                path.classList.toggle("conn-active", active);
                path.classList.toggle("conn-dimmed", !active && isPlaying);
            });
        });

        // Update FIFO badges
        computeFifoFills(currentTime);
        updateFifoBadges();
    }

    // ================================================================
    // CONTROLS
    // ================================================================

    function setupControls() {
        document.getElementById("btn-play").addEventListener("click", togglePlay);
        document.getElementById("btn-stop").addEventListener("click", stopPlayback);

        // Loop toggle
        document.getElementById("btn-loop").addEventListener("click", function () {
            looping = !looping;
            this.classList.toggle("active", looping);
        });

        // Speed buttons
        document.querySelectorAll(".speed-btn").forEach(btn => {
            btn.addEventListener("click", function () {
                speed = parseFloat(this.dataset.speed);
                document.querySelectorAll(".speed-btn").forEach(b => b.classList.remove("active"));
                this.classList.add("active");
            });
        });

        // Scrub bar
        const scrub = document.getElementById("scrub");
        scrub.addEventListener("input", function () {
            currentTime = parseFloat(this.value);
            lastFrameTs = null;
            updateDisplay();
        });
        scrub.addEventListener("mousedown", () => { playing = false; });
        scrub.addEventListener("mouseup", () => {
            lastFrameTs = null;
            togglePlay();
        });

        // Total duration (simulation duration, editable)
        const totalDurInput = document.getElementById("total-duration");
        totalDurInput.value = totalDuration;
        totalDurInput.addEventListener("change", function () {
            const v = parseInt(this.value);
            if (v > 0) {
                totalDuration = v;
                document.getElementById("scrub").max = totalDuration;
                if (currentTime > totalDuration) currentTime = 0;
                renderGanttStatic();
                lastFrameTs = null;
                updateDisplay();
            }
        });

        // Cycle duration (how long one full loop takes at 1x)
        const cycleInput = document.getElementById("cycle-duration");
        cycleInput.value = playbackDurationS;
        cycleInput.addEventListener("change", function () {
            const v = parseFloat(this.value);
            if (v > 0) {
                playbackDurationS = v;
                lastFrameTs = null;
            }
        });

        // Keyboard shortcuts
        document.addEventListener("keydown", (e) => {
            if (e.code === "Space") { e.preventDefault(); togglePlay(); }
            if (e.code === "KeyL") { looping = !looping; document.getElementById("btn-loop").classList.toggle("active", looping); }
            if (e.code === "Home") { currentTime = 0; updateDisplay(); }
            if (e.code === "End") { currentTime = totalDuration; updateDisplay(); }
        });
    }

    // ================================================================
    // SPLIT DIVIDER (resizable)
    // ================================================================

    function setupSplitDivider() {
        const divider = document.getElementById("split-divider");
        const topoPanel = document.getElementById("topology-panel");
        let dragging = false;

        divider.addEventListener("mousedown", (e) => {
            dragging = true;
            e.preventDefault();
        });

        document.addEventListener("mousemove", (e) => {
            if (!dragging) return;
            const mainRect = document.getElementById("tab-split").getBoundingClientRect();
            const pct = ((e.clientX - mainRect.left) / mainRect.width) * 100;
            const clamped = Math.max(15, Math.min(80, pct));
            topoPanel.style.width = clamped + "%";
        });

        document.addEventListener("mouseup", () => {
            if (dragging) {
                dragging = false;
                renderGanttStatic();
            }
        });
    }

    // ================================================================
    // UTILS
    // ================================================================

    function esc(str) {
        const d = document.createElement("div");
        d.textContent = str || "";
        return d.innerHTML;
    }

    // ================================================================
    // START
    // ================================================================

    loadData();
})();
