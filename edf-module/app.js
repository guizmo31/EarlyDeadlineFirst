// Copyright (c) 2026 Ivan LE HIN
// Licensed under CC BY-NC-SA 4.0 — Non-commercial use only.
// For commercial licensing, contact the author.
// https://creativecommons.org/licenses/by-nc-sa/4.0/

(function () {
    "use strict";

    let allModules = [];
    let selectedModule = null;
    let currentMeta = null;
    let currentSourceCode = null;
    let simLoopTimer = null;
    let isEditing = false;

    // ---- Load modules from API ----

    async function loadModules() {
        try {
            const resp = await fetch("/api/modules");
            if (!resp.ok) throw new Error(`HTTP ${resp.status}`);
            allModules = await resp.json();
            populateCategoryFilter();
            renderModuleList();
        } catch (err) {
            document.getElementById("module-list").innerHTML =
                `<p style="color:#e94560;font-size:0.8rem;">Failed to load modules: ${err.message}</p>`;
        }
    }

    function populateCategoryFilter() {
        const select = document.getElementById("category-filter");
        const categories = [...new Set(allModules.map(m => m.category))].sort();
        select.innerHTML = '<option value="">All Categories</option>';
        categories.forEach(cat => {
            const opt = document.createElement("option");
            opt.value = cat;
            opt.textContent = cat;
            select.appendChild(opt);
        });
    }

    function getFilteredModules() {
        const search = document.getElementById("search-input").value.toLowerCase();
        const category = document.getElementById("category-filter").value;
        return allModules.filter(m => {
            if (category && m.category !== category) return false;
            if (search && !m.name.toLowerCase().includes(search) &&
                !m.description.toLowerCase().includes(search) &&
                !m.category.toLowerCase().includes(search)) return false;
            return true;
        });
    }

    function renderModuleList() {
        const list = document.getElementById("module-list");
        const filtered = getFilteredModules();
        list.innerHTML = "";

        const groups = {};
        filtered.forEach(m => {
            if (!groups[m.category]) groups[m.category] = [];
            groups[m.category].push(m);
        });

        const sortedCategories = Object.keys(groups).sort();
        sortedCategories.forEach(cat => {
            const header = document.createElement("div");
            header.className = "category-header";
            header.textContent = cat;
            list.appendChild(header);

            groups[cat].forEach(m => {
                const item = document.createElement("div");
                item.className = "module-item" + (selectedModule === m.name ? " selected" : "");
                item.innerHTML = `
                    <div class="module-item-name">${m.name}</div>
                    <div class="module-item-category">${m.scheduling_type} &middot; v${m.version}</div>
                `;
                item.addEventListener("click", () => selectModule(m.name));
                list.appendChild(item);
            });
        });

        if (filtered.length === 0) {
            list.innerHTML = '<p style="color:#666;font-size:0.8rem;padding:12px;">No modules found.</p>';
        }
    }

    // ---- Select and display module details ----

    async function selectModule(name) {
        selectedModule = name;
        renderModuleList();

        try {
            const resp = await fetch(`/api/modules/${encodeURIComponent(name)}`);
            if (!resp.ok) throw new Error(`HTTP ${resp.status}`);
            const data = await resp.json();
            renderDetail(data.metadata, data.source_code);
        } catch (err) {
            document.getElementById("detail-placeholder").innerHTML =
                `<p style="color:#e94560;">Error loading module: ${err.message}</p>`;
        }
    }

    function renderDetail(meta, sourceCode) {
        document.getElementById("detail-placeholder").classList.add("hidden");
        document.getElementById("detail-content").classList.remove("hidden");
        currentMeta = meta;
        currentSourceCode = sourceCode || "";

        renderOverview(meta);
        try { renderSource(meta, sourceCode); } catch (e) { console.warn("renderSource error:", e); }
        renderApi(meta);
        renderSimulation(meta);
    }

    function asilBadgeClass(level) {
        const map = {
            "QM": "badge-asil-qm",
            "ASIL-A": "badge-asil-a",
            "ASIL-B": "badge-asil-b",
            "ASIL-C": "badge-asil-c",
            "ASIL-D": "badge-asil-d",
        };
        return map[level] || "badge-asil-qm";
    }

    function renderOverview(meta) {
        document.getElementById("meta-header").innerHTML = `
            <div class="meta-title">${meta.name}</div>
            <div class="meta-version">Version ${meta.version}</div>
            <div class="meta-description">${meta.description}</div>
            <div class="meta-badges">
                <span class="badge badge-category">${meta.category}</span>
                <span class="badge badge-scheduling">${meta.scheduling_type}</span>
                <span class="badge ${asilBadgeClass(meta.asil_level)}">${meta.asil_level}</span>
            </div>
        `;

        let portsHtml = "";
        if (meta.input_ports.length > 0) {
            portsHtml += `<div class="section-title">Input Ports</div>
            <table class="port-table"><thead><tr><th>Port</th><th>Type</th><th>Size</th><th>Description</th></tr></thead><tbody>`;
            meta.input_ports.forEach(p => {
                portsHtml += `<tr><td>${p.port_name}</td><td><code>${p.data_type}</code></td><td>${p.sample_size_bytes} B</td><td>${p.description}</td></tr>`;
            });
            portsHtml += `</tbody></table>`;
        }
        if (meta.output_ports.length > 0) {
            portsHtml += `<div class="section-title">Output Ports</div>
            <table class="port-table"><thead><tr><th>Port</th><th>Type</th><th>Size</th><th>Description</th></tr></thead><tbody>`;
            meta.output_ports.forEach(p => {
                portsHtml += `<tr><td>${p.port_name}</td><td><code>${p.data_type}</code></td><td>${p.sample_size_bytes} B</td><td>${p.description}</td></tr>`;
            });
            portsHtml += `</tbody></table>`;
        }
        document.getElementById("meta-ports").innerHTML = portsHtml;

        let configHtml = "";
        if (meta.config_params && meta.config_params.length > 0) {
            configHtml = `<div class="section-title">Configuration Parameters</div>
            <table class="port-table"><thead><tr><th>Name</th><th>Type</th><th>Default</th><th>Description</th></tr></thead><tbody>`;
            meta.config_params.forEach(p => {
                const def = typeof p.default_value === "object" ? JSON.stringify(p.default_value) : p.default_value;
                configHtml += `<tr><td><code>${p.name}</code></td><td>${p.data_type}</td><td><code>${def}</code></td><td>${p.description}</td></tr>`;
            });
            configHtml += `</tbody></table>`;
        }
        document.getElementById("meta-config").innerHTML = configHtml;

        const t = meta.timing;
        document.getElementById("meta-timing").innerHTML = `
            <div class="section-title">Timing</div>
            <div class="info-grid">
                <div class="info-card">
                    <div class="info-card-label">WCET</div>
                    <div class="info-card-value">${t.wcet_us} &micro;s</div>
                </div>
                <div class="info-card">
                    <div class="info-card-label">BCET</div>
                    <div class="info-card-value">${t.bcet_us} &micro;s</div>
                </div>
                <div class="info-card">
                    <div class="info-card-label">Typical</div>
                    <div class="info-card-value">${t.typical_us} &micro;s</div>
                </div>
            </div>
        `;

        const r = meta.resources;
        document.getElementById("meta-resources").innerHTML = `
            <div class="section-title">Resources</div>
            <div class="info-grid">
                <div class="info-card">
                    <div class="info-card-label">Stack</div>
                    <div class="info-card-value">${formatBytes(r.stack_size_bytes)}</div>
                </div>
                <div class="info-card">
                    <div class="info-card-label">Static Memory</div>
                    <div class="info-card-value">${formatBytes(r.static_mem_bytes)}</div>
                </div>
                <div class="info-card">
                    <div class="info-card-label">FPU</div>
                    <div class="info-card-value">${r.requires_fpu ? "Required" : "No"}</div>
                </div>
                <div class="info-card">
                    <div class="info-card-label">GPU</div>
                    <div class="info-card-value">${r.requires_gpu ? "Required" : "No"}</div>
                </div>
            </div>
        `;
    }

    // ---- Source Code tab with Edit support ----

    function renderSource(meta, sourceCode) {
        const snakeName = toSnakeCase(meta.name);
        document.getElementById("source-filename").textContent = `modules/${snakeName}/src/lib.rs`;

        // Reset to view mode
        exitEditMode();

        const codeEl = document.getElementById("source-code");
        if (sourceCode) {
            codeEl.textContent = sourceCode;
            codeEl.removeAttribute("data-highlighted");
            hljs.highlightElement(codeEl);
            document.getElementById("btn-edit-source").disabled = false;
        } else {
            codeEl.textContent = "// Source code not available.\n// The module file may not exist on disk.";
            codeEl.removeAttribute("data-highlighted");
            hljs.highlightElement(codeEl);
            document.getElementById("btn-edit-source").disabled = true;
        }
    }

    function enterEditMode() {
        isEditing = true;
        const codeEl = document.getElementById("source-code");
        const preEl = document.getElementById("source-view");

        // Enable contenteditable on the highlighted code block
        codeEl.contentEditable = "true";
        codeEl.spellcheck = false;
        preEl.classList.add("editing");

        document.getElementById("btn-edit-source").classList.add("hidden");
        document.getElementById("btn-save-verify").classList.remove("hidden");
        document.getElementById("btn-cancel-edit").classList.remove("hidden");
        document.getElementById("verify-output").classList.add("hidden");
        setVerifyStatus("", "");
        codeEl.focus();
    }

    function exitEditMode() {
        isEditing = false;
        const codeEl = document.getElementById("source-code");
        const preEl = document.getElementById("source-view");

        codeEl.contentEditable = "false";
        preEl.classList.remove("editing");

        document.getElementById("btn-edit-source").classList.remove("hidden");
        document.getElementById("btn-save-verify").classList.add("hidden");
        document.getElementById("btn-cancel-edit").classList.add("hidden");
    }

    function setVerifyStatus(text, type) {
        const el = document.getElementById("verify-status");
        if (!text) {
            el.classList.add("hidden");
            return;
        }
        el.classList.remove("hidden");
        el.textContent = text;
        el.className = "status-" + type;
    }

    async function doSaveAndVerify() {
        if (!selectedModule) return;
        const source = document.getElementById("source-code").textContent;
        const saveBtn = document.getElementById("btn-save-verify");
        const outputDiv = document.getElementById("verify-output");

        saveBtn.disabled = true;
        saveBtn.textContent = "Compiling...";
        setVerifyStatus("Compiling...", "progress");
        outputDiv.classList.remove("hidden");
        outputDiv.className = "";
        outputDiv.textContent = "Saving source and running cargo build...";

        try {
            const resp = await fetch(`/api/modules/${encodeURIComponent(selectedModule)}/save-and-verify`, {
                method: "POST",
                headers: { "Content-Type": "application/json" },
                body: JSON.stringify({ source }),
            });
            const data = await resp.json();

            if (data.status === "verified") {
                setVerifyStatus("Verified", "ok");
                outputDiv.className = "output-ok";

                let msg = "Compilation successful. Plugin reloaded.";
                if (data.warnings && data.warnings.length > 0) {
                    msg += "\n\nWarnings:\n" + data.warnings.map(w => "  - " + w).join("\n");
                }
                if (data.compilation_output) {
                    msg += "\n\n--- Compiler Output ---\n" + data.compilation_output;
                }
                outputDiv.textContent = msg;

                // Update current state and re-highlight
                currentSourceCode = source;
                const codeEl = document.getElementById("source-code");
                codeEl.contentEditable = "false";
                codeEl.textContent = source;
                codeEl.removeAttribute("data-highlighted");
                hljs.highlightElement(codeEl);
                codeEl.contentEditable = "true";

                if (data.metadata) {
                    currentMeta = data.metadata;
                    renderOverview(data.metadata);
                    renderApi(data.metadata);
                    renderSimulation(data.metadata);
                }

                // Refresh module list
                await loadModules();

            } else if (data.status === "compiled") {
                setVerifyStatus("Compiled (warning)", "ok");
                outputDiv.className = "output-ok";
                outputDiv.textContent = (data.warning || "") + "\n\n" + (data.compilation_output || "");
                currentSourceCode = source;

            } else if (data.status === "error") {
                setVerifyStatus("Compilation Error", "error");
                outputDiv.className = "output-error";
                outputDiv.textContent = data.compilation_output || "Unknown compilation error";

            } else if (data.error) {
                setVerifyStatus("Error", "error");
                outputDiv.className = "output-error";
                outputDiv.textContent = data.error;
            }
        } catch (err) {
            setVerifyStatus("Error", "error");
            outputDiv.className = "output-error";
            outputDiv.textContent = "Request failed: " + err.message;
        } finally {
            saveBtn.disabled = false;
            saveBtn.textContent = "Save & Verify";
        }
    }

    function setupSourceEditor() {
        document.getElementById("btn-edit-source").addEventListener("click", enterEditMode);
        document.getElementById("btn-cancel-edit").addEventListener("click", () => {
            // Restore original highlighted source
            const codeEl = document.getElementById("source-code");
            exitEditMode();
            if (currentSourceCode) {
                codeEl.textContent = currentSourceCode;
                codeEl.removeAttribute("data-highlighted");
                hljs.highlightElement(codeEl);
            }
            document.getElementById("verify-output").classList.add("hidden");
            setVerifyStatus("", "");
        });
        document.getElementById("btn-save-verify").addEventListener("click", doSaveAndVerify);

        // Tab key support in contenteditable code block
        document.getElementById("source-code").addEventListener("keydown", (e) => {
            if (e.key === "Tab") {
                e.preventDefault();
                document.execCommand("insertText", false, "    ");
            }
        });
    }

    function renderApi(meta) {
        const apiContent = document.getElementById("api-content");
        apiContent.innerHTML = `
            <div class="section-title">EdfModule Trait — Standard API</div>

            <div class="api-method">
                <div class="api-method-name">fn init(&amp;mut self)</div>
                <div class="api-method-desc">
                    Initialize the module. Allocate internal buffers, set default values,
                    and prepare the module for processing. Must be called before <code>process()</code>.
                </div>
            </div>

            <div class="api-method">
                <div class="api-method-name">fn process(&amp;mut self, inputs: &amp;[&amp;[u8]], outputs: &amp;mut [Vec&lt;u8&gt;])</div>
                <div class="api-method-desc">
                    Run one processing cycle. Each element in <code>inputs</code> corresponds to an input port
                    (${meta.input_ports.map(p => p.port_name).join(", ") || "none"}).
                    Each element in <code>outputs</code> corresponds to an output port
                    (${meta.output_ports.map(p => p.port_name).join(", ") || "none"}).
                    Data is exchanged as raw bytes (<code>&amp;[u8]</code>).
                </div>
            </div>

            <div class="api-method">
                <div class="api-method-name">fn configure(&amp;mut self, params: &amp;serde_json::Value)</div>
                <div class="api-method-desc">
                    Apply runtime configuration or calibration parameters.
                    Accepts a JSON value so each module can define its own parameter schema.
                </div>
            </div>

            <div class="api-method">
                <div class="api-method-name">fn reset(&amp;mut self)</div>
                <div class="api-method-desc">
                    Reset the module's internal state to initial conditions.
                    After calling <code>reset()</code>, the module should behave as if freshly initialized.
                </div>
            </div>

            <div class="api-method">
                <div class="api-method-name">fn metadata(&amp;self) -> ModuleMetadata</div>
                <div class="api-method-desc">
                    Return the module's metadata descriptor (MCP-compatible).
                    Provides name, version, description, category, ports, timing, resources,
                    and ASIL level for AI agent discovery and builder integration.
                </div>
            </div>
        `;
    }

    function formatBytes(bytes) {
        if (bytes >= 1048576) return (bytes / 1048576).toFixed(1) + " MB";
        if (bytes >= 1024) return (bytes / 1024).toFixed(1) + " KB";
        return bytes + " B";
    }

    function toSnakeCase(s) {
        return s
            .replace(/([A-Z]+)([A-Z][a-z])/g, "$1_$2")  // acronym end: AMRCodec -> AMR_Codec
            .replace(/([a-z0-9])([A-Z])/g, "$1_$2")       // camelCase: beamForming -> beam_Forming
            .toLowerCase();
    }

    // ---- Simulation ----

    function renderSimulation(meta) {
        if (simLoopTimer) { clearInterval(simLoopTimer); simLoopTimer = null; }

        document.getElementById("sim-status").textContent = "Not initialized";
        document.getElementById("sim-status").className = "sim-status";

        const configEl = document.getElementById("sim-config");
        if (meta.config_params && meta.config_params.length > 0) {
            const defaults = {};
            meta.config_params.forEach(p => defaults[p.name] = p.default_value);
            configEl.value = JSON.stringify(defaults);
            configEl.placeholder = JSON.stringify(defaults);
        } else {
            configEl.value = "";
            configEl.placeholder = "{}";
        }

        document.getElementById("sim-stop").disabled = true;
        document.getElementById("sim-loop").disabled = false;

        const inputsDiv = document.getElementById("sim-inputs");
        inputsDiv.innerHTML = "";
        meta.input_ports.forEach((port, i) => {
            const isFloat = port.data_type.startsWith("f32");
            const fallback = isFloat ? "0.5, 1.0" : "0, 0, 128, 63";
            const example = port.example_values || fallback;

            const wrapper = document.createElement("div");
            wrapper.className = "sim-port";

            const label = document.createElement("label");
            label.innerHTML = `${port.port_name} <code>${port.data_type}</code>`;

            const input = document.createElement("input");
            input.type = "text";
            input.id = `sim-input-${i}`;
            input.className = "sim-port-field";
            input.value = example;
            input.placeholder = example;
            input.spellcheck = false;
            input.dataset.dtype = port.data_type;
            input.dataset.sampleSize = port.sample_size_bytes;

            wrapper.appendChild(label);
            wrapper.appendChild(input);
            inputsDiv.appendChild(wrapper);
        });

        const outputsDiv = document.getElementById("sim-outputs");
        outputsDiv.innerHTML = "";
        meta.output_ports.forEach((port, i) => {
            const wrapper = document.createElement("div");
            wrapper.className = "sim-port";

            const label = document.createElement("label");
            label.innerHTML = `${port.port_name} <code>${port.data_type}</code>`;

            const output = document.createElement("div");
            output.id = `sim-output-${i}`;
            output.className = "sim-port-field sim-output-value";
            output.textContent = "\u2014";

            wrapper.appendChild(label);
            wrapper.appendChild(output);
            outputsDiv.appendChild(wrapper);
        });
    }

    function encodeInputPort(textareaEl) {
        const dtype = textareaEl.dataset.dtype;
        const raw = textareaEl.value.trim();
        if (!raw) return [];

        if (dtype.startsWith("f32")) {
            const floats = raw.split(/[,\s]+/).filter(s => s).map(Number);
            const bytes = [];
            const buf = new ArrayBuffer(4);
            const view = new DataView(buf);
            for (const f of floats) {
                view.setFloat32(0, f, true);
                for (let b = 0; b < 4; b++) bytes.push(view.getUint8(b));
            }
            return bytes;
        }
        return raw.split(/[,\s]+/).filter(s => s).map(n => parseInt(n, 10) & 0xFF);
    }

    function decodeOutputPort(bytes, meta_port) {
        if (!bytes || bytes.length === 0) return "[ empty ]";

        if (meta_port.data_type.startsWith("f32")) {
            const floats = [];
            const buf = new ArrayBuffer(4);
            const view = new DataView(buf);
            for (let i = 0; i + 3 < bytes.length; i += 4) {
                view.setUint8(0, bytes[i]);
                view.setUint8(1, bytes[i + 1]);
                view.setUint8(2, bytes[i + 2]);
                view.setUint8(3, bytes[i + 3]);
                floats.push(view.getFloat32(0, true).toFixed(6));
            }
            return floats.join(", ");
        }
        return bytes.join(", ");
    }

    async function simCall(action, body) {
        if (!selectedModule) return null;
        const url = `/api/modules/${encodeURIComponent(selectedModule)}/sim/${action}`;
        const opts = { method: "POST", headers: { "Content-Type": "application/json" } };
        if (body) opts.body = JSON.stringify(body);
        const resp = await fetch(url, opts);
        return resp.json();
    }

    function setSimStatus(text, type) {
        const el = document.getElementById("sim-status");
        el.textContent = text;
        el.className = "sim-status" + (type ? " sim-status-" + type : "");
    }

    async function doProcess() {
        if (!currentMeta) return;
        const inputs = currentMeta.input_ports.map((_, i) => {
            const el = document.getElementById(`sim-input-${i}`);
            if (!el) return [];
            return encodeInputPort(el);
        });
        try {
            const result = await simCall("process", { inputs });
            if (result.error) { setSimStatus(result.error, "error"); return; }
            if (result.outputs) {
                currentMeta.output_ports.forEach((port, i) => {
                    const el = document.getElementById(`sim-output-${i}`);
                    if (el) {
                        el.textContent = decodeOutputPort(result.outputs[i] || [], port);
                    }
                });
                setSimStatus("Processed", "ok");
            }
        } catch (err) {
            setSimStatus("Process error: " + err.message, "error");
        }
    }

    function setupSimulation() {
        document.getElementById("sim-init").addEventListener("click", async () => {
            try {
                const result = await simCall("init");
                if (result.error) { setSimStatus(result.error, "error"); return; }
                setSimStatus("Initialized", "ok");
            } catch (err) { setSimStatus("Init error: " + err.message, "error"); }
        });

        document.getElementById("sim-reset").addEventListener("click", async () => {
            try {
                const result = await simCall("reset");
                if (result.error) { setSimStatus(result.error, "error"); return; }
                setSimStatus("Reset", "ok");
            } catch (err) { setSimStatus("Reset error: " + err.message, "error"); }
        });

        document.getElementById("sim-configure").addEventListener("click", async () => {
            const raw = document.getElementById("sim-config").value.trim();
            let params;
            try { params = raw ? JSON.parse(raw) : {}; }
            catch { setSimStatus("Invalid JSON in config", "error"); return; }
            try {
                const result = await simCall("configure", { params });
                if (result.error) { setSimStatus(result.error, "error"); return; }
                setSimStatus("Configured", "ok");
            } catch (err) { setSimStatus("Configure error: " + err.message, "error"); }
        });

        document.getElementById("sim-process").addEventListener("click", doProcess);

        document.getElementById("sim-loop").addEventListener("click", () => {
            if (simLoopTimer) return;
            const interval = parseInt(document.getElementById("sim-interval").value) || 200;
            document.getElementById("sim-loop").disabled = true;
            document.getElementById("sim-stop").disabled = false;
            simLoopTimer = setInterval(doProcess, interval);
        });

        document.getElementById("sim-stop").addEventListener("click", () => {
            if (simLoopTimer) { clearInterval(simLoopTimer); simLoopTimer = null; }
            document.getElementById("sim-loop").disabled = false;
            document.getElementById("sim-stop").disabled = true;
        });
    }

    // ---- Tabs ----

    function setupTabs() {
        const buttons = document.querySelectorAll(".tab-btn");
        buttons.forEach(btn => {
            btn.addEventListener("click", () => {
                buttons.forEach(b => b.classList.remove("active"));
                btn.classList.add("active");

                document.querySelectorAll(".tab-content").forEach(tc => tc.classList.remove("active"));
                const tabId = "tab-" + btn.dataset.tab;
                document.getElementById(tabId).classList.add("active");
            });
        });
    }

    // ---- Create module modal ----

    function showStep(stepId) {
        document.querySelectorAll(".create-step").forEach(s => s.classList.add("hidden"));
        document.getElementById(stepId).classList.remove("hidden");
        document.getElementById("create-result").classList.add("hidden");
    }

    // Wizard port management
    function createPortRow(prefix, index) {
        const row = document.createElement("div");
        row.className = "port-row";
        row.dataset.index = index;
        row.innerHTML = `
            <input class="port-name" type="text" placeholder="port_name" value="${prefix}_${index}">
            <select class="port-type">
                <option value="f32[]">f32[]</option>
                <option value="u8[]">u8[]</option>
                <option value="i16[]">i16[]</option>
                <option value="f64[]">f64[]</option>
            </select>
            <input class="port-size" type="number" value="4" min="1" max="64" title="Sample size (bytes)">
            <input class="port-desc" type="text" placeholder="Description">
            <button class="btn-remove-port" title="Remove">&times;</button>
        `;
        row.querySelector(".btn-remove-port").addEventListener("click", () => row.remove());
        return row;
    }

    function collectPorts(containerId) {
        const ports = [];
        document.querySelectorAll(`#${containerId} .port-row`).forEach(row => {
            ports.push({
                port_name: row.querySelector(".port-name").value.trim() || "port",
                data_type: row.querySelector(".port-type").value,
                sample_size_bytes: parseInt(row.querySelector(".port-size").value) || 4,
                description: row.querySelector(".port-desc").value.trim(),
            });
        });
        return ports;
    }

    function setupCreateModal() {
        const overlay = document.getElementById("create-modal-overlay");
        const resultDiv = document.getElementById("create-result");
        let inputPortIdx = 0;
        let outputPortIdx = 0;

        // Open modal
        document.getElementById("btn-create-module").addEventListener("click", () => {
            showStep("create-step-choose");
            overlay.classList.remove("hidden");
        });

        // Close on overlay click
        overlay.addEventListener("click", (e) => {
            if (e.target === overlay) overlay.classList.add("hidden");
        });

        // Choose method
        document.getElementById("btn-method-ai").addEventListener("click", () => {
            document.getElementById("ai-module-name").value = "";
            document.getElementById("ai-module-description").value = "";
            showStep("create-step-ai");
        });

        document.getElementById("btn-method-manual").addEventListener("click", () => {
            document.getElementById("man-module-name").value = "";
            document.getElementById("man-module-description").value = "";
            document.getElementById("wizard-input-ports").innerHTML = "";
            document.getElementById("wizard-output-ports").innerHTML = "";
            inputPortIdx = 0;
            outputPortIdx = 0;

            // Add one default input and output
            document.getElementById("wizard-input-ports").appendChild(createPortRow("input", inputPortIdx++));
            document.getElementById("wizard-output-ports").appendChild(createPortRow("output", outputPortIdx++));
            showStep("create-step-manual");
        });

        // Back buttons
        document.getElementById("btn-ai-back").addEventListener("click", () => showStep("create-step-choose"));
        document.getElementById("btn-man-back").addEventListener("click", () => showStep("create-step-choose"));

        // Add port buttons
        document.getElementById("btn-add-input").addEventListener("click", () => {
            document.getElementById("wizard-input-ports").appendChild(createPortRow("input", inputPortIdx++));
        });
        document.getElementById("btn-add-output").addEventListener("click", () => {
            document.getElementById("wizard-output-ports").appendChild(createPortRow("output", outputPortIdx++));
        });

        // ---- AI creation ----
        document.getElementById("btn-ai-create").addEventListener("click", async () => {
            const name = document.getElementById("ai-module-name").value.trim();
            const category = document.getElementById("ai-module-category").value;
            const description = document.getElementById("ai-module-description").value.trim();
            const createBtn = document.getElementById("btn-ai-create");

            if (!name) {
                resultDiv.textContent = "Module name is required.";
                resultDiv.className = "error";
                resultDiv.classList.remove("hidden");
                return;
            }
            if (!/^[A-Z][a-zA-Z0-9]*$/.test(name)) {
                resultDiv.textContent = "Name must be PascalCase (e.g. NoiseReduction).";
                resultDiv.className = "error";
                resultDiv.classList.remove("hidden");
                return;
            }
            if (!description) {
                resultDiv.textContent = "Description is required for AI generation.";
                resultDiv.className = "error";
                resultDiv.classList.remove("hidden");
                return;
            }

            createBtn.disabled = true;
            createBtn.textContent = "Generating...";
            resultDiv.textContent = "AI is generating the Rust module code...";
            resultDiv.className = "";
            resultDiv.classList.remove("hidden");

            try {
                const aiResp = await fetch("http://localhost:6590/generate-module", {
                    method: "POST",
                    headers: { "Content-Type": "application/json" },
                    body: JSON.stringify({ name, category, description }),
                });
                const aiData = await aiResp.json();

                if (!aiResp.ok) {
                    resultDiv.textContent = aiData.error || "AI generation failed.";
                    resultDiv.className = "error";
                    createBtn.disabled = false;
                    createBtn.textContent = "Create with AI";
                    return;
                }

                resultDiv.textContent = "AI code generated. Saving module file...";

                const resp = await fetch("/api/modules", {
                    method: "POST",
                    headers: { "Content-Type": "application/json" },
                    body: JSON.stringify({ name, category, description, source: aiData.source }),
                });
                const data = await resp.json();

                if (!resp.ok) {
                    resultDiv.textContent = data.error || "Creation failed.";
                    resultDiv.className = "error";
                    createBtn.disabled = false;
                    createBtn.textContent = "Create with AI";
                    return;
                }

                resultDiv.textContent = `Module "${name}" created with AI! ${data.note}`;
                resultDiv.className = "";

                setTimeout(() => {
                    overlay.classList.add("hidden");
                    createBtn.disabled = false;
                    createBtn.textContent = "Create with AI";
                    loadModules();
                }, 2000);
            } catch (err) {
                resultDiv.textContent = `Error: ${err.message}`;
                resultDiv.className = "error";
                createBtn.disabled = false;
                createBtn.textContent = "Create with AI";
                resultDiv.classList.remove("hidden");
            }
        });

        // ---- Manual creation ----
        document.getElementById("btn-man-create").addEventListener("click", async () => {
            const name = document.getElementById("man-module-name").value.trim();
            const category = document.getElementById("man-module-category").value;
            const description = document.getElementById("man-module-description").value.trim();
            const asil_level = document.getElementById("man-asil").value;
            const scheduling_type = document.getElementById("man-scheduling").value;
            const createBtn = document.getElementById("btn-man-create");

            if (!name) {
                resultDiv.textContent = "Module name is required.";
                resultDiv.className = "error";
                resultDiv.classList.remove("hidden");
                return;
            }
            if (!/^[A-Z][a-zA-Z0-9]*$/.test(name)) {
                resultDiv.textContent = "Name must be PascalCase (e.g. NoiseReduction).";
                resultDiv.className = "error";
                resultDiv.classList.remove("hidden");
                return;
            }

            const input_ports = collectPorts("wizard-input-ports");
            const output_ports = collectPorts("wizard-output-ports");

            createBtn.disabled = true;
            createBtn.textContent = "Creating...";
            resultDiv.textContent = "Creating module crate...";
            resultDiv.className = "";
            resultDiv.classList.remove("hidden");

            try {
                const resp = await fetch("/api/modules/create-manual", {
                    method: "POST",
                    headers: { "Content-Type": "application/json" },
                    body: JSON.stringify({
                        name, category, description,
                        asil_level, scheduling_type,
                        input_ports, output_ports,
                    }),
                });
                const data = await resp.json();

                if (!resp.ok) {
                    resultDiv.textContent = data.error || "Creation failed.";
                    resultDiv.className = "error";
                    createBtn.disabled = false;
                    createBtn.textContent = "Create Module";
                    return;
                }

                resultDiv.textContent = `Module "${name}" created at ${data.path}. Open Source Code tab to edit and compile.`;
                resultDiv.className = "";

                setTimeout(async () => {
                    overlay.classList.add("hidden");
                    createBtn.disabled = false;
                    createBtn.textContent = "Create Module";
                    await loadModules();
                    // Auto-select the new module
                    selectModule(name);
                }, 1500);
            } catch (err) {
                resultDiv.textContent = `Error: ${err.message}`;
                resultDiv.className = "error";
                createBtn.disabled = false;
                createBtn.textContent = "Create Module";
                resultDiv.classList.remove("hidden");
            }
        });
    }

    // ---- Init ----

    function init() {
        setupTabs();
        setupCreateModal();
        setupSimulation();
        setupSourceEditor();
        loadModules();

        document.getElementById("search-input").addEventListener("input", renderModuleList);
        document.getElementById("category-filter").addEventListener("change", renderModuleList);
    }

    init();
})();
