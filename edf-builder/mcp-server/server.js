// ============================================================
// Safety Topology Builder — MCP Server v3.0
// Dual mode: headless (internal state) + live sync (browser)
// stdio transport (for Claude Code) + WebSocket (for browser)
// ============================================================

import { McpServer } from '@modelcontextprotocol/sdk/server/mcp.js';
import { StdioServerTransport } from '@modelcontextprotocol/sdk/server/stdio.js';
import { WebSocketServer } from 'ws';
import { z } from 'zod';
import { createServer } from 'http';
import { writeFile, readFile, mkdir, readdir, stat, unlink } from 'fs/promises';
import { dirname, resolve, isAbsolute, extname, normalize } from 'path';
import { fileURLToPath } from 'url';
import { TopologyState } from './topology-state.js';

const __dirname = dirname(fileURLToPath(import.meta.url));
const PROJECT_ROOT = resolve(__dirname, '../..');

const WS_PORT = 6589;
const HTTP_PORT = 6590;
const TIMEOUT_MS = 10000;

// Parse --data-dir CLI argument (default: PROJECT_ROOT/Topologies)
function parseDataDir() {
    const idx = process.argv.indexOf('--data-dir');
    if (idx !== -1 && process.argv[idx + 1]) {
        const p = process.argv[idx + 1];
        return isAbsolute(p) ? p : resolve(PROJECT_ROOT, p);
    }
    return resolve(PROJECT_ROOT, 'Topologies');
}
const DATA_DIR = parseDataDir();

// ============================================================
// Internal Topology State (headless engine)
// ============================================================
const topology = new TopologyState();

// ============================================================
// HTTP Server — shared file directory + EDF config endpoint
// ============================================================
let latestEdfConfig = null;

function safePath(filename) {
    const resolved = resolve(DATA_DIR, normalize(filename));
    if (!resolved.startsWith(DATA_DIR)) return null;
    return resolved;
}

function readBody(req) {
    return new Promise((resolve, reject) => {
        const chunks = [];
        req.on('data', (c) => chunks.push(c));
        req.on('end', () => resolve(Buffer.concat(chunks).toString('utf-8')));
        req.on('error', reject);
    });
}

