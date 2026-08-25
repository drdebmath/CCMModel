let wasm;

const cachedTextDecoder = (typeof TextDecoder !== 'undefined' ? new TextDecoder('utf-8', { ignoreBOM: true, fatal: true }) : { decode: () => { throw Error('TextDecoder not available') } } );

if (typeof TextDecoder !== 'undefined') { cachedTextDecoder.decode(); };

let cachedUint8ArrayMemory0 = null;

function getUint8ArrayMemory0() {
    if (cachedUint8ArrayMemory0 === null || cachedUint8ArrayMemory0.byteLength === 0) {
        cachedUint8ArrayMemory0 = new Uint8Array(wasm.memory.buffer);
    }
    return cachedUint8ArrayMemory0;
}

function getStringFromWasm0(ptr, len) {
    ptr = ptr >>> 0;
    return cachedTextDecoder.decode(getUint8ArrayMemory0().subarray(ptr, ptr + len));
}

let cachedUint32ArrayMemory0 = null;

function getUint32ArrayMemory0() {
    if (cachedUint32ArrayMemory0 === null || cachedUint32ArrayMemory0.byteLength === 0) {
        cachedUint32ArrayMemory0 = new Uint32Array(wasm.memory.buffer);
    }
    return cachedUint32ArrayMemory0;
}

let WASM_VECTOR_LEN = 0;

function passArray32ToWasm0(arg, malloc) {
    const ptr = malloc(arg.length * 4, 4) >>> 0;
    getUint32ArrayMemory0().set(arg, ptr / 4);
    WASM_VECTOR_LEN = arg.length;
    return ptr;
}

let cachedUint16ArrayMemory0 = null;

function getUint16ArrayMemory0() {
    if (cachedUint16ArrayMemory0 === null || cachedUint16ArrayMemory0.byteLength === 0) {
        cachedUint16ArrayMemory0 = new Uint16Array(wasm.memory.buffer);
    }
    return cachedUint16ArrayMemory0;
}

function passArray16ToWasm0(arg, malloc) {
    const ptr = malloc(arg.length * 2, 2) >>> 0;
    getUint16ArrayMemory0().set(arg, ptr / 2);
    WASM_VECTOR_LEN = arg.length;
    return ptr;
}

function takeFromExternrefTable0(idx) {
    const value = wasm.__wbindgen_export_0.get(idx);
    wasm.__externref_table_dealloc(idx);
    return value;
}
/**
 * Algorithm selector shared by native and browser callers.
 * @enum {0 | 1 | 2}
 */
export const AlgorithmSelector = Object.freeze({
    HelpByScouts: 0, "0": "HelpByScouts",
    DropAndFreeze: 1, "1": "DropAndFreeze",
    P1Tree: 2, "2": "P1Tree",
});
/**
 * Trace policy. `Bounded` uses the supplied record and sampling limits.
 * @enum {0 | 1 | 2}
 */
export const TraceMode = Object.freeze({
    Off: 0, "0": "Off",
    Full: 1, "1": "Full",
    Bounded: 2, "2": "Bounded",
});

const SimulationOutputFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_simulationoutput_free(ptr >>> 0, 1));
/**
 * Final typed state and compact render stream returned by one run.
 */
export class SimulationOutput {

