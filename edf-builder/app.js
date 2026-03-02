// Copyright (c) 2026 Ivan LE HIN
// Licensed under CC BY-NC-SA 4.0 — Non-commercial use only.
// For commercial licensing, contact the author.
// https://creativecommons.org/licenses/by-nc-sa/4.0/

// ============================================================
// Safety Topology Builder - Main Application
// ============================================================

(() => {
    'use strict';

    // ----- State -----
    const state = {
        modules: [],          // ModuleClass templates (new model)
        instances: [],        // ModuleInstance on canvas (new model)
        connections: [],      // Links: { id, fromInstanceId, fromPort, toInstanceId, toPort }
        useCases: [],         // Use Case groups (renamed from pipelines)
        nextInstanceCounters: {},  // moduleId -> counter
        topologyName: '',          // user-chosen name for the topology
        zoom: 1,
        panX: 0,
        panY: 0,
        selectedIds: new Set(),
        selectedType: null,        // 'instance' | 'usecase' | null
    };

    let idCounter = 1;
    const genId = () => `id-${idCounter++}`;
    const genUUID = () => crypto.randomUUID ? crypto.randomUUID() : `${Date.now()}-${Math.random().toString(36).slice(2)}`;

    // ----- DOM refs -----
    const $moduleList = document.getElementById('module-list');
    const $canvas = document.getElementById('canvas');
    const $viewport = document.getElementById('canvas-viewport');
    const $svg = document.getElementById('connections-svg');
    const $props = document.getElementById('properties-content');
    const $ctxMenu = document.getElementById('context-menu');

    // ----- Default modules -----
    function initDefaults() {
        // No defaults — user creates modules from scratch
    }

    // ============================================================
    // MODULE LIBRARY (new ModuleClass model)
    // ============================================================
    function addModule(params) {
        const name = (typeof params === 'string') ? params : params.name;
        if (!name || !name.trim()) return;

        const mod = {
            id: genId(),
            uuid: genUUID(),
            name: name.trim(),
            version: (typeof params === 'object' ? params.version : undefined) || 1,
            input_ports: (typeof params === 'object' ? params.input_ports : undefined) || [],
            output_ports: (typeof params === 'object' ? params.output_ports : undefined) || [],
            wcet_us: (typeof params === 'object' ? params.wcet_us : undefined) || 500,
            bcet_us: (typeof params === 'object' ? params.bcet_us : undefined) || 100,
            typical_us: (typeof params === 'object' ? params.typical_us : undefined) || 300,
            stack_size_bytes: (typeof params === 'object' ? params.stack_size_bytes : undefined) || 4096,
            static_mem_bytes: (typeof params === 'object' ? params.static_mem_bytes : undefined) || 1024,
            requires_fpu: (typeof params === 'object' ? params.requires_fpu : undefined) || false,
            requires_gpu: (typeof params === 'object' ? params.requires_gpu : undefined) || false,
            asil_level: (typeof params === 'object' ? params.asil_level : undefined) || 'QM',
        };
        state.modules.push(mod);
        state.nextInstanceCounters[mod.id] = 0;
        renderModuleList();
        return mod;
    }

    function removeModule(modId) {
        state.modules = state.modules.filter(m => m.id !== modId);
        delete state.nextInstanceCounters[modId];
        renderModuleList();
    }

    function asilBadgeHtml(level) {
        const cls = 'asil-' + (level || 'qm').toLowerCase();
        return `<span class="asil-badge ${cls}">${level || 'QM'}</span>`;
    }

    function renderModuleList() {
        $moduleList.innerHTML = '';
        state.modules.forEach(mod => {
            const el = document.createElement('div');
            el.className = 'module-template';
            el.draggable = true;
            el.dataset.moduleId = mod.id;
            const nbIn = mod.input_ports ? mod.input_ports.length : 0;
            const nbOut = mod.output_ports ? mod.output_ports.length : 0;
            el.innerHTML = `
                <span class="mod-name">${esc(mod.name)} ${asilBadgeHtml(mod.asil_level)}</span>
                <span class="mod-info">${nbIn} in / ${nbOut} out | WCET: ${mod.wcet_us}µs</span>
                <button class="btn-delete-module" title="Delete this module">&times;</button>
            `;
            el.querySelector('.btn-delete-module').addEventListener('click', (e) => {
                e.stopPropagation();
                removeModule(mod.id);
            });
            el.addEventListener('dblclick', (e) => {
                e.stopPropagation();
                openCreateModuleModal(mod);
            });
            el.addEventListener('dragstart', (e) => {
                e.dataTransfer.setData('application/module-id', mod.id);
                e.dataTransfer.effectAllowed = 'copy';
            });
            $moduleList.appendChild(el);
        });
    }

    // ============================================================
    // MODULE CREATION MODAL (new model)
    // ============================================================
    const $modalOverlay = document.getElementById('modal-overlay');
    const $modalTitle = document.getElementById('modal-title');
    const $modalConfirm = document.getElementById('btn-modal-confirm');
    let editingModuleId = null;

    function addPortRow(container, port) {
        const row = document.createElement('div');
        row.className = 'modal-port-row';
        row.innerHTML = `
            <input type="text" class="port-field-name" placeholder="port_name" value="${esc(port?.port_name || '')}">
            <input type="text" class="port-field-type" placeholder="data_type" value="${esc(port?.data_type || '')}">
            <input type="number" class="port-field-size" placeholder="bytes" min="0" value="${port?.sample_size_bytes || 0}">
            <button type="button" class="btn-remove-port">&times;</button>
        `;
        row.querySelector('.btn-remove-port').addEventListener('click', () => row.remove());
        container.appendChild(row);
    }

    function readPortsFromContainer(container) {
        return [...container.querySelectorAll('.modal-port-row')].map(row => ({
            port_name: row.querySelector('.port-field-name').value.trim(),
            data_type: row.querySelector('.port-field-type').value.trim(),
            sample_size_bytes: parseInt(row.querySelector('.port-field-size').value) || 0,
        })).filter(p => p.port_name);
    }

    document.getElementById('btn-add-input-port').addEventListener('click', () => {
        addPortRow(document.getElementById('modal-input-ports'), null);
    });
    document.getElementById('btn-add-output-port').addEventListener('click', () => {
        addPortRow(document.getElementById('modal-output-ports'), null);
    });

    function openCreateModuleModal(mod) {
        editingModuleId = mod ? mod.id : null;
        $modalTitle.textContent = mod ? 'Edit Module' : 'Create Module';
        $modalConfirm.textContent = mod ? 'Save' : 'Create';

        document.getElementById('modal-mod-name').value = mod ? mod.name : '';
        document.getElementById('modal-mod-version').value = mod ? mod.version : 1;
        document.getElementById('modal-mod-wcet').value = mod ? mod.wcet_us : 500;
        document.getElementById('modal-mod-bcet').value = mod ? mod.bcet_us : 100;
        document.getElementById('modal-mod-typical').value = mod ? mod.typical_us : 300;
        document.getElementById('modal-mod-stack').value = mod ? mod.stack_size_bytes : 4096;
        document.getElementById('modal-mod-mem').value = mod ? mod.static_mem_bytes : 1024;
        document.getElementById('modal-mod-fpu').checked = mod ? mod.requires_fpu : false;
        document.getElementById('modal-mod-gpu').checked = mod ? mod.requires_gpu : false;
        document.getElementById('modal-mod-asil').value = mod ? (mod.asil_level || 'QM') : 'QM';

        // Populate ports
        const inContainer = document.getElementById('modal-input-ports');
        const outContainer = document.getElementById('modal-output-ports');
        inContainer.innerHTML = '';
        outContainer.innerHTML = '';
        if (mod && mod.input_ports) mod.input_ports.forEach(p => addPortRow(inContainer, p));
        if (mod && mod.output_ports) mod.output_ports.forEach(p => addPortRow(outContainer, p));

        $modalOverlay.classList.remove('hidden');
        document.getElementById('modal-mod-name').focus();
    }

    function closeModal() {
        $modalOverlay.classList.add('hidden');
        editingModuleId = null;
    }

    document.getElementById('btn-open-create-modal').addEventListener('click', () => openCreateModuleModal(null));
    document.getElementById('btn-modal-cancel').addEventListener('click', closeModal);
    $modalOverlay.addEventListener('click', (e) => { if (e.target === $modalOverlay) closeModal(); });
    document.addEventListener('keydown', (e) => {
        if (e.key === 'Escape' && !$modalOverlay.classList.contains('hidden')) closeModal();
    });

    $modalConfirm.addEventListener('click', () => {
        const name = document.getElementById('modal-mod-name').value;
        if (!name.trim()) { document.getElementById('modal-mod-name').focus(); return; }

        const input_ports = readPortsFromContainer(document.getElementById('modal-input-ports'));
        const output_ports = readPortsFromContainer(document.getElementById('modal-output-ports'));

        const data = {
            name: name.trim(),
            version: parseInt(document.getElementById('modal-mod-version').value) || 1,
            input_ports,
            output_ports,
            wcet_us: parseInt(document.getElementById('modal-mod-wcet').value) || 0,
            bcet_us: parseInt(document.getElementById('modal-mod-bcet').value) || 0,
            typical_us: parseInt(document.getElementById('modal-mod-typical').value) || 0,
            stack_size_bytes: parseInt(document.getElementById('modal-mod-stack').value) || 0,
            static_mem_bytes: parseInt(document.getElementById('modal-mod-mem').value) || 0,
            requires_fpu: document.getElementById('modal-mod-fpu').checked,
            requires_gpu: document.getElementById('modal-mod-gpu').checked,
            asil_level: document.getElementById('modal-mod-asil').value,
        };

        if (editingModuleId) {
            const mod = state.modules.find(m => m.id === editingModuleId);
            if (mod) {
                Object.assign(mod, data);
                renderModuleList();
            }
        } else {
            addModule(data);
        }
        closeModal();
    });

    // ============================================================
    // CANVAS: Drop, Pan, Zoom
    // ============================================================
    $viewport.addEventListener('dragover', (e) => { e.preventDefault(); e.dataTransfer.dropEffect = 'copy'; });
    $viewport.addEventListener('drop', (e) => {
        e.preventDefault();
        const modId = e.dataTransfer.getData('application/module-id');
        if (!modId) return;
        const mod = state.modules.find(m => m.id === modId);
        if (!mod) return;
        const rect = $viewport.getBoundingClientRect();
        const x = (e.clientX - rect.left - state.panX) / state.zoom;
        const y = (e.clientY - rect.top - state.panY) / state.zoom;
        createInstance(mod, x, y);
    });

    // Zoom
    $viewport.addEventListener('wheel', (e) => {
        e.preventDefault();
        const delta = e.deltaY > 0 ? -0.05 : 0.05;
        state.zoom = Math.max(0.2, Math.min(3, state.zoom + delta));
        applyTransform();
    }, { passive: false });

    document.getElementById('btn-zoom-in').addEventListener('click', () => { state.zoom = Math.min(3, state.zoom + 0.1); applyTransform(); });
    document.getElementById('btn-zoom-out').addEventListener('click', () => { state.zoom = Math.max(0.2, state.zoom - 0.1); applyTransform(); });
    document.getElementById('btn-zoom-reset').addEventListener('click', () => { state.zoom = 1; state.panX = 0; state.panY = 0; applyTransform(); });

    function applyTransform() {
        $canvas.style.transform = `translate(${state.panX}px, ${state.panY}px) scale(${state.zoom})`;
        $svg.style.transform = `translate(${state.panX}px, ${state.panY}px) scale(${state.zoom})`;
        $svg.style.transformOrigin = '0 0';
    }

    // Pan with middle mouse button or Ctrl+drag
    let isPanning = false, panStartX, panStartY;
    $viewport.addEventListener('mousedown', (e) => {
        if (e.button === 1 || (e.button === 0 && e.ctrlKey && !e.target.closest('.instance-node') && !e.target.closest('.usecase-group'))) {
            isPanning = true;
            panStartX = e.clientX - state.panX;
            panStartY = e.clientY - state.panY;
            e.preventDefault();
        }
    });
    window.addEventListener('mousemove', (e) => {
        if (isPanning) {
            state.panX = e.clientX - panStartX;
            state.panY = e.clientY - panStartY;
            applyTransform();
        }
    });
    window.addEventListener('mouseup', () => {
        if (isPanning) isPanning = false;
    });

    // Deselect on canvas click
    $viewport.addEventListener('mousedown', (e) => {
        if (e.target === $viewport || e.target === $canvas) {
            if (!e.ctrlKey && !e.shiftKey) {
                clearSelection();
            }
            if (e.button === 0 && !e.ctrlKey) {
                startSelectionRect(e);
            }
        }
    });

    // ============================================================
    // SELECTION RECTANGLE
    // ============================================================
    let selRect = null;
    function startSelectionRect(e) {
        const rect = $viewport.getBoundingClientRect();
        const sx = e.clientX - rect.left;
        const sy = e.clientY - rect.top;
        selRect = { sx, sy, el: document.createElement('div') };
        selRect.el.className = 'selection-rect';
        selRect.el.style.left = sx + 'px';
        selRect.el.style.top = sy + 'px';
        selRect.el.style.width = '0';
        selRect.el.style.height = '0';
        $viewport.appendChild(selRect.el);

        const onMove = (ev) => {
            const cx = ev.clientX - rect.left;
            const cy = ev.clientY - rect.top;
            const x = Math.min(sx, cx), y = Math.min(sy, cy);
            const w = Math.abs(cx - sx), h = Math.abs(cy - sy);
            selRect.el.style.left = x + 'px';
            selRect.el.style.top = y + 'px';
            selRect.el.style.width = w + 'px';
            selRect.el.style.height = h + 'px';
        };
        const onUp = (ev) => {
            window.removeEventListener('mousemove', onMove);
            window.removeEventListener('mouseup', onUp);
            if (!selRect) return;
            const cx = ev.clientX - rect.left;
            const cy = ev.clientY - rect.top;
            const rx = Math.min(sx, cx), ry = Math.min(sy, cy);
            const rw = Math.abs(cx - sx), rh = Math.abs(cy - sy);
            selRect.el.remove();
            selRect = null;
            if (rw > 5 && rh > 5) {
                selectInRect(rx, ry, rw, rh, rect);
            }
        };
        window.addEventListener('mousemove', onMove);
        window.addEventListener('mouseup', onUp);
    }

    function selectInRect(rx, ry, rw, rh, viewportRect) {
        clearSelection();
        state.instances.forEach(inst => {
            const el = document.getElementById(inst.id);
            if (!el) return;
            const r = el.getBoundingClientRect();
            const ix = r.left - viewportRect.left;
            const iy = r.top - viewportRect.top;
            if (ix < rx + rw && ix + r.width > rx && iy < ry + rh && iy + r.height > ry) {
                state.selectedIds.add(inst.id);
            }
        });
        if (state.selectedIds.size > 0) {
            state.selectedType = 'instance';
            updateSelectionVisuals();
            renderProperties();
        }
    }

    // ============================================================
    // INSTANCES (new ModuleInstance model)
    // ============================================================
    function createInstance(mod, x, y) {
        state.nextInstanceCounters[mod.id] = (state.nextInstanceCounters[mod.id] || 0) + 1;
        const counter = state.nextInstanceCounters[mod.id];
        const inst = {
            id: genId(),
            moduleId: mod.id,
            moduleName: mod.name,
            instance_id: counter,
            name: `${mod.name}/${counter}`,
            x, y,
            // Activation
            activation: 'PERIODIC',
            period_us: 10000,
            min_interarrival_us: 0,
            // Port configs (from module ports with defaults)
            input_configs: (mod.input_ports || []).map(p => ({
                port_name: p.port_name,
                fifo_depth: 4,
                mandatory: true,
                mode: 'ALL',
                consume_count: 1,
                overflow_policy: 'DROP_OLDEST',
            })),
            output_configs: (mod.output_ports || []).map(p => ({
                port_name: p.port_name,
                burst_size: 1,
                suggested_min_depth: 2,
            })),
            // Placement
            allowed_cores_mask: 0xFFFF,
            preferred_core: 0,
            affinity_group: '',
            // Runtime state
            state: 'INACTIVE',
        };
        state.instances.push(inst);
        renderInstance(inst);
        return inst;
    }

    function renderInstance(inst) {
        const mod = state.modules.find(m => m.id === inst.moduleId);
        const el = document.createElement('div');
        el.className = 'instance-node';
        el.id = inst.id;
        el.style.left = inst.x + 'px';
        el.style.top = inst.y + 'px';

        // Header
        const header = document.createElement('div');
        header.className = 'node-header';
        const asilLevel = mod ? mod.asil_level : 'QM';
        header.innerHTML = `<span>${esc(inst.name)}</span><span class="node-type">${asilBadgeHtml(asilLevel)} ${inst.activation}</span>`;
        el.appendChild(header);

        // Body with named ports
        const body = document.createElement('div');
        body.className = 'node-body';

        const inCol = document.createElement('div');
        inCol.className = 'ports-col inputs';
        const inputPorts = mod ? mod.input_ports : (inst.input_configs || []);
        inputPorts.forEach((p, i) => {
            const port = document.createElement('div');
            port.className = 'port input';
            port.dataset.instanceId = inst.id;
            port.dataset.portIndex = i;
            port.dataset.portType = 'input';
            port.innerHTML = `<span class="port-dot"></span><span>${esc(p.port_name || `in-${i}`)}</span>`;
            inCol.appendChild(port);
        });

        const outCol = document.createElement('div');
        outCol.className = 'ports-col outputs';
        const outputPorts = mod ? mod.output_ports : (inst.output_configs || []);
        outputPorts.forEach((p, i) => {
            const port = document.createElement('div');
            port.className = 'port output';
            port.dataset.instanceId = inst.id;
            port.dataset.portIndex = i;
            port.dataset.portType = 'output';
            port.innerHTML = `<span>${esc(p.port_name || `out-${i}`)}</span><span class="port-dot"></span>`;
            outCol.appendChild(port);
        });

        body.appendChild(inCol);
        body.appendChild(outCol);
        el.appendChild(body);

        // Drag to move
        makeDraggable(el, inst);

        // Select on click
        el.addEventListener('mousedown', (e) => {
            if (e.target.closest('.port')) return;
            e.stopPropagation();
            if (e.shiftKey || e.ctrlKey) {
                toggleSelection(inst.id, 'instance');
            } else if (!state.selectedIds.has(inst.id)) {
                clearSelection();
                state.selectedIds.add(inst.id);
                state.selectedType = 'instance';
                updateSelectionVisuals();
                renderProperties();
            }
        });

        // Right-click context menu
        el.addEventListener('contextmenu', (e) => {
            e.preventDefault();
            e.stopPropagation();
            if (!state.selectedIds.has(inst.id)) {
                clearSelection();
                state.selectedIds.add(inst.id);
                state.selectedType = 'instance';
                updateSelectionVisuals();
                renderProperties();
            }
            const menuItems = [
                { label: 'Delete', action: () => deleteSelected() },
                { label: 'Duplicate', action: () => duplicateInstance(inst) },
            ];

            // --- Use Case actions ---
            menuItems.push({ separator: true });
            const memberUcIds = new Set(findUseCasesOfInstance(inst.id).map(u => u.id));
            const availableUcs = state.useCases.filter(u => !memberUcIds.has(u.id));
            if (availableUcs.length > 0) {
                availableUcs.forEach(uc => {
                    menuItems.push({ label: `Add to "${uc.name}"`, action: () => addInstanceToUseCase(inst.id, uc.id) });
                });
            }
            menuItems.push({ label: 'Create Use Case with this instance', action: () => createUseCase([inst.id]) });

            // "Retirer d'un Use Case" if already in some
            if (memberUcIds.size > 0) {
                findUseCasesOfInstance(inst.id).forEach(uc => {
                    menuItems.push({ label: `Remove from "${uc.name}"`, action: () => removeInstanceFromUseCase(inst.id, uc.id) });
                });
            }

            showContextMenu(e.clientX, e.clientY, menuItems);
        });

        // Port click for connections
        el.querySelectorAll('.port').forEach(portEl => {
            portEl.addEventListener('mousedown', (e) => {
                e.stopPropagation();
                e.preventDefault();
                startConnection(portEl, e);
            });
        });

        $canvas.appendChild(el);
    }

    function duplicateInstance(inst) {
        const mod = state.modules.find(m => m.id === inst.moduleId);
        if (mod) createInstance(mod, inst.x + 30, inst.y + 30);
    }

    // ============================================================
    // DRAG INSTANCES
    // ============================================================
    function makeDraggable(el, inst) {
        let startX, startY, origX, origY;
        let dragOffsets = [];

        const onMouseDown = (e) => {
            if (e.target.closest('.port')) return;
            if (e.button !== 0) return;
            startX = e.clientX;
            startY = e.clientY;
            origX = inst.x;
            origY = inst.y;
            dragOffsets = [];
            if (state.selectedIds.has(inst.id) && state.selectedIds.size > 1) {
                state.selectedIds.forEach(id => {
                    const other = state.instances.find(i => i.id === id);
                    if (other) dragOffsets.push({ inst: other, dx: other.x - inst.x, dy: other.y - inst.y });
                });
            }
            window.addEventListener('mousemove', onMouseMove);
            window.addEventListener('mouseup', onMouseUp);
        };

        const onMouseMove = (e) => {
            const dx = (e.clientX - startX) / state.zoom;
            const dy = (e.clientY - startY) / state.zoom;
            inst.x = origX + dx;
            inst.y = origY + dy;
            el.style.left = inst.x + 'px';
            el.style.top = inst.y + 'px';
            dragOffsets.forEach(({ inst: other, dx: offX, dy: offY }) => {
                if (other.id === inst.id) return;
                other.x = inst.x + offX;
                other.y = inst.y + offY;
                const otherEl = document.getElementById(other.id);
                if (otherEl) {
                    otherEl.style.left = other.x + 'px';
                    otherEl.style.top = other.y + 'px';
                }
            });
            updateAllConnections();
            updateAllUseCaseBounds();
        };

        const onMouseUp = () => {
            window.removeEventListener('mousemove', onMouseMove);
            window.removeEventListener('mouseup', onMouseUp);
        };

        el.addEventListener('mousedown', onMouseDown);
    }

    // ============================================================
    // CONNECTIONS (LINKS)
    // ============================================================
    let tempLink = null;

    function startConnection(portEl, e) {
        const portType = portEl.dataset.portType;
        const instId = portEl.dataset.instanceId;
        const portIndex = parseInt(portEl.dataset.portIndex);
        if (portType !== 'output') return;

        const dot = portEl.querySelector('.port-dot');
        const dotRect = dot.getBoundingClientRect();
        const svgRect = $svg.getBoundingClientRect();
        const startXpx = (dotRect.left + dotRect.width / 2 - svgRect.left - state.panX) / state.zoom;
        const startYpx = (dotRect.top + dotRect.height / 2 - svgRect.top - state.panY) / state.zoom;

        const path = document.createElementNS('http://www.w3.org/2000/svg', 'path');
        path.setAttribute('stroke', 'var(--link-color)');
        path.setAttribute('stroke-width', '2');
        path.setAttribute('fill', 'none');
        path.setAttribute('stroke-dasharray', '6 3');
        path.classList.add('temp-link');
        $svg.appendChild(path);

        tempLink = { path, startX: startXpx, startY: startYpx, fromInstanceId: instId, fromPort: portIndex };

        const onMove = (ev) => {
            const ex = (ev.clientX - svgRect.left - state.panX) / state.zoom;
            const ey = (ev.clientY - svgRect.top - state.panY) / state.zoom;
            path.setAttribute('d', bezierPath(startXpx, startYpx, ex, ey));
        };

        const onUp = (ev) => {
            window.removeEventListener('mousemove', onMove);
            window.removeEventListener('mouseup', onUp);
            path.remove();
            const target = document.elementFromPoint(ev.clientX, ev.clientY);
            if (target) {
                const targetPort = target.closest('.port.input');
                if (targetPort) {
                    const toInstId = targetPort.dataset.instanceId;
                    const toPortIdx = parseInt(targetPort.dataset.portIndex);
                    if (toInstId !== instId) {
                        addConnection(instId, portIndex, toInstId, toPortIdx);
                    }
                }
            }
            tempLink = null;
        };

        window.addEventListener('mousemove', onMove);
        window.addEventListener('mouseup', onUp);
    }

    function addConnection(fromInstanceId, fromPort, toInstanceId, toPort) {
        const exists = state.connections.some(c =>
            c.fromInstanceId === fromInstanceId && c.fromPort === fromPort &&
            c.toInstanceId === toInstanceId && c.toPort === toPort
        );
        if (exists) return;
        const conn = { id: genId(), fromInstanceId, fromPort, toInstanceId, toPort };
        state.connections.push(conn);
        renderConnection(conn);
        updateAllUseCaseBounds();
    }

    function computeFifoDepth(fromInst, toInst) {
        if (!fromInst || !toInst) return 1;
        if (fromInst.activation !== 'PERIODIC' || toInst.activation !== 'PERIODIC') return 1;
        const pFrom = fromInst.period_us || 1;
        const pTo = toInst.period_us || 1;
        return pTo > pFrom ? Math.round(pTo / pFrom) : 1;
    }

    // Returns 'same-rate', 'slow-consumer' (FIFO accumulation), or 'fast-consumer' (sample-and-hold)
    function getConnectionType(fromInst, toInst) {
        if (!fromInst || !toInst) return 'same-rate';
        if (fromInst.activation !== 'PERIODIC' || toInst.activation !== 'PERIODIC') return 'same-rate';
        const pFrom = fromInst.period_us || 1;
        const pTo = toInst.period_us || 1;
        if (pTo > pFrom) return 'slow-consumer';
        if (pFrom > pTo) return 'fast-consumer';
        return 'same-rate';
    }

    // Badge label for a connection
    function getConnectionBadgeLabel(conn, fromInst, toInst) {
        const type = getConnectionType(fromInst, toInst);
        const depth = computeFifoDepth(fromInst, toInst);
        const db = conn.doubleBuffer ? ' DB' : '';
        if (type === 'slow-consumer') return `FIFO\u00d7${depth}${db}`;
        if (type === 'fast-consumer') return `S&H${db}`;
        if (conn.doubleBuffer) return 'DB';
        return null; // no badge
    }

    function removeConnection(connId) {
        state.connections = state.connections.filter(c => c.id !== connId);
        const pathEl = document.getElementById('conn-' + connId);
        if (pathEl) pathEl.remove();
        const badgeEl = document.getElementById('fifo-badge-' + connId);
        if (badgeEl) badgeEl.remove();
        updateAllUseCaseBounds();
    }

    function renderConnection(conn) {
        const pos = getConnectionEndpoints(conn);
        if (!pos) return;
        const path = document.createElementNS('http://www.w3.org/2000/svg', 'path');
        path.id = 'conn-' + conn.id;
        path.setAttribute('stroke', 'var(--link-color)');
        path.setAttribute('stroke-width', '2');
        path.setAttribute('fill', 'none');
        path.setAttribute('d', bezierPath(pos.x1, pos.y1, pos.x2, pos.y2));
        path.style.pointerEvents = 'stroke';
        path.style.cursor = 'pointer';
        path.addEventListener('contextmenu', (e) => {
            e.preventDefault();
            e.stopPropagation();
            showContextMenu(e.clientX, e.clientY, [
                { label: 'Delete this connection', action: () => removeConnection(conn.id) }
            ]);
        });
        path.addEventListener('click', (e) => {
            e.stopPropagation();
            clearSelection();
            renderConnectionProperties(conn);
        });
        $svg.appendChild(path);

        // Add badge (FIFO×N, S&H, DB) if applicable
        const fromInst = state.instances.find(i => i.id === conn.fromInstanceId);
        const toInst = state.instances.find(i => i.id === conn.toInstanceId);
        const badgeLabel = getConnectionBadgeLabel(conn, fromInst, toInst);
        if (badgeLabel) {
            const connType = getConnectionType(fromInst, toInst);
            const color = connType === 'fast-consumer' ? '#4fc3f7' : '#f9a825';
            const g = document.createElementNS('http://www.w3.org/2000/svg', 'g');
            g.id = 'fifo-badge-' + conn.id;
            g.setAttribute('class', 'fifo-badge');
            const midX = (pos.x1 + pos.x2) / 2;
            const midY = (pos.y1 + pos.y2) / 2;
            const rect = document.createElementNS('http://www.w3.org/2000/svg', 'rect');
            rect.setAttribute('rx', '4');
            rect.setAttribute('ry', '4');
            rect.setAttribute('fill', '#1a1a2e');
            rect.setAttribute('stroke', color);
            rect.setAttribute('stroke-width', '1.5');
            const text = document.createElementNS('http://www.w3.org/2000/svg', 'text');
            text.setAttribute('fill', color);
            text.setAttribute('font-size', '11');
            text.setAttribute('font-weight', '700');
            text.setAttribute('text-anchor', 'middle');
            text.setAttribute('dominant-baseline', 'central');
            text.textContent = badgeLabel;
            text.setAttribute('x', midX);
            text.setAttribute('y', midY);
            const tw = Math.max(28, badgeLabel.length * 8 + 12);
            rect.setAttribute('x', midX - tw / 2);
            rect.setAttribute('y', midY - 9);
            rect.setAttribute('width', tw);
            rect.setAttribute('height', 18);
            g.appendChild(rect);
            g.appendChild(text);
            g.style.pointerEvents = 'none';
            $svg.appendChild(g);
        }
    }

    function getConnectionEndpoints(conn) {
        const fromEl = document.getElementById(conn.fromInstanceId);
        const toEl = document.getElementById(conn.toInstanceId);
        if (!fromEl || !toEl) return null;
        const fromPort = fromEl.querySelectorAll('.port.output')[conn.fromPort];
        const toPort = toEl.querySelectorAll('.port.input')[conn.toPort];
        if (!fromPort || !toPort) return null;
        const fromDot = fromPort.querySelector('.port-dot');
        const toDot = toPort.querySelector('.port-dot');
        const svgRect = $svg.getBoundingClientRect();
        const fr = fromDot.getBoundingClientRect();
        const tr = toDot.getBoundingClientRect();
        return {
            x1: (fr.left + fr.width / 2 - svgRect.left - state.panX) / state.zoom,
            y1: (fr.top + fr.height / 2 - svgRect.top - state.panY) / state.zoom,
            x2: (tr.left + tr.width / 2 - svgRect.left - state.panX) / state.zoom,
            y2: (tr.top + tr.height / 2 - svgRect.top - state.panY) / state.zoom,
        };
    }

    function bezierPath(x1, y1, x2, y2) {
        const dx = Math.abs(x2 - x1) * 0.5;
        return `M${x1},${y1} C${x1 + dx},${y1} ${x2 - dx},${y2} ${x2},${y2}`;
    }

    function updateAllConnections() {
        state.connections.forEach(conn => {
            const pathEl = document.getElementById('conn-' + conn.id);
            if (!pathEl) return;
            const pos = getConnectionEndpoints(conn);
            if (!pos) return;
            pathEl.setAttribute('d', bezierPath(pos.x1, pos.y1, pos.x2, pos.y2));

            // Update or create/remove badge (FIFO×N, S&H, DB)
            const fromInst = state.instances.find(i => i.id === conn.fromInstanceId);
            const toInst = state.instances.find(i => i.id === conn.toInstanceId);
            const badgeLabel = getConnectionBadgeLabel(conn, fromInst, toInst);
            const badgeEl = document.getElementById('fifo-badge-' + conn.id);
            if (badgeLabel) {
                const connType = getConnectionType(fromInst, toInst);
                const color = connType === 'fast-consumer' ? '#4fc3f7' : '#f9a825';
                const midX = (pos.x1 + pos.x2) / 2;
                const midY = (pos.y1 + pos.y2) / 2;
                if (badgeEl) {
                    const text = badgeEl.querySelector('text');
                    const rect = badgeEl.querySelector('rect');
                    text.textContent = badgeLabel;
                    text.setAttribute('x', midX);
                    text.setAttribute('y', midY);
                    text.setAttribute('fill', color);
                    const tw = Math.max(28, badgeLabel.length * 8 + 12);
                    rect.setAttribute('x', midX - tw / 2);
                    rect.setAttribute('y', midY - 9);
                    rect.setAttribute('width', tw);
                    rect.setAttribute('stroke', color);
                }
                // If badge doesn't exist yet, it will be created on next full re-render
            } else if (badgeEl) {
                badgeEl.remove();
            }
        });
    }

    // ============================================================
    // USE CASES (renamed from Pipelines)
    // ============================================================
    function createUseCase(instanceIds) {
        if (instanceIds.length === 0) return;
        const name = `UseCase-${state.useCases.length + 1}`;
        const uc = { id: genId(), name, instanceIds: [...instanceIds], active: false };
        state.useCases.push(uc);
        renderUseCase(uc);
        updateUseCaseBounds(uc);
        clearSelection();
        state.selectedIds.add(uc.id);
        state.selectedType = 'usecase';
        updateSelectionVisuals();
        renderProperties();
    }

    function renderUseCase(uc) {
        const el = document.createElement('div');
        el.className = 'usecase-group';
        el.id = uc.id;

        const header = document.createElement('div');
        header.className = 'usecase-header';
        header.textContent = uc.name;
        el.appendChild(header);

        const order = document.createElement('div');
        order.className = 'usecase-order';
        el.appendChild(order);

        // Click to select + drag
        {
            let plDragging = false;
            let plStartX, plStartY;
            let instanceOrigPositions = [];

            el.addEventListener('mousedown', (e) => {
                if (e.target.closest('.instance-node')) return;
                if (e.button !== 0) return;
                e.stopPropagation();
                clearSelection();
                state.selectedIds.add(uc.id);
                state.selectedType = 'usecase';
                updateSelectionVisuals();
                renderProperties();
                plDragging = true;
                plStartX = e.clientX;
                plStartY = e.clientY;
                instanceOrigPositions = uc.instanceIds.map(iid => {
                    const inst = state.instances.find(i => i.id === iid);
                    return inst ? { inst, origX: inst.x, origY: inst.y } : null;
                }).filter(Boolean);

                const onMove = (ev) => {
                    if (!plDragging) return;
                    const dx = (ev.clientX - plStartX) / state.zoom;
                    const dy = (ev.clientY - plStartY) / state.zoom;
                    instanceOrigPositions.forEach(({ inst, origX, origY }) => {
                        inst.x = origX + dx;
                        inst.y = origY + dy;
                        const instEl = document.getElementById(inst.id);
                        if (instEl) {
                            instEl.style.left = inst.x + 'px';
                            instEl.style.top = inst.y + 'px';
                        }
                    });
                    updateAllConnections();
                    updateUseCaseBounds(uc);
                };

                const onUp = () => {
                    plDragging = false;
                    instanceOrigPositions = [];
                    window.removeEventListener('mousemove', onMove);
                    window.removeEventListener('mouseup', onUp);
                };

                window.addEventListener('mousemove', onMove);
                window.addEventListener('mouseup', onUp);
            });
        }

        // Context menu
        el.addEventListener('contextmenu', (e) => {
            if (e.target.closest('.instance-node')) return;
            e.preventDefault();
            e.stopPropagation();
            showContextMenu(e.clientX, e.clientY, [
                { label: 'Delete Use Case', action: () => deleteUseCase(uc.id) },
                { label: 'Rename', action: () => renameUseCasePrompt(uc) },
            ]);
        });

        $canvas.insertBefore(el, $canvas.firstChild);
    }

    function updateUseCaseBounds(uc) {
        const el = document.getElementById(uc.id);
        if (!el) return;
        const padding = 30;
        let minX = Infinity, minY = Infinity, maxX = -Infinity, maxY = -Infinity;
        uc.instanceIds.forEach(iid => {
            const inst = state.instances.find(i => i.id === iid);
            const instEl = document.getElementById(iid);
            if (!inst || !instEl) return;
            minX = Math.min(minX, inst.x);
            minY = Math.min(minY, inst.y);
            maxX = Math.max(maxX, inst.x + instEl.offsetWidth);
            maxY = Math.max(maxY, inst.y + instEl.offsetHeight);
        });
        if (minX === Infinity) { el.style.display = 'none'; return; }
        el.style.display = '';
        el.style.left = (minX - padding) + 'px';
        el.style.top = (minY - padding) + 'px';
        el.style.width = (maxX - minX + 2 * padding) + 'px';
        el.style.height = (maxY - minY + 2 * padding) + 'px';
        const headerEl = el.querySelector('.usecase-header');
        if (headerEl) headerEl.textContent = `${uc.name} [${uc.active ? 'ACTIVE' : 'INACTIVE'}]`;
    }

    function updateAllUseCaseBounds() {
        state.useCases.forEach(uc => updateUseCaseBounds(uc));
    }

    function deleteUseCase(ucId) {
        const el = document.getElementById(ucId);
        if (el) el.remove();
        state.useCases = state.useCases.filter(u => u.id !== ucId);
        clearSelection();
    }

    function renameUseCasePrompt(uc) {
        const newName = prompt('New Use Case name:', uc.name);
        if (newName && newName.trim()) {
            uc.name = newName.trim();
            updateUseCaseBounds(uc);
            renderProperties();
        }
    }

    function findUseCaseOfInstance(instId) {
        return state.useCases.find(u => u.instanceIds.includes(instId)) || null;
    }

    function findUseCasesOfInstance(instId) {
        return state.useCases.filter(u => u.instanceIds.includes(instId));
    }

    function addInstanceToUseCase(instId, ucId) {
        const uc = state.useCases.find(u => u.id === ucId);
        if (!uc) return;
        if (uc.instanceIds.includes(instId)) return; // already in
        uc.instanceIds.push(instId);
        updateUseCaseBounds(uc);
        renderProperties();
    }

    function removeInstanceFromUseCase(instId, ucId) {
        const uc = state.useCases.find(u => u.id === ucId);
        if (!uc) return;
        uc.instanceIds = uc.instanceIds.filter(id => id !== instId);
        if (uc.instanceIds.length === 0) {
            deleteUseCase(ucId);
        } else {
            updateUseCaseBounds(uc);
        }
        renderProperties();
    }

    document.getElementById('btn-create-usecase').addEventListener('click', () => {
        if (state.selectedIds.size < 1 || state.selectedType !== 'instance') {
            alert('Select at least one instance to create a Use Case.');
            return;
        }
        createUseCase([...state.selectedIds]);
    });

    // ============================================================
    // SELECTION & DELETION
    // ============================================================
    function clearSelection() {
        state.selectedIds.clear();
        state.selectedType = null;
        updateSelectionVisuals();
        $props.innerHTML = '<p class="placeholder">Select an element to view its properties.</p>';
    }

    function toggleSelection(id, type) {
        if (state.selectedType && state.selectedType !== type) clearSelection();
        if (state.selectedIds.has(id)) state.selectedIds.delete(id);
        else state.selectedIds.add(id);
        state.selectedType = type;
        updateSelectionVisuals();
        renderProperties();
    }

    function updateSelectionVisuals() {
        document.querySelectorAll('.instance-node').forEach(el => el.classList.remove('selected'));
        document.querySelectorAll('.usecase-group').forEach(el => el.classList.remove('selected'));
        state.selectedIds.forEach(id => {
            const el = document.getElementById(id);
            if (el) el.classList.add('selected');
        });
    }

    function deleteSelected() {
        if (state.selectedType === 'instance') {
            state.selectedIds.forEach(id => deleteInstance(id));
        } else if (state.selectedType === 'usecase') {
            state.selectedIds.forEach(id => deleteUseCase(id));
        }
        clearSelection();
    }

    function deleteInstance(instId) {
        const toRemove = state.connections.filter(c => c.fromInstanceId === instId || c.toInstanceId === instId);
        toRemove.forEach(c => removeConnection(c.id));
        state.useCases.forEach(u => {
            u.instanceIds = u.instanceIds.filter(id => id !== instId);
        });
        updateAllUseCaseBounds();
        state.instances = state.instances.filter(i => i.id !== instId);
        const el = document.getElementById(instId);
        if (el) el.remove();
    }

    document.getElementById('btn-delete').addEventListener('click', deleteSelected);
    document.addEventListener('keydown', (e) => {
        if (e.key === 'Delete' && state.selectedIds.size > 0) {
            deleteSelected();
        }
    });

    // ============================================================
    // PROPERTIES PANEL
    // ============================================================
    function renderProperties() {
        if (state.selectedIds.size === 0) {
            $props.innerHTML = '<p class="placeholder">Select an element to view its properties.</p>';
            return;
        }
        if (state.selectedIds.size > 1) {
            $props.innerHTML = `<p class="placeholder">${state.selectedIds.size} elements selected</p>`;
            return;
        }
        const id = [...state.selectedIds][0];
        if (state.selectedType === 'instance') {
            const inst = state.instances.find(i => i.id === id);
            if (!inst) return;
            renderInstanceProperties(inst);
        } else if (state.selectedType === 'usecase') {
            const uc = state.useCases.find(u => u.id === id);
            if (!uc) return;
            renderUseCaseProperties(uc);
        }
    }

    function renderInstanceProperties(inst) {
        const mod = state.modules.find(m => m.id === inst.moduleId);
        const connIn = state.connections.filter(c => c.toInstanceId === inst.id);
        const connOut = state.connections.filter(c => c.fromInstanceId === inst.id);

        let html = `
            <div class="prop-group">
                <label>Nom</label>
                <input type="text" id="prop-name" value="${esc(inst.name)}">
            </div>
            <div class="prop-group">
                <label>Module</label>
                <span class="prop-value">${esc(inst.moduleName)} ${mod ? asilBadgeHtml(mod.asil_level) : ''}</span>
            </div>
            ${mod ? `<div class="prop-group"><label>UUID</label><span class="prop-value" style="font-size:10px">${mod.uuid}</span></div>` : ''}
            <div class="prop-group">
                <label>Activation</label>
                <select id="prop-activation">
                    <option value="PERIODIC" ${inst.activation === 'PERIODIC' ? 'selected' : ''}>PERIODIC</option>
                    <option value="DATA_DRIVEN" ${inst.activation === 'DATA_DRIVEN' ? 'selected' : ''}>DATA_DRIVEN</option>
                    <option value="SPORADIC" ${inst.activation === 'SPORADIC' ? 'selected' : ''}>SPORADIC</option>
                </select>
            </div>
            <div class="prop-group" id="prop-period-group" style="${inst.activation === 'PERIODIC' ? '' : 'display:none'}">
                <label>Period (µs)</label>
                <input type="number" id="prop-period" value="${inst.period_us}" min="0">
            </div>
            <div class="prop-group" id="prop-sporadic-group" style="${inst.activation === 'SPORADIC' ? '' : 'display:none'}">
                <label>Min Interarrival (µs)</label>
                <input type="number" id="prop-interarrival" value="${inst.min_interarrival_us}" min="0">
            </div>
            <div class="prop-group">
                <label>Placement</label>
                <span class="prop-value">Core mask: 0x${inst.allowed_cores_mask.toString(16).toUpperCase()} | Preferred: ${inst.preferred_core}</span>
            </div>
            <div class="prop-group">
                <label>State</label>
                <select id="prop-state">
                    <option value="INACTIVE" ${inst.state === 'INACTIVE' ? 'selected' : ''}>INACTIVE</option>
                    <option value="STARTING" ${inst.state === 'STARTING' ? 'selected' : ''}>STARTING</option>
                    <option value="ACTIVE" ${inst.state === 'ACTIVE' ? 'selected' : ''}>ACTIVE</option>
                    <option value="STOPPING" ${inst.state === 'STOPPING' ? 'selected' : ''}>STOPPING</option>
                </select>
            </div>`;

        if (mod) {
            html += `
            <div class="prop-group">
                <label>Timing (module)</label>
                <span class="prop-value">WCET: ${mod.wcet_us}µs | BCET: ${mod.bcet_us}µs | Typical: ${mod.typical_us}µs</span>
            </div>`;
        }

        // Input port configs
        if (inst.input_configs && inst.input_configs.length > 0) {
            html += `<div class="prop-group"><label>Input Port Configs</label>`;
            inst.input_configs.forEach((pc, i) => {
                html += `<div style="font-size:11px;color:var(--text-muted);padding:2px 0;border-bottom:1px solid var(--border)">
                    <strong>${esc(pc.port_name)}</strong> — fifo:${pc.fifo_depth} | ${pc.mandatory ? 'mandatory' : 'optional'} | ${pc.mode} | overflow:${pc.overflow_policy}
                </div>`;
            });
            html += `</div>`;
        }

        // Output port configs
        if (inst.output_configs && inst.output_configs.length > 0) {
            html += `<div class="prop-group"><label>Output Port Configs</label>`;
            inst.output_configs.forEach((pc) => {
                html += `<div style="font-size:11px;color:var(--text-muted);padding:2px 0;border-bottom:1px solid var(--border)">
                    <strong>${esc(pc.port_name)}</strong> — burst:${pc.burst_size} | min_depth:${pc.suggested_min_depth}
                </div>`;
            });
            html += `</div>`;
        }

        // Connections
        html += `
            <div class="prop-group">
                <label>Connexions entrantes (${connIn.length})</label>
                <ul class="connections-list">
                    ${connIn.map(c => {
                        const from = state.instances.find(i => i.id === c.fromInstanceId);
                        return `<li><span>${esc(from?.name || '?')} [out-${c.fromPort}] → in-${c.toPort}</span><button onclick="window._removeConn('${c.id}')">&times;</button></li>`;
                    }).join('')}
                </ul>
            </div>
            <div class="prop-group">
                <label>Connexions sortantes (${connOut.length})</label>
                <ul class="connections-list">
                    ${connOut.map(c => {
                        const to = state.instances.find(i => i.id === c.toInstanceId);
                        return `<li><span>out-${c.fromPort} → ${esc(to?.name || '?')} [in-${c.toPort}]</span><button onclick="window._removeConn('${c.id}')">&times;</button></li>`;
                    }).join('')}
                </ul>
            </div>`;

        $props.innerHTML = html;

        // Bind events
        document.getElementById('prop-name').addEventListener('change', (e) => {
            inst.name = e.target.value;
            const header = document.getElementById(inst.id)?.querySelector('.node-header span');
            if (header) header.textContent = inst.name;
        });
        document.getElementById('prop-activation').addEventListener('change', (e) => {
            inst.activation = e.target.value;
            document.getElementById('prop-period-group').style.display = inst.activation === 'PERIODIC' ? '' : 'none';
            document.getElementById('prop-sporadic-group').style.display = inst.activation === 'SPORADIC' ? '' : 'none';
            // Update node header
            const nodeType = document.getElementById(inst.id)?.querySelector('.node-type');
            if (nodeType) {
                const asilLevel = mod ? mod.asil_level : 'QM';
                nodeType.innerHTML = `${asilBadgeHtml(asilLevel)} ${inst.activation}`;
            }
        });
        const periodEl = document.getElementById('prop-period');
        if (periodEl) periodEl.addEventListener('change', (e) => { inst.period_us = parseInt(e.target.value) || 0; updateAllConnections(); });
        const interarrivalEl = document.getElementById('prop-interarrival');
        if (interarrivalEl) interarrivalEl.addEventListener('change', (e) => { inst.min_interarrival_us = parseInt(e.target.value) || 0; });
        document.getElementById('prop-state').addEventListener('change', (e) => { inst.state = e.target.value; });
    }

    function renderConnectionProperties(conn) {
        const from = state.instances.find(i => i.id === conn.fromInstanceId);
        const to = state.instances.find(i => i.id === conn.toInstanceId);
        const depth = computeFifoDepth(from, to);
        const connType = getConnectionType(from, to);

        let rateHtml = '';
        if (connType === 'slow-consumer') {
            rateHtml = `<div class="prop-group">
                <label>Rate Transition</label>
                <span class="prop-value" style="color:#f9a825;font-weight:600;">FIFO depth ${depth} (${from?.period_us || '?'}µs \u2192 ${to?.period_us || '?'}µs)</span>
               </div>`;
        } else if (connType === 'fast-consumer') {
            rateHtml = `<div class="prop-group">
                <label>Rate Transition</label>
                <span class="prop-value" style="color:#4fc3f7;font-weight:600;">Sample & Hold (${from?.period_us || '?'}µs \u2192 ${to?.period_us || '?'}µs)</span>
               </div>`;
        }

        const dbChecked = conn.doubleBuffer ? 'checked' : '';
        $props.innerHTML = `
            <div class="prop-group">
                <label>Connexion</label>
                <span class="prop-value">${esc(from?.name || '?')} [out-${conn.fromPort}]</span>
                <span class="prop-value">\u2192 ${esc(to?.name || '?')} [in-${conn.toPort}]</span>
            </div>
            ${rateHtml}
            <div class="prop-group">
                <label style="display:flex;align-items:center;gap:6px;cursor:pointer;">
                    <input type="checkbox" id="prop-conn-db" ${dbChecked}>
                    Double Buffering
                </label>
                <span class="prop-value" style="font-size:11px;color:#888;">+1 period latency, enables parallel execution</span>
            </div>
            <div class="prop-group">
                <button onclick="window._removeConn('${conn.id}')" style="background:var(--accent);color:#fff;border:none;padding:6px 12px;border-radius:4px;cursor:pointer;">Delete</button>
            </div>
        `;

        document.getElementById('prop-conn-db').addEventListener('change', (e) => {
            conn.doubleBuffer = e.target.checked;
            updateAllConnections();
        });
    }

    function renderUseCaseProperties(uc) {
        const instances = uc.instanceIds.map(id => state.instances.find(i => i.id === id)).filter(Boolean);
        $props.innerHTML = `
            <div class="prop-group">
                <label>Use Case Name</label>
                <input type="text" id="prop-uc-name" value="${esc(uc.name)}">
            </div>
            <div class="prop-group">
                <label>Active</label>
                <select id="prop-uc-active">
                    <option value="false" ${!uc.active ? 'selected' : ''}>INACTIVE</option>
                    <option value="true" ${uc.active ? 'selected' : ''}>ACTIVE</option>
                </select>
            </div>
            <div class="prop-group">
                <label>Instances (${instances.length})</label>
                <ul class="connections-list">
                    ${instances.map(i => {
                        const mod = state.modules.find(m => m.id === i.moduleId);
                        return `<li>${esc(i.name)} ${mod ? asilBadgeHtml(mod.asil_level) : ''}</li>`;
                    }).join('')}
                </ul>
            </div>
        `;
        document.getElementById('prop-uc-name').addEventListener('change', (e) => {
            uc.name = e.target.value;
            updateUseCaseBounds(uc);
        });
        document.getElementById('prop-uc-active').addEventListener('change', (e) => {
            uc.active = e.target.value === 'true';
            updateUseCaseBounds(uc);
        });
    }

    // Global helper for inline onclick
    window._removeConn = (connId) => {
        removeConnection(connId);
        renderProperties();
    };

    // ============================================================
    // CONTEXT MENU
    // ============================================================
    function showContextMenu(x, y, items) {
        const ul = $ctxMenu.querySelector('ul');
        ul.innerHTML = '';
        items.forEach(item => {
            if (item.separator) {
                const li = document.createElement('li');
                li.className = 'separator';
                ul.appendChild(li);
                return;
            }
            const li = document.createElement('li');
            li.textContent = item.label;
            li.addEventListener('click', () => { hideContextMenu(); item.action(); });
            ul.appendChild(li);
        });
        $ctxMenu.style.left = x + 'px';
        $ctxMenu.style.top = y + 'px';
        $ctxMenu.classList.remove('hidden');
    }

    function hideContextMenu() {
        $ctxMenu.classList.add('hidden');
    }

    document.addEventListener('click', () => hideContextMenu());
    document.addEventListener('contextmenu', (e) => {
        if (!e.target.closest('.instance-node') && !e.target.closest('.usecase-group') && !e.target.closest('#connections-svg path')) {
            hideContextMenu();
        }
    });

    // ============================================================
    // EXPORT / IMPORT
    // ============================================================
    const MCP_HTTP = 'http://localhost:6590';

    document.getElementById('btn-save-topology').addEventListener('click', async () => {
        const defaultName = state.topologyName || 'topology';
        const name = prompt('Topology name:', defaultName);
        if (name === null) return; // cancelled
        const trimmed = name.trim() || 'topology';
        state.topologyName = trimmed;

        const filename = trimmed.endsWith('.json') ? trimmed : trimmed + '.json';

        const data = {
            version: '2.0',
            name: trimmed,
            savedAt: new Date().toISOString(),
            modules: state.modules,
            instances: state.instances,
            connections: state.connections,
            useCases: state.useCases,
            nextInstanceCounters: state.nextInstanceCounters,
        };
        const json = JSON.stringify(data, null, 2);

        try {
            const resp = await fetch(`${MCP_HTTP}/files/${encodeURIComponent(filename)}`, {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: json,
            });
            if (!resp.ok) throw new Error(`HTTP ${resp.status}`);
            showToast(`Topology saved as "${filename}"`);
        } catch (err) {
            console.warn('[Save] HTTP save failed, falling back to download:', err.message);
            const blob = new Blob([json], { type: 'application/json' });
            const url = URL.createObjectURL(blob);
            const a = document.createElement('a');
            a.href = url;
            a.download = filename;
            a.click();
            URL.revokeObjectURL(url);
            showToast('Topology downloaded (server unavailable)');
        }
    });

    const $loadOverlay = document.getElementById('load-modal-overlay');
    const $loadFileList = document.getElementById('load-file-list');
    let cachedTopologyFiles = [];
    let selectedLoadFile = null;

    // Pre-fetch topology file list from server
    async function fetchTopologyFiles() {
        try {
            const resp = await fetch(`${MCP_HTTP}/files/`);
            if (!resp.ok) throw new Error(`HTTP ${resp.status}`);
            const data = await resp.json();
            // Filter: keep only topology files (have "version" field pattern), exclude edf-config, viewer-data
            cachedTopologyFiles = data.files.filter(f =>
                f.name.endsWith('.json') &&
                !f.name.startsWith('edf-config') &&
                !f.name.startsWith('viewer-data')
            );
        } catch {
            cachedTopologyFiles = [];
        }
    }
    fetchTopologyFiles();

    function renderLoadFileList() {
        if (cachedTopologyFiles.length === 0) {
            $loadFileList.innerHTML = '<div class="load-file-empty">No topologies on server</div>';
            return;
        }
        $loadFileList.innerHTML = '';
        cachedTopologyFiles.forEach(f => {
            const item = document.createElement('div');
            item.className = 'load-file-item';
            item.innerHTML = `<span class="load-file-item-name">${f.name}</span><span class="load-file-item-right"><span class="load-file-item-size">${(f.size / 1024).toFixed(1)} KB</span><button class="load-file-delete" title="Delete">&times;</button></span>`;
            item.querySelector('.load-file-item-name').addEventListener('click', () => {
                loadFromServer(f.name);
            });
            item.querySelector('.load-file-delete').addEventListener('click', async (e) => {
                e.stopPropagation();
                if (!confirm(`Delete "${f.name}"?`)) return;
                try {
                    const resp = await fetch(`${MCP_HTTP}/files/${encodeURIComponent(f.name)}`, { method: 'DELETE' });
                    if (!resp.ok) throw new Error(`HTTP ${resp.status}`);
                    await fetchTopologyFiles();
                    renderLoadFileList();
                    showToast(`Deleted: ${f.name}`);
                } catch (err) {
                    alert('Error: ' + err.message);
                }
            });
            $loadFileList.appendChild(item);
        });
    }

    async function loadFromServer(filename) {
        try {
            const resp = await fetch(`${MCP_HTTP}/files/${encodeURIComponent(filename)}`);
            if (!resp.ok) throw new Error(`HTTP ${resp.status}`);
            const data = await resp.json();
            loadTopology(data);
            $loadOverlay.classList.add('hidden');
            showToast(`Loaded: ${filename}`);
        } catch (err) {
            alert('Loading error: ' + err.message);
        }
    }

    document.getElementById('btn-import').addEventListener('click', async () => {
        await fetchTopologyFiles();
        renderLoadFileList();
        $loadOverlay.classList.remove('hidden');
    });

    document.getElementById('btn-load-cancel').addEventListener('click', () => {
        $loadOverlay.classList.add('hidden');
    });
    $loadOverlay.addEventListener('click', (e) => {
        if (e.target === $loadOverlay) $loadOverlay.classList.add('hidden');
    });

    document.getElementById('btn-load-local').addEventListener('click', () => {
        $loadOverlay.classList.add('hidden');
        document.getElementById('file-import').click();
    });

    document.getElementById('file-import').addEventListener('change', (e) => {
        const file = e.target.files[0];
        if (!file) return;
        const reader = new FileReader();
        reader.onload = (ev) => {
            try {
                const data = JSON.parse(ev.target.result);
                loadTopology(data);
            } catch (err) {
                alert('Import error: ' + err.message);
            }
        };
        reader.readAsText(file);
        e.target.value = '';
    });

    function migrateV1(data) {
        // Migrate old format modules to new format
        if (data.modules) {
            data.modules = data.modules.map(mod => {
                if (mod.input_ports) return mod; // already new format
                const input_ports = [];
                for (let i = 0; i < (mod.inputs || 0); i++) input_ports.push({ port_name: `in-${i}`, data_type: '', sample_size_bytes: 0 });
                const output_ports = [];
                for (let i = 0; i < (mod.outputs || 0); i++) output_ports.push({ port_name: `out-${i}`, data_type: '', sample_size_bytes: 0 });
                return {
                    ...mod,
                    uuid: mod.uuid || genUUID(),
                    version: mod.version || 1,
                    input_ports,
                    output_ports,
                    wcet_us: (mod.execTime || 0) * 1000,
                    bcet_us: 0,
                    typical_us: (mod.execTime || 0) * 500,
                    stack_size_bytes: 4096,
                    static_mem_bytes: 1024,
                    requires_fpu: false,
                    requires_gpu: false,
                    asil_level: 'QM',
                };
            });
        }
        // Migrate old instances
        if (data.instances) {
            data.instances = data.instances.map(inst => {
                if (inst.activation) return inst; // already new format
                const mod = (data.modules || []).find(m => m.id === inst.moduleId);
                return {
                    ...inst,
                    instance_id: inst.instance_id || 0,
                    activation: 'PERIODIC',
                    period_us: (inst.period || 0) * 1000,
                    min_interarrival_us: 0,
                    input_configs: (mod ? mod.input_ports : []).map(p => ({
                        port_name: p.port_name, fifo_depth: 4, mandatory: true, mode: 'ALL', consume_count: 1, overflow_policy: 'DROP_OLDEST',
                    })),
                    output_configs: (mod ? mod.output_ports : []).map(p => ({
                        port_name: p.port_name, burst_size: 1, suggested_min_depth: 2,
                    })),
                    allowed_cores_mask: 0xFFFF,
                    preferred_core: 0,
                    affinity_group: '',
                    state: 'INACTIVE',
                };
            });
        }
        // Migrate pipelines → useCases
        if (data.pipelines && !data.useCases) {
            data.useCases = data.pipelines.map(pl => ({
                id: pl.id, name: pl.name, instanceIds: pl.instanceIds, active: false,
            }));
        }
        return data;
    }

    function loadTopology(data) {
        // Auto-migrate old format
        if (data.version !== '2.0') data = migrateV1(data);

        state.topologyName = data.name || '';

        $canvas.innerHTML = '';
        $svg.innerHTML = '';
        state.modules = data.modules || [];
        state.instances = [];
        state.connections = [];
        state.useCases = [];
        state.nextInstanceCounters = data.nextInstanceCounters || {};

        const allIds = [...(data.modules || []), ...(data.instances || []), ...(data.connections || []), ...(data.useCases || [])]
            .map(o => o.id).filter(Boolean);
        allIds.forEach(id => {
            const num = parseInt(id.replace('id-', ''));
            if (!isNaN(num) && num >= idCounter) idCounter = num + 1;
        });

        renderModuleList();

        (data.instances || []).forEach(inst => {
            state.instances.push(inst);
            renderInstance(inst);
        });

        (data.connections || []).forEach(conn => {
            state.connections.push(conn);
            renderConnection(conn);
        });

        (data.useCases || []).forEach(uc => {
            state.useCases.push(uc);
            renderUseCase(uc);
            updateUseCaseBounds(uc);
        });

        clearSelection();
    }

    // ============================================================
    // EDF SCHEDULING GENERATION (instances-based)
    // ============================================================
    const $edfOverlay = document.getElementById('edf-modal-overlay');

    document.getElementById('btn-generate-edf').addEventListener('click', () => {
        if (state.instances.length === 0) {
            alert('No instances on the canvas.');
            return;
        }
        openEdfModal();
    });

    function openEdfModal() {
        const listEl = document.getElementById('edf-pipeline-list');
        listEl.innerHTML = '';
        state.instances.forEach(inst => {
            const mod = state.modules.find(m => m.id === inst.moduleId);
            const wcet = mod ? mod.wcet_us : 0;
            const period = inst.activation === 'PERIODIC' ? inst.period_us : (inst.min_interarrival_us || 0);
            const div = document.createElement('div');
            div.className = 'edf-pipeline-item';
            div.innerHTML = `
                <span class="edf-pl-name">${esc(inst.name)}</span>
                <span class="edf-pl-info">${inst.activation} | period: ${period}µs | WCET: ${wcet}µs ${mod ? asilBadgeHtml(mod.asil_level) : ''}</span>
            `;
            listEl.appendChild(div);
        });
        $edfOverlay.classList.remove('hidden');
    }

    document.getElementById('btn-edf-cancel').addEventListener('click', () => {
        $edfOverlay.classList.add('hidden');
    });
    $edfOverlay.addEventListener('click', (e) => {
        if (e.target === $edfOverlay) $edfOverlay.classList.add('hidden');
    });

    document.getElementById('btn-edf-generate').addEventListener('click', async () => {
        const tickPeriod = parseInt(document.getElementById('edf-tick-period').value) || 1;
        const simDuration = parseInt(document.getElementById('edf-sim-duration').value) || 1000;
        const numCores = parseInt(document.getElementById('edf-num-cores').value) || 1;
        const fixedPartitioning = document.getElementById('edf-fixed-part').checked;
        const chainConstraints = document.getElementById('edf-chain-constraints').checked;

        // Build processes from instances (not pipelines)
        const processes = state.instances.map(inst => {
            const mod = state.modules.find(m => m.id === inst.moduleId);
            const wcet_us = mod ? mod.wcet_us : 0;
            const period_us = inst.activation === 'PERIODIC' ? inst.period_us :
                              inst.activation === 'SPORADIC' ? inst.min_interarrival_us : 10000;

            const proc = {
                name: inst.name,
                period_ms: Math.max(1, Math.round(period_us / 1000)),
                cpu_time_ms: Math.max(1, Math.round(wcet_us / 1000)),
                priority: 0,
                pinned_core: null,
            };

            if (chainConstraints) {
                // Find upstream instances via connections
                const incomingConns = state.connections.filter(c => c.toInstanceId === inst.id);
                const deps = incomingConns
                    .map(c => {
                        const fromInst = state.instances.find(i => i.id === c.fromInstanceId);
                        return fromInst ? fromInst.name : null;
                    })
                    .filter(Boolean);
                if (deps.length > 0) proc.dependencies = deps;

                // Collect double-buffered dependencies
                const dbDeps = incomingConns
                    .filter(c => c.doubleBuffer)
                    .map(c => {
                        const fromInst = state.instances.find(i => i.id === c.fromInstanceId);
                        return fromInst ? fromInst.name : null;
                    })
                    .filter(Boolean);
                if (dbDeps.length > 0) proc.double_buffer_deps = dbDeps;
            }

            return proc;
        });

        // Build use_cases array: each use case maps to instance names
        const use_cases = state.useCases.map(uc => ({
            name: uc.name,
            active: uc.active,
            process_names: uc.instanceIds
                .map(iid => state.instances.find(i => i.id === iid))
                .filter(Boolean)
                .map(i => i.name),
        }));

        const edfConfig = {
            tick_period_ms: tickPeriod,
            simulation_duration_ms: simDuration,
            num_cores: numCores,
            fixed_partitioning: fixedPartitioning,
            processes,
            use_cases,
        };

        const json = JSON.stringify(edfConfig, null, 2);

        // Save EDF config to shared directory via HTTP
        const saveName = document.getElementById('edf-save-path').value.trim() || 'edf-config.json';
        // Extract just the filename (strip any path prefix like "Topologies/")
        const filename = saveName.split('/').pop();
        try {
            await fetch(`${MCP_HTTP}/files/${encodeURIComponent(filename)}`, {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: json,
            });
        } catch (err) {
            console.warn('[EDF] HTTP save failed:', err.message);
        }

        // Also save the current topology as topology.json so the viewer can load it
        try {
            const topoData = {
                version: '2.0',
                name: state.topologyName || 'topology',
                savedAt: new Date().toISOString(),
                modules: state.modules,
                instances: state.instances,
                connections: state.connections,
                useCases: state.useCases,
                nextInstanceCounters: state.nextInstanceCounters,
            };
            await fetch(`${MCP_HTTP}/files/topology.json`, {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify(topoData, null, 2),
            });
        } catch (err) {
            console.warn('[EDF] Topology save failed:', err.message);
        }

        // Send config to MCP server in-memory for the simulator to fetch via /edf-config
        if (window._mcpSendEdfConfig) {
            window._mcpSendEdfConfig(edfConfig);
        }

        // Open the EDF simulator in a new tab with autoload
        window.open('http://localhost:8080/simulator/?autoload=http://localhost:6590/edf-config', '_blank');

        $edfOverlay.classList.add('hidden');
    });

    // ============================================================
    // UTILS
    // ============================================================
    function showToast(msg, duration = 2500) {
        let t = document.getElementById('toast-msg');
        if (!t) {
            t = document.createElement('div');
            t.id = 'toast-msg';
            t.style.cssText = 'position:fixed;bottom:24px;left:50%;transform:translateX(-50%);background:#16213e;color:#e0e0e0;border:1px solid #533483;border-radius:6px;padding:10px 20px;font-size:14px;z-index:9999;opacity:0;transition:opacity 0.3s;';
            document.body.appendChild(t);
        }
        t.textContent = msg;
        t.style.opacity = '1';
        clearTimeout(t._timer);
        t._timer = setTimeout(() => { t.style.opacity = '0'; }, duration);
    }

    function esc(str) {
        const d = document.createElement('div');
        d.textContent = str || '';
        return d.innerHTML;
    }

    // ============================================================
    // PUBLIC API (for MCP WebSocket bridge)
    // ============================================================
    window.topologyAPI = {
        // State access
        getState: () => state,

        // Module operations
        addModule,
        removeModule,

        // Instance operations
        createInstance: (moduleId, x, y) => {
            const mod = state.modules.find(m => m.id === moduleId);
            if (!mod) throw new Error(`Module not found: ${moduleId}`);
            return createInstance(mod, x, y);
        },
        deleteInstance,

        // Connection operations
        addConnection,
        removeConnection,

        // Property updates (for MCP sync)
        setInstanceProperties: (instId, props) => {
            const inst = state.instances.find(i => i.id === instId);
            if (!inst) throw new Error(`Instance not found: ${instId}`);
            if (props.activation !== undefined) inst.activation = props.activation;
            if (props.period_us !== undefined) inst.period_us = props.period_us;
            if (props.min_interarrival_us !== undefined) inst.min_interarrival_us = props.min_interarrival_us;
            if (props.name !== undefined) { inst.name = props.name; }
            if (props.allowed_cores_mask !== undefined) inst.allowed_cores_mask = props.allowed_cores_mask;
            if (props.preferred_core !== undefined) inst.preferred_core = props.preferred_core;
            if (props.affinity_group !== undefined) inst.affinity_group = props.affinity_group;
            if (props.state !== undefined) inst.state = props.state;
            if (props.x !== undefined) inst.x = props.x;
            if (props.y !== undefined) inst.y = props.y;
            // Re-render the instance node
            const el = document.getElementById(instId);
            if (el) {
                const header = el.querySelector('.instance-header span');
                if (header && props.name !== undefined) header.textContent = inst.name;
                if (props.x !== undefined || props.y !== undefined) {
                    el.style.left = inst.x + 'px';
                    el.style.top = inst.y + 'px';
                }
            }
            updateAllConnections();
            renderProperties();
            return inst;
        },
        setConnectionProperties: (connId, props) => {
            const conn = state.connections.find(c => c.id === connId);
            if (!conn) throw new Error(`Connection not found: ${connId}`);
            if (props.doubleBuffer !== undefined) conn.doubleBuffer = props.doubleBuffer;
            updateAllConnections();
            return conn;
        },

        // Use Case operations
        createUseCase,
        deleteUseCase,
        renameUseCase: (ucId, newName) => {
            const uc = state.useCases.find(u => u.id === ucId);
            if (!uc) throw new Error(`Use Case not found: ${ucId}`);
            uc.name = newName.trim();
            updateUseCaseBounds(uc);
            renderProperties();
            return { id: uc.id, name: uc.name };
        },

        // Use Case analysis
        findUseCaseOfInstance,
        findUseCasesOfInstance,
        addInstanceToUseCase,
        removeInstanceFromUseCase,

        // Topology IO
        loadTopology,
        clearCanvas: () => {
            loadTopology({ version: '2.0', modules: [], instances: [], connections: [], useCases: [], nextInstanceCounters: {} });
        },

        // EDF generation (returns JSON object, no file dialog)
        generateEdfConfig: (tickPeriod, simDuration, numCores, fixedPartitioning, chainConstraints) => {
            const processes = state.instances.map(inst => {
                const mod = state.modules.find(m => m.id === inst.moduleId);
                const wcet_us = mod ? mod.wcet_us : 0;
                const period_us = inst.activation === 'PERIODIC' ? inst.period_us :
                                  inst.activation === 'SPORADIC' ? inst.min_interarrival_us : 10000;
                const proc = {
                    name: inst.name,
                    period_ms: Math.max(1, Math.round(period_us / 1000)),
                    cpu_time_ms: Math.max(1, Math.round(wcet_us / 1000)),
                    priority: 0,
                    pinned_core: null,
                };
                if (chainConstraints) {
                    const deps = state.connections
                        .filter(c => c.toInstanceId === inst.id)
                        .map(c => {
                            const fromInst = state.instances.find(i => i.id === c.fromInstanceId);
                            return fromInst ? fromInst.name : null;
                        }).filter(Boolean);
                    if (deps.length > 0) proc.dependencies = deps;
                }
                return proc;
            });
            const use_cases = state.useCases.map(uc => ({
                name: uc.name,
                active: uc.active,
                process_names: uc.instanceIds
                    .map(iid => state.instances.find(i => i.id === iid))
                    .filter(Boolean)
                    .map(i => i.name),
            }));
            return {
                tick_period_ms: tickPeriod,
                simulation_duration_ms: simDuration,
                num_cores: numCores,
                fixed_partitioning: fixedPartitioning,
                processes,
                use_cases,
            };
        },
    };

    // ============================================================
    // INIT
    // ============================================================
    initDefaults();
    applyTransform();

})();