const httpServer = createServer(async (req, res) => {
    res.setHeader('Access-Control-Allow-Origin', '*');
    res.setHeader('Access-Control-Allow-Methods', 'GET, POST, DELETE, OPTIONS');
    res.setHeader('Access-Control-Allow-Headers', 'Content-Type');

    if (req.method === 'OPTIONS') { res.writeHead(204); res.end(); return; }

    if (req.method === 'GET' && req.url === '/edf-config') {
        if (!latestEdfConfig) {
            res.writeHead(404, { 'Content-Type': 'application/json' });
            res.end(JSON.stringify({ error: 'No EDF config available' }));
            return;
        }
        res.writeHead(200, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify(latestEdfConfig));
        return;
    }

    if (req.method === 'GET' && req.url === '/files/') {
        try {
            await mkdir(DATA_DIR, { recursive: true });
            const entries = await readdir(DATA_DIR);
            const files = [];
            for (const name of entries) {
                if (extname(name).toLowerCase() !== '.json') continue;
                try {
                    const s = await stat(resolve(DATA_DIR, name));
                    files.push({ name, size: s.size, modified: s.mtime.toISOString() });
                } catch { /* skip */ }
            }
            files.sort((a, b) => b.modified.localeCompare(a.modified));
            res.writeHead(200, { 'Content-Type': 'application/json' });
            res.end(JSON.stringify({ dir: DATA_DIR, files }));
        } catch (err) {
            res.writeHead(500, { 'Content-Type': 'application/json' });
            res.end(JSON.stringify({ error: err.message }));
        }
        return;
    }

    if (req.method === 'GET' && req.url.startsWith('/files/') && req.url.length > 7) {
        const filename = decodeURIComponent(req.url.slice(7));
        const absPath = safePath(filename);
        if (!absPath) { res.writeHead(400); res.end(JSON.stringify({ error: 'Invalid path' })); return; }
        try {
            const content = await readFile(absPath, 'utf-8');
            res.writeHead(200, { 'Content-Type': 'application/json' });
            res.end(content);
        } catch {
            res.writeHead(404, { 'Content-Type': 'application/json' });
            res.end(JSON.stringify({ error: 'File not found' }));
        }
        return;
    }

    if (req.method === 'POST' && req.url.startsWith('/files/') && req.url.length > 7) {
        const filename = decodeURIComponent(req.url.slice(7));
        const absPath = safePath(filename);
        if (!absPath) { res.writeHead(400); res.end(JSON.stringify({ error: 'Invalid path' })); return; }
        try {
            const body = await readBody(req);
            await mkdir(DATA_DIR, { recursive: true });
            await writeFile(absPath, body, 'utf-8');
            console.error(`[MCP] File saved: ${absPath}`);
            res.writeHead(200, { 'Content-Type': 'application/json' });
            res.end(JSON.stringify({ saved: filename }));
        } catch (err) {
            res.writeHead(500, { 'Content-Type': 'application/json' });
            res.end(JSON.stringify({ error: err.message }));
        }
        return;
    }

    if (req.method === 'DELETE' && req.url.startsWith('/files/') && req.url.length > 7) {
        const filename = decodeURIComponent(req.url.slice(7));
        const absPath = safePath(filename);
        if (!absPath) { res.writeHead(400); res.end(JSON.stringify({ error: 'Invalid path' })); return; }
        try {
            await unlink(absPath);
            console.error(`[MCP] File deleted: ${absPath}`);
            res.writeHead(200, { 'Content-Type': 'application/json' });
            res.end(JSON.stringify({ deleted: filename }));
        } catch (err) {
            res.writeHead(err.code === 'ENOENT' ? 404 : 500, { 'Content-Type': 'application/json' });
            res.end(JSON.stringify({ error: err.message }));
        }
        return;
    }

    res.writeHead(404); res.end();
});

httpServer.listen(HTTP_PORT, () => {
    console.error(`[MCP] HTTP server listening on http://localhost:${HTTP_PORT}`);
    console.error(`[MCP] Shared data directory: ${DATA_DIR}`);
});

// ============================================================
// WebSocket Server — bridge to browser (live sync)
// ============================================================
let browserSocket = null;
const pendingRequests = new Map();
let requestIdCounter = 0;

const wss = new WebSocketServer({ port: WS_PORT });

wss.on('connection', (ws) => {
    browserSocket = ws;
    console.error('[MCP-WS] Browser connected');

    // Push current internal state to browser for rendering
    if (topology.modules.length > 0 || topology.instances.length > 0) {
        const data = topology.exportTopology(topology.topologyName || 'untitled');
        ws.send(JSON.stringify({
            id: `sync-${++requestIdCounter}`,
            command: 'load_topology',
            params: { data },
        }));
        console.error(`[MCP-WS] Pushed internal state to browser (${topology.instances.length} instances)`);
    }

    ws.on('message', (rawData) => {
        let msg;
        try { msg = JSON.parse(rawData.toString()); } catch { return; }

        // Handle store_edf_config from browser
        if (msg.command === 'store_edf_config' && msg.config) {
            latestEdfConfig = msg.config;
            const edfPath = resolve(DATA_DIR, 'edf-config.json');
            mkdir(DATA_DIR, { recursive: true })
                .then(() => writeFile(edfPath, JSON.stringify(msg.config, null, 2), 'utf-8'))
                .then(() => console.error(`[MCP] EDF config stored: ${edfPath}`))
                .catch(err => console.error(`[MCP] EDF config persist error: ${err.message}`));
            return;
        }

        // Handle sync_state from browser (browser pushes its state to server)
        if (msg.command === 'sync_state' && msg.state) {
            topology.loadTopology(msg.state);
            console.error('[MCP-WS] Browser state synced to server');
            return;
        }

        // Handle save_file from browser
        if (msg.command === 'save_file' && msg.id && msg.params) {
            const { filePath, content } = msg.params;
            const absPath = isAbsolute(filePath) ? filePath : resolve(PROJECT_ROOT, filePath);
            (async () => {
                try {
                    await mkdir(dirname(absPath), { recursive: true });
                    await writeFile(absPath, content, 'utf-8');
                    console.error(`[MCP] File saved: ${absPath}`);
                    ws.send(JSON.stringify({ id: msg.id, success: true, result: { saved: absPath } }));
                } catch (err) {
                    ws.send(JSON.stringify({ id: msg.id, success: false, error: err.message }));
                }
            })();
            return;
        }

        // Handle response to a pending request
        const pending = pendingRequests.get(msg.id);
        if (pending) {
            clearTimeout(pending.timer);
            pendingRequests.delete(msg.id);
            if (msg.success) pending.resolve(msg.result);
            else pending.reject(new Error(msg.error || 'Unknown error from browser'));
        }
    });

    ws.on('close', () => {
        if (browserSocket === ws) {
            browserSocket = null;
            console.error('[MCP-WS] Browser disconnected');
        }
    });

    ws.on('error', (err) => {
        console.error('[MCP-WS] WebSocket error:', err.message);
    });
});