    static __wrap(ptr) {
        ptr = ptr >>> 0;
        const obj = Object.create(SimulationOutput.prototype);
        obj.__wbg_ptr = ptr;
        SimulationOutputFinalization.register(obj, obj.__wbg_ptr, obj);
        return obj;
    }

    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        SimulationOutputFinalization.unregister(this);
        return ptr;
    }

    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_simulationoutput_free(ptr, 0);
    }
    /**
     * Compact binary render records; this avoids one JS message per agent.
     * @returns {Uint8Array}
     */
    render_bytes() {
        const ret = wasm.simulationoutput_render_bytes(this.__wbg_ptr);
        return ret;
    }
    /**
     * @returns {number}
     */
    render_byte_len() {
        const ret = wasm.simulationoutput_render_byte_len(this.__wbg_ptr);
        return ret >>> 0;
    }
    /**
     * @returns {boolean}
     */
    render_truncated() {
        const ret = wasm.simulationoutput_render_truncated(this.__wbg_ptr);
        return ret !== 0;
    }
    /**
     * @returns {number}
     */
    termination_code() {
        const ret = wasm.simulationoutput_termination_code(this.__wbg_ptr);
        return ret;
    }
    /**
     * @returns {number}
     */
    render_record_count() {
        const ret = wasm.simulationoutput_render_record_count(this.__wbg_ptr);
        return ret >>> 0;
    }
    /**
     * Home node IDs, or -1 when an agent has no home.
     * @returns {Int32Array}
     */
    homes() {
        const ret = wasm.simulationoutput_homes(this.__wbg_ptr);
        return ret;
    }
    /**
     * @returns {bigint}
     */
    moves() {
        const ret = wasm.simulationoutput_moves(this.__wbg_ptr);
        return BigInt.asUintN(64, ret);
    }
    /**
     * @returns {bigint}
     */
    probes() {
        const ret = wasm.simulationoutput_probes(this.__wbg_ptr);
        return BigInt.asUintN(64, ret);
    }
    /**
     * @returns {bigint}
     */
    rounds() {
        const ret = wasm.simulationoutput_rounds(this.__wbg_ptr);
        return BigInt.asUintN(64, ret);
    }
    /**
     * Numeric status codes in dense agent order. Algorithms retain their
     * native status conventions: settled=0, unsettled=1, waiting/scout=2.
     * @returns {Uint8Array}
     */
    statuses() {
        const ret = wasm.simulationoutput_statuses(this.__wbg_ptr);
        return ret;
    }
    /**
     * @returns {AlgorithmSelector}
     */
    algorithm() {
        const ret = wasm.simulationoutput_algorithm(this.__wbg_ptr);
        return ret;
    }
    /**
     * Dense agent-node positions as a JS Uint32Array.
     * @returns {Uint32Array}
     */
    positions() {
        const ret = wasm.simulationoutput_positions(this.__wbg_ptr);
        return ret;
    }
}

const WasmSimulationFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_wasmsimulation_free(ptr >>> 0, 1));
/**
 * An opaque simulation instance.  The graph and initial placement are
 * immutable after construction; cancellation and trace policy are explicit.
 */
export class WasmSimulation {

