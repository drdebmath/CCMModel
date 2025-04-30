// main.js

import { pyodideReady } from './pyodide-setup.js';
import { runSimulation } from './simulation-runner.js';
import { drawCytoscape } from './cytoscape-visualizer.js';

console.log("main.js: Script start."); // Log script execution start

// Simplified input validation
function getVal(input, def, min) {
    let v = parseInt(input.value, 10);
    if (isNaN(v) || v < min) {
        console.warn(`main.js: Invalid input value in ${input.id}. Resetting to default ${def}.`);
        input.value = def;
        return def;
    }
    return v;
}

// --- Wait for the DOM to be fully loaded ---
// Although type="module" and script at end often suffice, let's be explicit
document.addEventListener('DOMContentLoaded', () => {
    console.log("main.js: DOM fully loaded and parsed.");

    const btn = document.getElementById('runBtn');
    const out = document.getElementById('output');
    const cyId = 'cy';

    if (!btn) {
        console.error("main.js: CRITICAL - Could not find button #runBtn after DOMContentLoaded.");
        out.textContent = "Error: Button element not found.";
        return; // Stop if button isn't found
    } else {
        console.log("main.js: Found button #runBtn:", btn);
    }

    if (!out) {
        console.warn("main.js: Could not find output element #output.");
    }

    // --- Define the click handler function ---
    const handleRunClick = async () => {
        console.log("main.js: handleRunClick invoked!"); // <-- *** THIS IS THE KEY LOG TO LOOK FOR ON CLICK ***
        if (btn.disabled) {
            console.log("main.js: Click ignored, button is already disabled.");
            return; // Prevent re-entry if already running
        }

        btn.disabled = true;
        if (out) out.textContent = 'Loading…';
        console.log("main.js: Button disabled, text set to Loading...");

        try {
            console.log("main.js: Waiting for pyodideReady...");
            const py = await pyodideReady;
            console.log("main.js: Pyodide ready.");
            if (out) out.textContent = 'Running…';

            const n = getVal(document.getElementById('nodeCountInput'), 10, 1);
            const d = Math.min(getVal(document.getElementById('maxDegreeInput'), 4, 1), n - 1);
            const a = getVal(document.getElementById('agentCountInput'), 3, 1);
            const r = n*d;
            const seed = parseInt(document.getElementById('seedInput').value, 10) || 42;
            console.log(`main.js: Parameters - n=${n}, d=${d}, a=${a}, r=${r}, seed=${seed}`);
            if (a > n) {
                const msg = `Number of agents (${a}) must be smaller than number of nodes (${n}).`;
                console.error("main.js:", msg);
                if (out) out.textContent = `Error: ${msg}`;
                btn.disabled = false;
                return;
            }
            
            if (out) out.textContent = `Gen graph: ${n} nodes, maxDeg ${d}, agents ${a}, rounds ${r}, seed ${seed}`;

            console.log("main.js: Calling runSimulation...");
            const data = await runSimulation(py, n, d, a, r, seed);
            console.log("main.js: runSimulation returned.");

            if (!data || !data.positions || !data.statuses) {
                 throw new Error('Bad data received from runSimulation');
            }
            console.log("main.js: Simulation data looks okay. Calling drawCytoscape...");
            drawCytoscape(cyId, data);
            console.log("main.js: drawCytoscape finished.");
            if (out) out.textContent = `Displayed ${data.positions.length - 1} rounds. Green = SETTLED.`;

        } catch (err) {
            console.error("main.js: Error during handleRunClick execution:", err);
            if (out) out.textContent = `Error: ${err.message}`;
        } finally {
            console.log("main.js: Re-enabling button.");
            btn.disabled = false;
        }
    };

    // --- Attach the event listener ---
    // Remove potentially existing listener first (safer if script re-runs)
    btn.removeEventListener('click', handleRunClick);
    // Add the listener
    btn.addEventListener('click', handleRunClick);
    console.log("main.js: Attached click listener to #runBtn.");

    // Initial message
    if (out) out.textContent = 'Click “Run” to start.';

}); // End of DOMContentLoaded listener

console.log("main.js: Script end."); // Log script execution end