// Send command to browser (for live sync, non-critical)
function sendCommand(command, params) {
    return new Promise((resolve, reject) => {
        if (!browserSocket || browserSocket.readyState !== 1) {
            reject(new Error('Browser not connected'));
            return;
        }
        const id = `req-${++requestIdCounter}`;
        const timer = setTimeout(() => {
            pendingRequests.delete(id);
            reject(new Error('Timeout: browser did not respond'));
        }, TIMEOUT_MS);
        pendingRequests.set(id, { resolve, reject, timer });
        browserSocket.send(JSON.stringify({ id, command, params }));
    });
}

// ============================================================
// Dual-mode execution: internal state + optional browser sync
// ============================================================

// Map TopologyState method names to WebSocket command names
const METHOD_TO_CMD = {
    addModule: 'add_module',
    removeModule: 'remove_module',
    listModules: 'list_modules',
    createInstance: 'create_instance',
    deleteInstance: 'delete_instance',
    listInstances: 'list_instances',
    setInstanceProperties: 'set_instance_properties',
    addConnection: 'add_connection',
    removeConnection: 'remove_connection',
    listConnections: 'list_connections',
    setConnectionProperties: 'set_connection_properties',
    createUseCase: 'create_usecase',
    deleteUseCase: 'delete_usecase',
    renameUseCase: 'rename_usecase',
    listUseCases: 'list_usecases',
    addInstanceToUseCase: 'add_instance_to_usecase',
    removeInstanceFromUseCase: 'remove_instance_from_usecase',
    getTopology: 'get_topology',
    loadTopology: 'load_topology',
    clearCanvas: 'clear_canvas',
    generateEdfConfig: 'generate_edf_config',
};

// Read-only methods (no need to sync to browser)
const READ_ONLY = new Set([
    'listModules', 'listInstances', 'listConnections', 'listUseCases', 'getTopology',
]);

async function mcpExec(method, params = {}) {
    try {
        // 1. Execute on internal state
        const result = topology[method](params);

        // 2. Sync to browser if connected (mutations only)
        if (!READ_ONLY.has(method) && browserSocket && browserSocket.readyState === 1) {
            const cmd = METHOD_TO_CMD[method];
            if (cmd) {
                try { await sendCommand(cmd, params); }
                catch (e) { console.error(`[MCP] Browser sync failed (${cmd}): ${e.message}`); }
            }
        }

        return { content: [{ type: 'text', text: JSON.stringify(result, null, 2) }] };
    } catch (err) {
        return { content: [{ type: 'text', text: `Error: ${err.message}` }], isError: true };
    }
}

