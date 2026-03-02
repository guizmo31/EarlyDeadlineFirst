// ============================================================
// TopologyState — Pure-logic state engine (no DOM, no WebSocket)
// Manages modules, instances, connections, and use cases in memory.
// Used by the MCP server for headless operation.
// ============================================================

import { randomUUID } from 'crypto';

export class TopologyState {
    constructor() {
        this.modules = [];
        this.instances = [];
        this.connections = [];
        this.useCases = [];
        this.nextInstanceCounters = {};
        this.topologyName = '';
        this._idCounter = 1;
    }

    genId() { return `id-${this._idCounter++}`; }
    genUUID() { return randomUUID(); }

    // Auto-position for headless mode (grid layout)
    _autoPosition() {
        const col = this.instances.length % 4;
        const row = Math.floor(this.instances.length / 4);
        return { x: 100 + col * 250, y: 100 + row * 200 };
    }

    // ---- Module Management ----

    addModule(params) {
        const name = (typeof params === 'string') ? params : params.name;
        if (!name || !name.trim()) throw new Error('Module name is required');

        const mod = {
            id: this.genId(),
            uuid: this.genUUID(),
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
        this.modules.push(mod);
        this.nextInstanceCounters[mod.id] = 0;
        return mod;
    }

    removeModule(params) {
        const modId = typeof params === 'string' ? params : params.moduleId;
        this.modules = this.modules.filter(m => m.id !== modId);
        delete this.nextInstanceCounters[modId];
        return { removed: modId };
    }

    listModules() {
        return this.modules;
    }

    // ---- Instance Operations ----

    createInstance(params) {
        const moduleId = params.moduleId || params;
        const mod = this.modules.find(m => m.id === moduleId);
        if (!mod) throw new Error(`Module not found: ${moduleId}`);

        const pos = (params.x != null && params.y != null)
            ? { x: params.x, y: params.y }
            : this._autoPosition();

        this.nextInstanceCounters[mod.id] = (this.nextInstanceCounters[mod.id] || 0) + 1;
        const counter = this.nextInstanceCounters[mod.id];

        const inst = {
            id: this.genId(),
            moduleId: mod.id,
            moduleName: mod.name,
            instance_id: counter,
            name: `${mod.name}/${counter}`,
            x: pos.x,
            y: pos.y,
            activation: 'PERIODIC',
            period_us: 10000,
            min_interarrival_us: 0,
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
            allowed_cores_mask: 0xFFFF,
            preferred_core: 0,
            affinity_group: '',
            state: 'INACTIVE',
        };
        this.instances.push(inst);
        return this._instanceView(inst);
    }

    deleteInstance(params) {
        const instId = typeof params === 'string' ? params : params.instanceId;
        // Remove connections involving this instance
        const toRemove = this.connections.filter(c => c.fromInstanceId === instId || c.toInstanceId === instId);
        toRemove.forEach(c => this.removeConnection({ connectionId: c.id }));
        // Remove from use cases
        this.useCases.forEach(u => {
            u.instanceIds = u.instanceIds.filter(id => id !== instId);
        });
        // Remove empty use cases
        this.useCases = this.useCases.filter(u => u.instanceIds.length > 0);
        // Remove instance
        this.instances = this.instances.filter(i => i.id !== instId);
        return { deleted: instId };
    }

    listInstances() {
        return this.instances.map(i => this._instanceView(i));
    }

    setInstanceProperties(params) {
        const inst = this.instances.find(i => i.id === params.instanceId);
        if (!inst) throw new Error(`Instance not found: ${params.instanceId}`);

        if (params.activation !== undefined) inst.activation = params.activation;
        if (params.period_us !== undefined) inst.period_us = params.period_us;
        if (params.min_interarrival_us !== undefined) inst.min_interarrival_us = params.min_interarrival_us;
        if (params.name !== undefined) inst.name = params.name;
        if (params.allowed_cores_mask !== undefined) inst.allowed_cores_mask = params.allowed_cores_mask;
        if (params.preferred_core !== undefined) inst.preferred_core = params.preferred_core;
        if (params.affinity_group !== undefined) inst.affinity_group = params.affinity_group;
        if (params.state !== undefined) inst.state = params.state;
        if (params.x !== undefined) inst.x = params.x;
        if (params.y !== undefined) inst.y = params.y;

        return this._instanceView(inst);
    }

    _instanceView(inst) {
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
    }

    // ---- Connection Operations ----

    addConnection(params) {
        const { fromInstanceId, fromPort, toInstanceId, toPort } = params;

        // Validation
        const fromInst = this.instances.find(i => i.id === fromInstanceId);
        const toInst = this.instances.find(i => i.id === toInstanceId);
        if (!fromInst) throw new Error(`Source instance not found: ${fromInstanceId}`);
        if (!toInst) throw new Error(`Destination instance not found: ${toInstanceId}`);
        if (fromInstanceId === toInstanceId) throw new Error('Cannot connect an instance to itself');

        const fromMod = this.modules.find(m => m.id === fromInst.moduleId);
        const toMod = this.modules.find(m => m.id === toInst.moduleId);
        if (fromMod && fromPort >= (fromMod.output_ports || []).length) {
            throw new Error(`Output port index ${fromPort} out of range (module ${fromMod.name} has ${(fromMod.output_ports || []).length} outputs)`);
        }
        if (toMod && toPort >= (toMod.input_ports || []).length) {
            throw new Error(`Input port index ${toPort} out of range (module ${toMod.name} has ${(toMod.input_ports || []).length} inputs)`);
        }

        // Dedup check
        const exists = this.connections.some(c =>
            c.fromInstanceId === fromInstanceId && c.fromPort === fromPort &&
            c.toInstanceId === toInstanceId && c.toPort === toPort
        );
        if (exists) throw new Error('Connection already exists');

        const conn = { id: this.genId(), fromInstanceId, fromPort, toInstanceId, toPort, doubleBuffer: false };
        this.connections.push(conn);
        return conn;
    }

    removeConnection(params) {
        const connId = typeof params === 'string' ? params : params.connectionId;
        this.connections = this.connections.filter(c => c.id !== connId);
        return { removed: connId };
    }

    listConnections() {
        return this.connections;
    }

    setConnectionProperties(params) {
        const conn = this.connections.find(c => c.id === params.connectionId);
        if (!conn) throw new Error(`Connection not found: ${params.connectionId}`);
        if (params.doubleBuffer !== undefined) conn.doubleBuffer = params.doubleBuffer;
        return conn;
    }

    // ---- Use Case Operations ----

    createUseCase(params) {
        const instanceIds = params.instanceIds || params;
        if (!instanceIds || instanceIds.length === 0) throw new Error('At least one instance ID is required');

        const name = `UseCase-${this.useCases.length + 1}`;
        const uc = { id: this.genId(), name, instanceIds: [...instanceIds], active: false };
        this.useCases.push(uc);
        return { id: uc.id, name: uc.name, instanceIds: uc.instanceIds, active: uc.active };
    }

    deleteUseCase(params) {
        const ucId = typeof params === 'string' ? params : params.useCaseId;
        this.useCases = this.useCases.filter(u => u.id !== ucId);
        return { deleted: ucId };
    }

    renameUseCase(params) {
        const uc = this.useCases.find(u => u.id === params.useCaseId);
        if (!uc) throw new Error(`Use Case not found: ${params.useCaseId}`);
        uc.name = params.newName.trim();
        return { id: uc.id, name: uc.name };
    }

    listUseCases() {
        return this.useCases.map(uc => ({
            id: uc.id, name: uc.name, instanceIds: uc.instanceIds, active: uc.active,
        }));
    }

    addInstanceToUseCase(params) {
        const uc = this.useCases.find(u => u.id === params.useCaseId);
        if (!uc) throw new Error(`Use Case not found: ${params.useCaseId}`);
        if (uc.instanceIds.includes(params.instanceId)) return { already: true };
        uc.instanceIds.push(params.instanceId);
        return { added: params.instanceId, to: params.useCaseId };
    }

    removeInstanceFromUseCase(params) {
        const uc = this.useCases.find(u => u.id === params.useCaseId);
        if (!uc) throw new Error(`Use Case not found: ${params.useCaseId}`);
        uc.instanceIds = uc.instanceIds.filter(id => id !== params.instanceId);
        if (uc.instanceIds.length === 0) {
            this.useCases = this.useCases.filter(u => u.id !== params.useCaseId);
        }
        return { removed: params.instanceId, from: params.useCaseId };
    }

    // ---- Topology State ----

    getTopology() {
        return {
            modules: this.modules,
            instances: this.instances.map(i => this._instanceView(i)),
            connections: this.connections,
            useCases: this.useCases.map(uc => ({
                id: uc.id, name: uc.name, instanceIds: uc.instanceIds, active: uc.active,
            })),
        };
    }

    loadTopology(params) {
        let data = params.data || params;
        // Auto-migrate old format
        if (data.version !== '2.0') data = this._migrateV1(data);

        this.topologyName = data.name || '';
        this.modules = data.modules || [];
        this.instances = data.instances || [];
        this.connections = data.connections || [];
        this.useCases = data.useCases || [];
        this.nextInstanceCounters = data.nextInstanceCounters || {};

        // Sync ID counter
        const allIds = [...this.modules, ...this.instances, ...this.connections, ...this.useCases]
            .map(o => o.id).filter(Boolean);
        allIds.forEach(id => {
            const num = parseInt(id.replace('id-', ''));
            if (!isNaN(num) && num >= this._idCounter) this._idCounter = num + 1;
        });

        return { loaded: true, modules: this.modules.length, instances: this.instances.length };
    }

    clearCanvas() {
        this.modules = [];
        this.instances = [];
        this.connections = [];
        this.useCases = [];
        this.nextInstanceCounters = {};
        this.topologyName = '';
        return { cleared: true };
    }

    exportTopology(name) {
        return {
            version: '2.0',
            name: name || this.topologyName || 'untitled',
            savedAt: new Date().toISOString(),
            modules: this.modules,
            instances: this.instances,
            connections: this.connections,
            useCases: this.useCases,
            nextInstanceCounters: this.nextInstanceCounters,
        };
    }

    // ---- EDF Config Generation ----

    generateEdfConfig(params = {}) {
        const tickPeriod = params.tickPeriod || 1;
        const simDuration = params.simDuration || 1000;
        const numCores = params.numCores || 1;
        const fixedPartitioning = params.fixedPartitioning || false;
        const chainConstraints = params.chainConstraints !== false; // default true for agent usage

        const processes = this.instances.map(inst => {
            const mod = this.modules.find(m => m.id === inst.moduleId);
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
                const incomingConns = this.connections.filter(c => c.toInstanceId === inst.id);
                const deps = incomingConns
                    .map(c => {
                        const fromInst = this.instances.find(i => i.id === c.fromInstanceId);
                        return fromInst ? fromInst.name : null;
                    })
                    .filter(Boolean);
                if (deps.length > 0) proc.dependencies = deps;

                // Double-buffered dependencies
                const dbDeps = incomingConns
                    .filter(c => c.doubleBuffer)
                    .map(c => {
                        const fromInst = this.instances.find(i => i.id === c.fromInstanceId);
                        return fromInst ? fromInst.name : null;
                    })
                    .filter(Boolean);
                if (dbDeps.length > 0) proc.double_buffer_deps = dbDeps;
            }

            return proc;
        });

        const use_cases = this.useCases.map(uc => ({
            name: uc.name,
            active: uc.active,
            process_names: uc.instanceIds
                .map(iid => this.instances.find(i => i.id === iid))
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
    }

    // ---- V1 Migration ----

    _migrateV1(data) {
        if (data.modules) {
            data.modules = data.modules.map(mod => {
                if (mod.input_ports) return mod;
                const input_ports = [];
                for (let i = 0; i < (mod.inputs || 0); i++) input_ports.push({ port_name: `in-${i}`, data_type: '', sample_size_bytes: 0 });
                const output_ports = [];
                for (let i = 0; i < (mod.outputs || 0); i++) output_ports.push({ port_name: `out-${i}`, data_type: '', sample_size_bytes: 0 });
                return {
                    ...mod,
                    uuid: mod.uuid || this.genUUID(),
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
        if (data.instances) {
            data.instances = data.instances.map(inst => {
                if (inst.activation) return inst;
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
        if (data.pipelines && !data.useCases) {
            data.useCases = data.pipelines.map(pl => ({
                id: pl.id, name: pl.name, instanceIds: pl.instanceIds, active: false,
            }));
        }
        return data;
    }
}
