let cy = null;
let animationTimeout = null;

/**
 * Draws the graph and animates agents using Cytoscape.
 * @param {string} containerId - The ID of the div element for Cytoscape.
 * @param {object} data - The simulation data ({nodes, edges, positions, statuses}).
 */
export function drawCytoscape(containerId, data) {
    if (cy) {
        cy.destroy();
        cy = null;
    }
    if (animationTimeout) {
        clearTimeout(animationTimeout);
        animationTimeout = null;
    }
    const container = document.getElementById(containerId);
    if (!container) {
        console.error(`Cytoscape container with ID "${containerId}" not found.`);
        return;
    }
    container.innerHTML = "";

    // Initialize round display
    const roundDisplay = document.getElementById('round-display');
    if (roundDisplay) {
        roundDisplay.textContent = 'Round: 0';
    } else {
        console.warn('Round display element (#round-display) not found.');
    }

    console.log("Initializing Cytoscape...");
    cy = cytoscape({
        container: container,
        elements: data.nodes.concat(data.edges),
        style: [
            { selector: 'node', style: { 'label': 'data(id)', 'background-color': 'lightblue', 'width': 25, 'height': 25, 'text-valign': 'center', 'text-halign': 'center', 'font-size': '10px' } },
            { selector: 'edge', style: { 'width': 2, 'line-color': '#ccc', 'label': 'data(label)', 'font-size': '8px', 'text-background-color': '#fff', 'text-background-opacity': 0.7, 'text-rotation': 'autorotate' } },
            { selector: '.agent', style: { 'shape': 'ellipse', 'background-color': 'red', 'width': 12, 'height': 12, 'label': 'data(label)', 'font-size': '8px', 'text-valign': 'center', 'text-halign': 'center', 'color': 'white', 'z-index': 10 } },
            { selector: '.agent.settled', style: { 'background-color': 'green' } }
        ],
        layout: {
            name: 'preset',
            positions: node => {
                const nodeEntry = data.nodes.find(n => n.data && n.data.id === node.id());
                return nodeEntry && nodeEntry.position;
            },
            animate: false,
            padding: 30
        },
    });

    cy.ready(() => {
        console.log("Cytoscape layout ready. Adding agents...");
        addAgents(cy, data.positions, data.statuses);
        console.log("Starting agent animation...");
        animateAgents(cy, data.positions, data.statuses);
    });
}

/**
 * Adds agent nodes to the Cytoscape graph at initial positions and sets initial status class.
 * @param {object} cyInstance - The Cytoscape instance.
 * @param {Array<Array<number|string>>} positions - Agent positions per round.
 * @param {Array<Array<string>>} statuses - Agent statuses per round ('SETTLED'/'UNSETTLED').
 */
function addAgents(cyInstance, positions, statuses) {
    if (!positions || positions.length === 0 || !statuses || statuses.length === 0) {
        console.warn("Cannot add agents: Missing positions or statuses data.");
        return;
    }

    const initialPositions = positions[0];
    const initialStatuses = statuses[0];
    const agentCount = initialPositions.length;

    if (initialPositions.length !== initialStatuses.length) {
        console.error("Mismatch between initial positions and statuses count.");
        return;
    }

    for (let i = 0; i < agentCount; i++) {
        const initialNodeId = String(initialPositions[i]);
        const initialStatus = initialStatuses[i];
        const targetNode = cyInstance.getElementById(initialNodeId);

        if (targetNode.length === 0) {
            console.warn(`Node with ID ${initialNodeId} not found for agent ${i}. Skipping agent.`);
            continue;
        }
        const pos = targetNode.position();

        const agentData = {
            data: { id: `agent_${i}`, label: `A${i}` },
            position: { x: pos.x, y: pos.y },
            classes: 'agent',
            grabbable: false
        };
        const newAgent = cyInstance.add(agentData);

        if (initialStatus === 'SETTLED') {
            newAgent.addClass('settled');
        }
        console.log(`Added agent ${i} at node ${initialNodeId} with status ${initialStatus}`);
    }
}

/**
 * Animates the agent nodes through the recorded positions and updates status class.
 * @param {object} cyInstance - The Cytoscape instance.
 * @param {Array<Array<number|string>>} positions - Agent positions per round.
 * @param {Array<Array<string>>} statuses - Agent statuses per round.
 */
function animateAgents(cyInstance, positions, statuses) {
    const totalRounds = positions.length;
    let currentRound = 1;
    const animationDuration = 800;
    const pauseDuration = 1000;

    if (!statuses || statuses.length !== totalRounds) {
        console.error("Animation aborted: Statuses data missing or length mismatch.");
        return;
    }

    const roundDisplay = document.getElementById('round-display');

    function animateRound() {
        if (currentRound >= totalRounds) {
            console.log("Animation finished.");
            const finalPositions = positions[totalRounds - 1];
            const finalStatuses = statuses[totalRounds - 1];
            finalPositions.forEach((_, agentIndex) => {
                const agentNode = cyInstance.getElementById(`agent_${agentIndex}`);
                if (agentNode.length > 0) {
                    if (finalStatuses[agentIndex] === 'SETTLED') {
                        agentNode.addClass('settled');
                    } else {
                        agentNode.removeClass('settled');
                    }
                }
            });
            if (roundDisplay) {
                roundDisplay.textContent = `Round: ${totalRounds - 1} (Finished)`;
            }
            return;
        }

        console.log(`Animating round ${currentRound}`);
        if (roundDisplay) {
            roundDisplay.textContent = `Round: ${currentRound}`;
        }

        const currentPositions = positions[currentRound];
        const currentStatuses = statuses[currentRound];
        const agentPromises = [];

        if (currentPositions.length !== currentStatuses.length) {
            console.error(`Data mismatch in round ${currentRound}. Aborting animation.`);
            clearTimeout(animationTimeout);
            return;
        }

        currentPositions.forEach((nodeId, agentIndex) => {
            const agentId = `agent_${agentIndex}`;
            const agentNode = cyInstance.getElementById(agentId);
            const targetNode = cyInstance.getElementById(String(nodeId));
            const agentStatus = currentStatuses[agentIndex];

            if (agentNode.length === 0) {
                console.warn(`Skipping animation for agent ${agentIndex}: agent node ${agentId} not found.`);
                return;
            }
            if (targetNode.length === 0) {
                console.warn(`Skipping animation for agent ${agentIndex}: target node ${nodeId} not found.`);
                return;
            }

            if (agentStatus === 'SETTLED') {
                agentNode.addClass('settled');
            } else {
                agentNode.removeClass('settled');
            }

            const targetPosition = targetNode.position();
            const currentAgentPos = agentNode.position();
            const distanceThreshold = 0.1;

            if (
                Math.abs(currentAgentPos.x - targetPosition.x) > distanceThreshold ||
                Math.abs(currentAgentPos.y - targetPosition.y) > distanceThreshold
            ) {
                const animationPromise = agentNode.animate({
                    position: { x: targetPosition.x, y: targetPosition.y }
                }, {
                    duration: animationDuration
                });
                agentPromises.push(animationPromise);
            } else {
                agentNode.position({ x: targetPosition.x, y: targetPosition.y });
            }
        });

        Promise.all(agentPromises).then(() => {
            console.log(`Round ${currentRound} animation complete.`);
            currentRound++;
            animationTimeout = setTimeout(animateRound, pauseDuration);
        }).catch(err => {
            console.error(`Error during animation promise for round ${currentRound}:`, err);
            clearTimeout(animationTimeout);
        });
    }

    animationTimeout = setTimeout(animateRound, 500);
}