// ============================================================
// MCP Server — tool registrations
// ============================================================
const server = new McpServer({
    name: 'safety-topology-builder',
    version: '3.0.0',
});

const portDefSchema = z.object({
    port_name: z.string().describe('Port name (e.g. "imu_in", "state_out")'),
    data_type: z.string().describe('Data type (e.g. "IMU_Sample_T", "State_T")'),
    sample_size_bytes: z.number().int().min(0).describe('Sample size in bytes'),
});

// ----- Module Management -----

server.tool(
    'add_module',
    'Create a new ModuleClass template. A module defines a reusable SW component with named/typed I/O ports, timing, resources, and ASIL level. You must create modules before instantiating them.',
    {
        name: z.string().describe('Module name (e.g. "KalmanFilter", "SensorFusion", "ActuatorDriver")'),
        version: z.number().int().min(1).optional().describe('Module version (default: 1)'),
        input_ports: z.array(portDefSchema).optional().describe('Input port definitions — define these for connections between instances'),
        output_ports: z.array(portDefSchema).optional().describe('Output port definitions — define these for connections between instances'),
        wcet_us: z.number().int().min(0).optional().describe('Worst Case Execution Time in µs (default: 500)'),
        bcet_us: z.number().int().min(0).optional().describe('Best Case Execution Time in µs (default: 100)'),
        typical_us: z.number().int().min(0).optional().describe('Typical Execution Time in µs (default: 300)'),
        stack_size_bytes: z.number().int().min(0).optional().describe('Stack size in bytes (default: 4096)'),
        static_mem_bytes: z.number().int().min(0).optional().describe('Static memory in bytes (default: 1024)'),
        requires_fpu: z.boolean().optional().describe('Whether module requires FPU (default: false)'),
        requires_gpu: z.boolean().optional().describe('Whether module requires GPU (default: false)'),
        asil_level: z.enum(['QM', 'A', 'B', 'C', 'D']).optional().describe('ASIL safety level (default: QM)'),
    },
    async (params) => mcpExec('addModule', params)
);

server.tool(
    'remove_module',
    'Remove a module template from the library',
    { moduleId: z.string().describe('ID of the module to remove') },
    async (params) => mcpExec('removeModule', params)
);

server.tool(
    'list_modules',
    'List all ModuleClass templates. Returns id, name, version, input_ports, output_ports, wcet_us, asil_level, etc.',
    {},
    async () => mcpExec('listModules')
);

// ----- Instance Operations -----

server.tool(
    'create_instance',
    'Instantiate a module on the canvas. Returns the instance with auto-generated name (ModuleName/N). Default activation is PERIODIC at 10ms. Use set_instance_properties to change period_us or activation type after creation.',
    {
        moduleId: z.string().describe('ID of the module to instantiate (from list_modules)'),
        x: z.number().optional().describe('X position on canvas (auto-positioned if omitted)'),
        y: z.number().optional().describe('Y position on canvas (auto-positioned if omitted)'),
    },
    async (params) => mcpExec('createInstance', params)
);

server.tool(
    'delete_instance',
    'Delete an instance and all its connections.',
    { instanceId: z.string().describe('ID of the instance to delete') },
    async (params) => mcpExec('deleteInstance', params)
);

server.tool(
    'list_instances',
    'List all instances with their properties: id, name, moduleId, activation, period_us, ports, placement.',
    {},
    async () => mcpExec('listInstances')
);

server.tool(
    'set_instance_properties',
    'Update properties of an existing instance: activation type (PERIODIC/DATA_DRIVEN/SPORADIC), period_us, name, core placement, etc.',
    {
        instanceId: z.string().describe('ID of the instance to update'),
        activation: z.enum(['PERIODIC', 'DATA_DRIVEN', 'SPORADIC']).optional().describe('Activation type'),
        period_us: z.number().int().min(0).optional().describe('Period in microseconds (for PERIODIC)'),
        min_interarrival_us: z.number().int().min(0).optional().describe('Min interarrival in µs (for SPORADIC)'),
        name: z.string().optional().describe('Display name'),
        allowed_cores_mask: z.number().int().optional().describe('Bitmask of allowed cores (0xFFFF = all)'),
        preferred_core: z.number().int().min(0).optional().describe('Preferred core index'),
        affinity_group: z.string().optional().describe('Affinity group name'),
        state: z.enum(['INACTIVE', 'ACTIVE']).optional().describe('Runtime state'),
        x: z.number().optional().describe('X position on canvas'),
        y: z.number().optional().describe('Y position on canvas'),
    },
    async (params) => mcpExec('setInstanceProperties', params)
);

