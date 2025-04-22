// main.js
import { pyodideReady } from './pyodide-setup.js';
import { runSimulation } from './simulation-runner.js';
import { drawCytoscape } from './cytoscape-visualizer.js';

// --- Configuration ---
const CYTOSCAPE_CONTAINER_ID = 'cy';
const OUTPUT_DIV_ID = 'output';
const RUN_BUTTON_ID = 'runBtn';

// --- End Configuration ---

/**
 * Generate random undirected graph edges with max degree constraint.
 * @param {number} numNodes
 * @param {number} maxDegree
 * @returns {number[][]}
 */
function generateRandomGraphEdges(numNodes, maxDegree) {
    if (maxDegree >= numNodes) {
        maxDegree = numNodes - 1;
        console.warn(`Max degree adjusted to ${maxDegree} (numNodes - 1)`);
    }
    if (maxDegree < 0) {
        return [];
    }

    const pairs = [];
    for (let i = 0; i < numNodes; i++) {
        for (let j = i + 1; j < numNodes; j++) {
            pairs.push([i, j]);
        }
    }
    // Fisher-Yates Shuffle
    for (let i = pairs.length - 1; i > 0; i--) {
        const j = Math.floor(Math.random() * (i + 1));
        [pairs[i], pairs[j]] = [pairs[j], pairs[i]];
    }

    const degrees = Array(numNodes).fill(0);
    const edges = [];
    for (const [u, v] of pairs) {
        if (degrees[u] < maxDegree && degrees[v] < maxDegree) {
            edges.push([u, v]);
            degrees[u]++;
            degrees[v]++;
        }
    }
    return edges;
}

const runBtn = document.getElementById(RUN_BUTTON_ID);
const outputDiv = document.getElementById(OUTPUT_DIV_ID);
const cyContainer = document.getElementById(CYTOSCAPE_CONTAINER_ID);
const nodeCountInput = document.getElementById('nodeCountInput');
const maxDegreeInput = document.getElementById('maxDegreeInput');
const agentCountInput = document.getElementById('agentCountInput');
const roundCountInput = document.getElementById('roundCountInput');

if (!runBtn || !outputDiv || !cyContainer || !nodeCountInput || !maxDegreeInput || !agentCountInput || !roundCountInput) {
    console.error("Required HTML elements not found. Check IDs.");
} else {
    runBtn.onclick = async () => {
        runBtn.disabled = true;
        outputDiv.textContent = "Loading Pyodide and Python packages...";
        cyContainer.innerHTML = "";

        try {
            const pyodide = await pyodideReady;
            outputDiv.textContent = "Pyodide ready. Running simulation...";

            // Read and validate inputs
            let numNodes = parseInt(nodeCountInput.value, 10);
            if (isNaN(numNodes) || numNodes <= 0) {
                console.warn("Invalid node count, defaulting to 5.");
                numNodes = 5;
                nodeCountInput.value = numNodes;
            }

            let maxDegree = parseInt(maxDegreeInput.value, 10);
            if (isNaN(maxDegree) || maxDegree < 0) {
                console.warn("Invalid max degree, defaulting to 2.");
                maxDegree = 2;
                maxDegreeInput.value = maxDegree;
            }
            if (maxDegree >= numNodes && numNodes > 1) {
                console.warn(`Max degree ${maxDegree} adjusted to ${numNodes - 1}`);
                maxDegree = numNodes - 1;
                maxDegreeInput.value = maxDegree;
            }

            let numAgents = parseInt(agentCountInput.value, 10);
            if (isNaN(numAgents) || numAgents <= 0) {
                console.warn("Invalid agent count, defaulting to 3.");
                numAgents = 3;
                agentCountInput.value = numAgents;
            }

            let numRounds = parseInt(roundCountInput.value, 10);
            if (isNaN(numRounds) || numRounds <= 0) {
                console.warn("Invalid round count, defaulting to 10.");
                numRounds = 10;
                roundCountInput.value = numRounds;
            }

            outputDiv.textContent = `Generating graph with ${numNodes} nodes, max degree ${maxDegree}, ${numAgents} agents, and ${numRounds} rounds...`;
            const graphEdges = generateRandomGraphEdges(numNodes, maxDegree);

            if (numNodes > 1 && graphEdges.length === 0 && maxDegree > 0) {
                console.warn("Graph generation resulted in no edges. Agents might not move.");
            }
            let nodeZeroDegree = 0;
            if (numNodes > 0) {
                for (const edge of graphEdges) {
                    if (edge[0] === 0 || edge[1] === 0) {
                        nodeZeroDegree++;
                    }
                }
                if (nodeZeroDegree === 0 && numNodes > 1) {
                    console.warn("Node 0 has no edges. Agents starting at node 0 will not move initially.");
                }
            }

            const simulationData = await runSimulation(
                pyodide,
                numNodes,
                graphEdges,
                numAgents,
                numRounds
            );
            outputDiv.textContent = "Simulation complete. Drawing graph...";

            if (!simulationData || !simulationData.positions || !simulationData.statuses) {
                throw new Error("Simulation did not return the expected data structure (positions/statuses).");
            }
            if (simulationData.positions.length !== simulationData.statuses.length) {
                throw new Error("Mismatch between lengths of position and status arrays returned from simulation.");
            }

            drawCytoscape(CYTOSCAPE_CONTAINER_ID, simulationData);
            const actualRoundsSimulated = simulationData.positions.length - 1;
            outputDiv.textContent = `Simulation complete. Displaying ${actualRoundsSimulated} rounds. Green agents are SETTLED.`;

        } catch (error) {
            console.error("Simulation or Visualization failed:", error);
            outputDiv.textContent = `Error: ${error.message}${error.stack ? `\nStack: ${error.stack}` : ''}\nCheck console for more details.`;
        } finally {
            runBtn.disabled = false;
        }
    };
}