    static __wrap(ptr) {
        ptr = ptr >>> 0;
        const obj = Object.create(WasmSimulation.prototype);
        obj.__wbg_ptr = ptr;
        WasmSimulationFinalization.register(obj, obj.__wbg_ptr, obj);
        return obj;
    }

    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        WasmSimulationFinalization.unregister(this);
        return ptr;
    }

    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_wasmsimulation_free(ptr, 0);
    }
    /**
     * @returns {number}
     */
    node_count() {
        const ret = wasm.wasmsimulation_node_count(this.__wbg_ptr);
        return ret >>> 0;
    }
    /**
     * @returns {TraceMode}
     */
    trace_mode() {
        const ret = wasm.wasmsimulation_trace_mode(this.__wbg_ptr);
        return ret;
    }
    /**
     * @returns {number}
     */
    agent_count() {
        const ret = wasm.wasmsimulation_agent_count(this.__wbg_ptr);
        return ret >>> 0;
    }
    /**
     * @returns {boolean}
     */
    is_cancelled() {
        const ret = wasm.wasmsimulation_is_cancelled(this.__wbg_ptr);
        return ret !== 0;
    }
    clear_cancelled() {
        wasm.wasmsimulation_clear_cancelled(this.__wbg_ptr);
    }
    /**
     * Creates a graph from CSR-like local port tables. `offsets` has
     * `node_count + 1` entries; neighbors and reciprocal ports have one entry
     * per local port. This preserves independently assigned local labels.
     * @param {number} node_count
     * @param {Uint32Array} offsets
     * @param {Uint32Array} neighbors
     * @param {Uint16Array} remote_ports
     * @param {Uint32Array} starts
     * @param {AlgorithmSelector} algorithm
     * @param {TraceMode} trace_mode
     * @param {bigint} round_limit
     * @param {number} max_trace_records
     * @param {number} sample_every
     * @returns {WasmSimulation}
     */
    static from_port_tables(node_count, offsets, neighbors, remote_ports, starts, algorithm, trace_mode, round_limit, max_trace_records, sample_every) {
        const ptr0 = passArray32ToWasm0(offsets, wasm.__wbindgen_malloc);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passArray32ToWasm0(neighbors, wasm.__wbindgen_malloc);
        const len1 = WASM_VECTOR_LEN;
        const ptr2 = passArray16ToWasm0(remote_ports, wasm.__wbindgen_malloc);
        const len2 = WASM_VECTOR_LEN;
        const ptr3 = passArray32ToWasm0(starts, wasm.__wbindgen_malloc);
        const len3 = WASM_VECTOR_LEN;
        const ret = wasm.wasmsimulation_from_port_tables(node_count, ptr0, len0, ptr1, len1, ptr2, len2, ptr3, len3, algorithm, trace_mode, round_limit, max_trace_records, sample_every);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return WasmSimulation.__wrap(ret[0]);
    }
    /**
     * Creates a canonical-port graph from `[source, destination, ...]` edge
     * pairs and dense start-node IDs.
     * @param {number} node_count
     * @param {Uint32Array} edges
     * @param {Uint32Array} starts
     * @param {AlgorithmSelector} algorithm
     * @param {TraceMode} trace_mode
     * @param {bigint} round_limit
     * @param {number} max_trace_records
     * @param {number} sample_every
     */
    constructor(node_count, edges, starts, algorithm, trace_mode, round_limit, max_trace_records, sample_every) {
        const ptr0 = passArray32ToWasm0(edges, wasm.__wbindgen_malloc);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passArray32ToWasm0(starts, wasm.__wbindgen_malloc);
        const len1 = WASM_VECTOR_LEN;
        const ret = wasm.wasmsimulation_new(node_count, ptr0, len0, ptr1, len1, algorithm, trace_mode, round_limit, max_trace_records, sample_every);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        this.__wbg_ptr = ret[0] >>> 0;
        WasmSimulationFinalization.register(this, this.__wbg_ptr, this);
        return this;
    }
    /**
     * Runs once and returns compact typed final state plus binary render data.
     * @returns {SimulationOutput}
     */
    run() {
        const ret = wasm.wasmsimulation_run(this.__wbg_ptr);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return SimulationOutput.__wrap(ret[0]);
    }
    /**
     * Requests cancellation before the next run. The current algorithm APIs
     * are synchronous, so an in-flight call cannot be interrupted safely;
     * callers should run in a Web Worker and cancel between calls.
     */
    cancel() {
        wasm.wasmsimulation_cancel(this.__wbg_ptr);
    }
    /**
     * @returns {AlgorithmSelector}
     */
    algorithm() {
        const ret = wasm.wasmsimulation_algorithm(this.__wbg_ptr);
        return ret;
    }
}

async function __wbg_load(module, imports) {
    if (typeof Response === 'function' && module instanceof Response) {
        if (typeof WebAssembly.instantiateStreaming === 'function') {
            try {
                return await WebAssembly.instantiateStreaming(module, imports);

            } catch (e) {
                if (module.headers.get('Content-Type') != 'application/wasm') {
                    console.warn("`WebAssembly.instantiateStreaming` failed because your server does not serve Wasm with `application/wasm` MIME type. Falling back to `WebAssembly.instantiate` which is slower. Original error:\n", e);

                } else {
                    throw e;
                }
            }
        }

        const bytes = await module.arrayBuffer();
        return await WebAssembly.instantiate(bytes, imports);

    } else {
        const instance = await WebAssembly.instantiate(module, imports);

        if (instance instanceof WebAssembly.Instance) {
            return { instance, module };

        } else {
            return instance;
        }
    }
}