// ----- Connection Operations -----

server.tool(
    'add_connection',
    'Connect an output port of one instance to an input port of another. Port indices are 0-based, matching the order in the module input_ports/output_ports arrays. Use list_instances to see available ports.',
    {
        fromInstanceId: z.string().describe('ID of the source instance'),
        fromPort: z.number().int().min(0).describe('Output port index (0-based)'),
        toInstanceId: z.string().describe('ID of the destination instance'),
        toPort: z.number().int().min(0).describe('Input port index (0-based)'),
    },
    async (params) => mcpExec('addConnection', params)
);

server.tool(
    'remove_connection',
    'Remove a connection between two instance ports',
    { connectionId: z.string().describe('ID of the connection to remove') },
    async (params) => mcpExec('removeConnection', params)
);

server.tool(
    'list_connections',
    'List all connections with id, fromInstanceId, fromPort, toInstanceId, toPort, doubleBuffer.',
    {},
    async () => mcpExec('listConnections')
);

server.tool(
    'set_connection_properties',
    'Update connection properties. Supports doubleBuffer (enables parallel execution with 1-period latency trade-off).',
    {
        connectionId: z.string().describe('ID of the connection to update'),
        doubleBuffer: z.boolean().optional().describe('Enable double buffering on this connection'),
    },
    async (params) => mcpExec('setConnectionProperties', params)
);

// ----- Use Case Operations -----

server.tool(
    'create_usecase',
    'Group instances into a Use Case — a functional grouping that can be started/stopped dynamically.',
    {
        instanceIds: z.array(z.string()).describe('Array of instance IDs to group'),
    },
    async (params) => mcpExec('createUseCase', params)
);

server.tool(
    'delete_usecase',
    'Remove a Use Case grouping (does NOT delete the instances inside)',
    { useCaseId: z.string().describe('ID of the Use Case to delete') },
    async (params) => mcpExec('deleteUseCase', params)
);

server.tool(
    'rename_usecase',
    'Rename a Use Case',
    {
        useCaseId: z.string().describe('ID of the Use Case to rename'),
        newName: z.string().describe('New name for the Use Case'),
    },
    async (params) => mcpExec('renameUseCase', params)
);

server.tool(
    'list_usecases',
    'List all Use Cases with id, name, instanceIds, active.',
    {},
    async () => mcpExec('listUseCases')
);

server.tool(
    'add_instance_to_usecase',
    'Add an instance to an existing Use Case.',
    {
        instanceId: z.string().describe('ID of the instance to add'),
        useCaseId: z.string().describe('ID of the Use Case'),
    },
    async (params) => mcpExec('addInstanceToUseCase', params)
);

server.tool(
    'remove_instance_from_usecase',
    'Remove an instance from a Use Case. If empty, the Use Case is deleted.',
    {
        instanceId: z.string().describe('ID of the instance to remove'),
        useCaseId: z.string().describe('ID of the Use Case'),
    },
    async (params) => mcpExec('removeInstanceFromUseCase', params)
);

// ----- Topology State -----

server.tool(
    'get_topology',
    'Get the complete topology state: all modules, instances, connections, and Use Cases. Use this to inspect the current state before making changes.',
    {},
    async () => mcpExec('getTopology')
);

