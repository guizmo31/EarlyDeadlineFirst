// ============================================================
// WebSocket Client for MCP Bridge
// Connects to the MCP server's WebSocket and dispatches commands
// to window.topologyAPI, returning results back.
// ============================================================

(function initMcpWebSocket() {
    'use strict';

    const WS_PORT = 6589;
    const WS_URL = `ws://localhost:${WS_PORT}`;
    let ws = null;
    let api = null;

    // Command dispatcher: maps command names to topologyAPI calls
    const COMMANDS = {
        // Module management (new ModuleClass model)
        add_module: (p) => {
            const mod = api.addModule(p);
            if (!mod) throw new Error('Failed to add module (empty name?)');
            return mod;
        },
        remove_module: (p) => {
            api.removeModule(p.moduleId);
            return { removed: p.moduleId };
        },
        list_modules: () => {
            return api.getState().modules;
        },

        // Instance operations (new ModuleInstance model)
        create_instance: (p) => {
            const inst = api.createInstance(p.moduleId, p.x ?? 200, p.y ?? 200);
            return {
                id: inst.id, moduleId: inst.moduleId, moduleName: inst.moduleName,
                name: inst.name, instance_id: inst.instance_id,
                x: inst.x, y: inst.y,
                activation: inst.activation, period_us: inst.period_us,
                min_interarrival_us: inst.min_interarrival_us,
                input_configs: inst.input_configs,
                output_configs: inst.output_configs,
                allowed_cores_mask: inst.allowed_cores_mask,
                preferred_core: inst.preferred_core,
                affinity_group: inst.affinity_group,
                state: inst.state,
            };
        },
        delete_instance: (p) => {
            api.deleteInstance(p.instanceId);
            return { deleted: p.instanceId };
        },
        list_instances: () => {
            return api.getState().instances.map(i => ({
                id: i.id, moduleId: i.moduleId, moduleName: i.moduleName,
                name: i.name, instance_id: i.instance_id,
                x: i.x, y: i.y,
                activation: i.activation, period_us: i.period_us,
                min_interarrival_us: i.min_interarrival_us,
                input_configs: i.input_configs,
                output_configs: i.output_configs,
                allowed_cores_mask: i.allowed_cores_mask,
                preferred_core: i.preferred_core,
                affinity_group: i.affinity_group,
                state: i.state,
            }));
        },

        // Instance property updates (from MCP server)
        set_instance_properties: (p) => {
            return api.setInstanceProperties(p.instanceId, p);
        },

        // Connections
        add_connection: (p) => {
            api.addConnection(p.fromInstanceId, p.fromPort, p.toInstanceId, p.toPort);
            const state = api.getState();
            const conn = state.connections.find(c =>
                c.fromInstanceId === p.fromInstanceId && c.fromPort === p.fromPort &&
                c.toInstanceId === p.toInstanceId && c.toPort === p.toPort
            );
            return conn || { created: true };
        },
        remove_connection: (p) => {
            api.removeConnection(p.connectionId);
            return { removed: p.connectionId };
        },
        list_connections: () => {
            return api.getState().connections;
        },
        set_connection_properties: (p) => {
            return api.setConnectionProperties(p.connectionId, p);
        },

        // Use Cases (renamed from Pipelines)
        create_usecase: (p) => {
            api.createUseCase(p.instanceIds);
            const state = api.getState();
            const uc = state.useCases[state.useCases.length - 1];
            return {
                id: uc.id, name: uc.name, instanceIds: uc.instanceIds,
                active: uc.active,
            };
        },
        delete_usecase: (p) => {
            api.deleteUseCase(p.useCaseId);
            return { deleted: p.useCaseId };
        },
        rename_usecase: (p) => {
            return api.renameUseCase(p.useCaseId, p.newName);
        },
        list_usecases: () => {
            const state = api.getState();
            return state.useCases.map(uc => ({
                id: uc.id, name: uc.name, instanceIds: uc.instanceIds,
                active: uc.active,
            }));
        },
        add_instance_to_usecase: (p) => {
            api.addInstanceToUseCase(p.instanceId, p.useCaseId);
            return { added: p.instanceId, to: p.useCaseId };
        },
        remove_instance_from_usecase: (p) => {
            api.removeInstanceFromUseCase(p.instanceId, p.useCaseId);
            return { removed: p.instanceId, from: p.useCaseId };
        },

        // State
        get_topology: () => {
            const s = api.getState();
            return {
                modules: s.modules,
                instances: s.instances.map(i => ({
                    id: i.id, moduleId: i.moduleId, moduleName: i.moduleName,
                    name: i.name, instance_id: i.instance_id,
                    x: i.x, y: i.y,
                    activation: i.activation, period_us: i.period_us,
                    min_interarrival_us: i.min_interarrival_us,
                    input_configs: i.input_configs,
                    output_configs: i.output_configs,
                    allowed_cores_mask: i.allowed_cores_mask,
                    preferred_core: i.preferred_core,
                    affinity_group: i.affinity_group,
                    state: i.state,
                })),
                connections: s.connections,
                useCases: s.useCases.map(uc => ({
                    id: uc.id, name: uc.name, instanceIds: uc.instanceIds,
                    active: uc.active,
                })),
            };
        },
        load_topology: (p) => {
            api.loadTopology(p.data);
            return { loaded: true };
        },
        clear_canvas: () => {
            api.clearCanvas();
            return { cleared: true };
        },
        generate_edf_config: (p) => {
            return api.generateEdfConfig(
                p.tickPeriod ?? 1,
                p.simDuration ?? 1000,
                p.numCores ?? 1,
                p.fixedPartitioning ?? false,
                p.chainConstraints ?? false
            );
        },
    };

    // Expose a function for app.js to send EDF config to the MCP server
    window._mcpSendEdfConfig = function(config) {
        if (ws && ws.readyState === WebSocket.OPEN) {
            ws.send(JSON.stringify({ command: 'store_edf_config', config }));
        }
    };

    // Expose a function for app.js to save a file via the MCP server
    window._mcpSaveFile = function(filePath, content) {
        return new Promise((resolve, reject) => {
            if (!ws || ws.readyState !== WebSocket.OPEN) {
                reject(new Error('MCP server not connected'));
                return;
            }
            const id = `save-${Date.now()}`;
            let settled = false;
            const handler = (event) => {
                let msg;
                try { msg = JSON.parse(event.data); } catch { return; }
                if (msg.id !== id) return;
                settled = true;
                ws.removeEventListener('message', handler);
                if (msg.success) resolve(msg.result);
                else reject(new Error(msg.error || 'Save failed'));
            };
            ws.addEventListener('message', handler);
            ws.send(JSON.stringify({ command: 'save_file', id, params: { filePath, content } }));
            setTimeout(() => {
                if (!settled) {
                    ws.removeEventListener('message', handler);
                    reject(new Error('Save timeout'));
                }
            }, 3000);
        });
    };

    function connect() {
        try {
            ws = new WebSocket(WS_URL);
        } catch {
            scheduleReconnect();
            return;
        }

        ws.onopen = () => {
            console.log('[MCP-WS] Connected to MCP server');
        };

        ws.onmessage = (event) => {
            let msg;
            try { msg = JSON.parse(event.data); } catch { return; }

            if (!msg.id || !msg.command) return;

            const handler = COMMANDS[msg.command];
            if (!handler) {
                ws.send(JSON.stringify({ id: msg.id, success: false, error: `Unknown command: ${msg.command}` }));
                return;
            }
            try {
                const result = handler(msg.params || {});
                ws.send(JSON.stringify({ id: msg.id, success: true, result }));
            } catch (err) {
                ws.send(JSON.stringify({ id: msg.id, success: false, error: err.message }));
            }
        };

        ws.onclose = () => {
            console.log('[MCP-WS] Disconnected. Reconnecting in 3s...');
            scheduleReconnect();
        };

        ws.onerror = () => {
            ws.close();
        };
    }

    function scheduleReconnect() {
        setTimeout(() => {
            if (window.topologyAPI) connect();
        }, 3000);
    }

    // Wait for topologyAPI to be available, then connect
    function waitForAPI() {
        if (window.topologyAPI) {
            api = window.topologyAPI;
            connect();
        } else {
            setTimeout(waitForAPI, 100);
        }
    }

    waitForAPI();
})();