function __wbg_get_imports() {
    const imports = {};
    imports.wbg = {};
    imports.wbg.__wbg_buffer_609cc3eee51ed158 = function(arg0) {
        const ret = arg0.buffer;
        return ret;
    };
    imports.wbg.__wbg_new_a12002a7f91c75be = function(arg0) {
        const ret = new Uint8Array(arg0);
        return ret;
    };
    imports.wbg.__wbg_new_e3b321dcfef89fc7 = function(arg0) {
        const ret = new Uint32Array(arg0);
        return ret;
    };
    imports.wbg.__wbg_new_e9a4a67dbababe57 = function(arg0) {
        const ret = new Int32Array(arg0);
        return ret;
    };
    imports.wbg.__wbg_newwithbyteoffsetandlength_999332a180064b59 = function(arg0, arg1, arg2) {
        const ret = new Int32Array(arg0, arg1 >>> 0, arg2 >>> 0);
        return ret;
    };
    imports.wbg.__wbg_newwithbyteoffsetandlength_d97e637ebe145a9a = function(arg0, arg1, arg2) {
        const ret = new Uint8Array(arg0, arg1 >>> 0, arg2 >>> 0);
        return ret;
    };
    imports.wbg.__wbg_newwithbyteoffsetandlength_f1dead44d1fc7212 = function(arg0, arg1, arg2) {
        const ret = new Uint32Array(arg0, arg1 >>> 0, arg2 >>> 0);
        return ret;
    };
    imports.wbg.__wbindgen_init_externref_table = function() {
        const table = wasm.__wbindgen_export_0;
        const offset = table.grow(4);
        table.set(0, undefined);
        table.set(offset + 0, undefined);
        table.set(offset + 1, null);
        table.set(offset + 2, true);
        table.set(offset + 3, false);
        ;
    };
    imports.wbg.__wbindgen_memory = function() {
        const ret = wasm.memory;
        return ret;
    };
    imports.wbg.__wbindgen_string_new = function(arg0, arg1) {
        const ret = getStringFromWasm0(arg0, arg1);
        return ret;
    };
    imports.wbg.__wbindgen_throw = function(arg0, arg1) {
        throw new Error(getStringFromWasm0(arg0, arg1));
    };

    return imports;
}

function __wbg_init_memory(imports, memory) {

}

function __wbg_finalize_init(instance, module) {
    wasm = instance.exports;
    __wbg_init.__wbindgen_wasm_module = module;
    cachedUint16ArrayMemory0 = null;
    cachedUint32ArrayMemory0 = null;
    cachedUint8ArrayMemory0 = null;


    wasm.__wbindgen_start();
    return wasm;
}

function initSync(module) {
    if (wasm !== undefined) return wasm;


    if (typeof module !== 'undefined') {
        if (Object.getPrototypeOf(module) === Object.prototype) {
            ({module} = module)
        } else {
            console.warn('using deprecated parameters for `initSync()`; pass a single object instead')
        }
    }

    const imports = __wbg_get_imports();

    __wbg_init_memory(imports);

    if (!(module instanceof WebAssembly.Module)) {
        module = new WebAssembly.Module(module);
    }

    const instance = new WebAssembly.Instance(module, imports);

    return __wbg_finalize_init(instance, module);
}

async function __wbg_init(module_or_path) {
    if (wasm !== undefined) return wasm;


    if (typeof module_or_path !== 'undefined') {
        if (Object.getPrototypeOf(module_or_path) === Object.prototype) {
            ({module_or_path} = module_or_path)
        } else {
            console.warn('using deprecated parameters for the initialization function; pass a single object instead')
        }
    }

    if (typeof module_or_path === 'undefined') {
        module_or_path = new URL('ccm_wasm_bg.wasm', import.meta.url);
    }
    const imports = __wbg_get_imports();

    if (typeof module_or_path === 'string' || (typeof Request === 'function' && module_or_path instanceof Request) || (typeof URL === 'function' && module_or_path instanceof URL)) {
        module_or_path = fetch(module_or_path);
    }

    __wbg_init_memory(imports);

    const { instance, module } = await __wbg_load(await module_or_path, imports);

    return __wbg_finalize_init(instance, module);
}

export { initSync };
export default __wbg_init;