server.tool(
    'load_topology',
    'Load/replace the entire topology from a JSON object. Supports v1 (legacy) and v2 format.',
    {
        data: z.object({
            modules: z.array(z.any()),
            instances: z.array(z.any()),
            connections: z.array(z.any()),
            useCases: z.array(z.any()).optional(),
            pipelines: z.array(z.any()).optional(),
            nextInstanceCounters: z.record(z.number()).optional(),
        }).describe('Complete topology data to load'),
    },
    async (params) => mcpExec('loadTopology', params)
);

server.tool(
    'clear_canvas',
    'Clear everything: remove all modules, instances, connections, and Use Cases.',
    {},
    async () => mcpExec('clearCanvas')
);

server.tool(
    'generate_edf_config',
    'Generate an EDF scheduling configuration from the current topology. Each instance becomes a process. Dependencies are derived from connections. Returns the config JSON.',
    {
        tickPeriod: z.number().int().min(1).optional().describe('Tick period in ms (default: 1)'),
        simDuration: z.number().int().min(1).optional().describe('Simulation duration in ms (default: 1000)'),
        numCores: z.number().int().min(1).optional().describe('Number of CPU cores (default: 1)'),
        fixedPartitioning: z.boolean().optional().describe('Fixed core partitioning (default: false)'),
        chainConstraints: z.boolean().optional().describe('Add dependency constraints from connections (default: true)'),
    },
    async (params) => mcpExec('generateEdfConfig', params)
);

// ----- File Operations -----

server.tool(
    'save_topology',
    'Save the current topology to a JSON file in the shared data directory. The file can be loaded later in the browser or via load_topology_file.',
    {
        name: z.string().describe('Topology name (saved as <name>.json in the data directory)'),
    },
    async (params) => {
        try {
            const data = topology.exportTopology(params.name);
            const filename = params.name.endsWith('.json') ? params.name : params.name + '.json';
            const absPath = safePath(filename);
            if (!absPath) return { content: [{ type: 'text', text: 'Error: Invalid filename' }], isError: true };
            await mkdir(DATA_DIR, { recursive: true });
            await writeFile(absPath, JSON.stringify(data, null, 2), 'utf-8');
            console.error(`[MCP] Topology saved: ${absPath}`);
            return { content: [{ type: 'text', text: JSON.stringify({ saved: absPath, modules: data.modules.length, instances: data.instances.length, connections: data.connections.length }) }] };
        } catch (err) {
            return { content: [{ type: 'text', text: `Error: ${err.message}` }], isError: true };
        }
    }
);

server.tool(
    'load_topology_file',
    'Load a topology from a JSON file on disk. Accepts a filename in the data directory (e.g. "my-topology.json") or an absolute path.',
    {
        filename: z.string().describe('Filename or absolute path to the topology JSON file'),
    },
    async (params) => {
        try {
            const absPath = isAbsolute(params.filename) ? params.filename : safePath(params.filename);
            if (!absPath) return { content: [{ type: 'text', text: 'Error: Invalid path' }], isError: true };
            const content = await readFile(absPath, 'utf-8');
            const data = JSON.parse(content);
            const result = topology.loadTopology(data);

            // Sync to browser if connected
            if (browserSocket && browserSocket.readyState === 1) {
                try {
                    const fullData = topology.exportTopology(topology.topologyName);
                    await sendCommand('load_topology', { data: fullData });
                } catch (e) {
                    console.error(`[MCP] Browser sync failed: ${e.message}`);
                }
            }

            return { content: [{ type: 'text', text: JSON.stringify({ loaded: absPath, ...result }) }] };
        } catch (err) {
            return { content: [{ type: 'text', text: `Error: ${err.message}` }], isError: true };
        }
    }
);

// ============================================================
// Start
// ============================================================
async function main() {
    const transport = new StdioServerTransport();
    await server.connect(transport);
    console.error(`[MCP] Safety Topology Builder MCP server started (v3.0 — headless + live sync)`);
    console.error(`[MCP] WebSocket server listening on ws://localhost:${WS_PORT}`);
}

main().catch((err) => {
    console.error('[MCP] Fatal error:', err);
    process.exit(1);
});
