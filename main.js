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

// Variable to store the last loaded/generated simulation data
let lastSimulationData = null;

// Helper to disable/enable relevant buttons
function setRunningState(isRunning) {
    const runBtn = document.getElementById('runBtn');
    const loadJsonBtn = document.getElementById('loadJsonBtn');
    const jsonFileInput = document.getElementById('jsonFileInput');
    const saveDataBtn = document.getElementById('saveDataBtn');

    if (runBtn) runBtn.disabled = isRunning;
    if (loadJsonBtn) loadJsonBtn.disabled = isRunning;
    if (jsonFileInput) jsonFileInput.disabled = isRunning;

    // Save button is enabled only when not running AND data is available
    if (saveDataBtn) saveDataBtn.disabled = isRunning || lastSimulationData === null;

    // Also disable animation controls while busy
    const playPauseBtn = document.getElementById("playPauseBtn");
    const nextStepBtn = document.getElementById("nextStepBtn");
    const prevStepBtn = document.getElementById("prevStepBtn");
    // Animation controls are managed by cytoscape-visualizer,
    // but we can disable them here *while the sim/load is running*.
    // They will be re-enabled by cytoscape-visualizer's draw function or updateControlStates.
    if (playPauseBtn) playPauseBtn.disabled = isRunning;
    if (nextStepBtn) nextStepBtn.disabled = isRunning;
    if (prevStepBtn) prevStepBtn.disabled = isRunning;
}

// Helper to trigger a file download
function triggerDownload(data, filename) {
    if (!data) {
        console.error("No data available to save.");
        return;
    }

    try {
        // Convert data object back to JSON string
        const jsonString = JSON.stringify(data, null, 2); // Use 2 spaces for pretty printing

        // Create a Blob
        const blob = new Blob([jsonString], { type: 'application/json' });

        // Create a link element
        const a = document.createElement('a');
        a.href = URL.createObjectURL(blob);
        a.download = filename;

        // Append link to body, click it, and remove
        document.body.appendChild(a);
        a.click();
        document.body.removeChild(a);

        // Revoke the object URL after a short delay to free up resources
        setTimeout(() => {
            URL.revokeObjectURL(a.href);
        }, 100);

        console.log(`File "${filename}" download triggered successfully.`);

    } catch (error) {
        console.error("Error triggering file download:", error);
        const out = document.getElementById('output');
        if (out) out.textContent = `Error saving file: ${error.message}`;
    }
}

// Helper to generate a timestamped filename
function generateTimestampFilename() {
    const now = new Date();
    const year = now.getFullYear();
    const month = (now.getMonth() + 1).toString().padStart(2, '0');
    const day = now.getDate().toString().padStart(2, '0');
    const hours = now.getHours().toString().padStart(2, '0');
    const minutes = now.getMinutes().toString().padStart(2, '0');
    const seconds = now.getSeconds().toString().padStart(2, '0');
    return `simulation_data_${year}${month}${day}_${hours}${minutes}${seconds}.json`;
}


// --- Wait for the DOM to be fully loaded ---
document.addEventListener('DOMContentLoaded', () => {
    console.log("main.js: DOM fully loaded and parsed.");

    const runBtn = document.getElementById('runBtn');
    const loadJsonBtn = document.getElementById('loadJsonBtn');
    const jsonFileInput = document.getElementById('jsonFileInput');
    const saveDataBtn = document.getElementById('saveDataBtn'); // Get the new button
    const out = document.getElementById('output');
    const cyId = 'cy';

    // Check for critical elements
    if (!runBtn || !loadJsonBtn || !jsonFileInput || !saveDataBtn || !out || !document.getElementById(cyId)) {
        console.error("main.js: CRITICAL - Missing one or more required HTML elements.");
        if (out) out.textContent = "Error: Missing UI elements.";
        return;
    } else {
        console.log("main.js: Found all required UI elements.");
    }


    // --- Handle Simulation Run ---
    const handleRunSimulationClick = async () => {
        console.log("main.js: handleRunSimulationClick invoked!");
        lastSimulationData = null; // Clear previous data
        setRunningState(true);
        if (out) out.textContent = 'Loading Pyodide...';

        try {
            console.log("main.js: Waiting for pyodideReady...");
            const py = await pyodideReady;
            console.log("main.js: Pyodide ready.");
            if (out) out.textContent = 'Running simulation...';

            const n = getVal(document.getElementById('nodeCountInput'), 10, 1);
            const d = Math.min(getVal(document.getElementById('maxDegreeInput'), 4, 1), n - 1);
            const a = getVal(document.getElementById('agentCountInput'), 3, 1);
            const sp = getVal(document.getElementById('startingPositionsInput'), 5, 1);
            const r = n*d; // Default rounds
            const seed = parseInt(document.getElementById('seedInput').value, 10) || 42;
            console.log(`main.js: Simulation Parameters - n=${n}, d=${d}, a=${a}, r=${r}, seed=${seed}, sp=${sp}`);

            if (a > n) {
                const msg = `Number of agents (${a}) must be smaller than or equal to number of nodes (${n}).`;
                console.error("main.js:", msg);
                if (out) out.textContent = `Error: ${msg}`;
                return; // Stop execution
            }

            if (out) out.textContent = `Gen graph: ${n} nodes, maxDeg ${d}, #agents ${a}, #maxrounds ${r}, seed ${seed}, #starting positions ${sp}`;

            console.log("main.js: Calling runSimulation...");
            const data = await runSimulation(py, n, d, a, r, seed, sp);
            console.log("main.js: runSimulation returned.");

            if (!data || !data.positions || !data.statuses) {
                 throw new Error('Bad data received from runSimulation');
            }

            lastSimulationData = data; // Store the generated data
            console.log("main.js: Simulation data looks okay. Calling drawCytoscape...");
            drawCytoscape(cyId, data);
            console.log("main.js: drawCytoscape finished.");
            if (out) out.textContent = `Simulation complete. Displayed ${data.positions.length - 1} steps.`;

        } catch (err) {
            console.error("main.js: Error during handleRunSimulationClick execution:", err);
            if (out) out.textContent = `Error: ${err.message}`;
        } finally {
            setRunningState(false); // This will enable saveDataBtn if lastSimulationData is not null
            console.log("main.js: Run Simulation flow finished.");
        }
    };

    // --- Handle JSON Load ---
    const handleLoadJsonClick = async () => {
        console.log("main.js: handleLoadJsonClick invoked!");
        const file = jsonFileInput.files[0];

        if (!file) {
            if (out) out.textContent = "Please select a JSON file.";
            return;
        }

        lastSimulationData = null; // Clear previous data
        setRunningState(true);
        if (out) out.textContent = `Loading file: ${file.name}...`;
        console.log(`main.js: Loading file: ${file.name}`);

        // Wait for pyodideReady just to ensure environment is set up,
        // though technically not needed for file reading/parsing itself.
        // This simplifies button state management.
        try {
            await pyodideReady; // Ensure Pyodide is loaded before visualization attempt
            console.log("main.js: Pyodide ready (or already was).");

            const reader = new FileReader();

            reader.onload = (event) => {
                try {
                    console.log("main.js: File read successfully. Parsing JSON...");
                    const data = JSON.parse(event.target.result);
                    console.log("main.js: JSON parsed.", data);

                    // Basic validation
                    if (!data || !data.nodes || !data.edges || !data.positions) {
                        throw new Error("Invalid JSON structure. Missing required keys (nodes, edges, positions).");
                    }
                     if (!Array.isArray(data.positions) || data.positions.length === 0) {
                         throw new Error("Invalid positions data. Must be a non-empty array.");
                     }
                     // Less strict check for positions array structure after basic validation
                     if (!Array.isArray(data.positions[0]) || data.positions[0].length < 2 || !Array.isArray(data.positions[0][1])) {
                          console.warn("main.js: Positions array structure for first element might be malformed. Proceeding with rendering.", data.positions[0]);
                     }

                    lastSimulationData = data; // Store the loaded data
                    console.log("main.js: JSON structure validated. Calling drawCytoscape...");
                    drawCytoscape(cyId, data);
                    console.log("main.js: drawCytoscape finished.");
                    if (out) out.textContent = `File "${file.name}" loaded. Displayed ${data.positions.length - 1} steps.`;

                } catch (parseErr) {
                    console.error("main.js: Error processing JSON file:", parseErr);
                    if (out) out.textContent = `Error: ${parseErr.message}`;
                } finally {
                    setRunningState(false); // This will enable saveDataBtn if lastSimulationData is not null
                    console.log("main.js: JSON Load flow finished.");
                }
            };

            reader.onerror = (error) => {
                console.error("main.js: FileReader error:", error);
                if (out) out.textContent = `Error reading file: ${error.message}`;
                setRunningState(false);
                console.log("main.js: JSON Load flow finished with file error.");
            };

            reader.readAsText(file);

        } catch (pyodideErr) {
             console.error("main.js: Error during pyodideReady wait in JSON load:", pyodideErr);
             if (out) out.textContent = `Error initializing environment: ${pyodideErr.message}`;
             setRunningState(false);
             console.log("main.js: JSON Load flow finished with Pyodide error.");
        }
    };

    // --- Handle Save Data ---
    const handleSaveDataClick = () => {
        console.log("main.js: handleSaveDataClick invoked!");
        if (lastSimulationData) {
            const filename = generateTimestampFilename();
            triggerDownload(lastSimulationData, filename);
        } else {
            console.warn("main.js: Save button clicked but no simulation data is available.");
            const out = document.getElementById('output');
            if (out) out.textContent = "No data to save.";
        }
    };


    // --- Attach Event Listeners ---
    runBtn.removeEventListener('click', handleRunSimulationClick);
    loadJsonBtn.removeEventListener('click', handleLoadJsonClick);
    saveDataBtn.removeEventListener('click', handleSaveDataClick); // Add listener for the new button

    runBtn.addEventListener('click', handleRunSimulationClick);
    loadJsonBtn.addEventListener('click', handleLoadJsonClick);
    saveDataBtn.addEventListener('click', handleSaveDataClick); // Attach listener

    console.log("main.js: Attached click listeners to buttons.");

    // Initial message
    if (out) out.textContent = 'Click “Run Simulation” or "Load & Visualize" to start.';

    // Initial state update
    setRunningState(false); // Ensure buttons are correctly enabled/disabled on load

}); // End of DOMContentLoaded listener

console.log("main.js: Script end."); // Log script execution end