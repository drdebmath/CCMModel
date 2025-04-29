// cytoscape-visualizer.js

let cy = null;
let animationTimeout = null;
const DEBUG = false;

/**
 * Draw graph + agents in Cytoscape.
 */
export function drawCytoscape(containerId, data) {
    if (cy) { cy.destroy(); clearTimeout(animationTimeout); }
    const container = document.getElementById(containerId);
    if (!container) { console.error(`No #${containerId}`); return; }
    container.innerHTML = '';

    const roundDisplay = document.getElementById('round-display');
    if (!roundDisplay && DEBUG) console.warn('#round-display missing');

    // Detect dark mode from <html> class
    const isDark = document.documentElement.classList.contains('dark');
    cy = cytoscape({
        container,
        elements: data.nodes.concat(data.edges),
        style: [
            {
                selector: 'node',
                style: {
                    label: 'data(id)',
                    'background-color': isDark ? '#334155' : 'lightblue', // slate-700 vs lightblue
                    width: 25, height: 25,
                    'text-valign': 'center',
                    'text-halign': 'center',
                    'font-size': 10,
                    color: isDark ? '#e2e8f0' : '#222', // slate-200 vs dark text
                }
            },
            {
                selector: 'node.has-unsettled',
                style: {
                    'background-color': isDark ? '#f59e42' : 'orange' // amber-500 vs orange
                }
            },
            {
                selector: 'edge',
                style: {
                    width: 2,
                    'line-color': isDark ? '#64748b' : '#ccc', // slate-400 vs light
                    'source-label': 'data(srcPort)',
                    'target-label': 'data(dstPort)',
                    'source-text-offset': 20,
                    'target-text-offset': 20,
                    'font-size': 8,
                    'text-background-color': isDark ? '#1e293b' : '#fff', // darkbg vs white
                    'text-background-opacity': 0.85,
                    color: isDark ? '#e2e8f0' : '#222',
                }
            },
            {
                selector: '.agent',
                style: {
                    shape: 'ellipse',
                    'background-color': isDark ? '#ef4444' : 'red', // red-500 vs red
                    width: 12, height: 12,
                    label: 'data(label)',
                    'font-size': 8,
                    'text-valign': 'center',
                    'text-halign': 'center',
                    color: isDark ? '#f1f5f9' : 'white', // slate-100 vs white
                    'z-index': 10
                }
            },
            {
                selector: '.agent.settled',
                style: { 'background-color': isDark ? '#22c55e' : 'green' } // green-500 vs green
            }
        ],
        layout: {
            name: 'preset',
            positions: node => {
                const n = data.nodes.find(n => n.data.id === node.id());
                return n ? n.position : null;
            }
        }
    });

    cy.ready(() => {
        addAgents(data.positions[0], data.statuses[0]);
        updateNodeStyles(data.positions[0], data.statuses[0]);
        const animDuration = parseInt(document.getElementById('animationDurationInput').value, 10) || 200;
        animateAgents(data.positions, data.statuses, animDuration);
    });
}

function addAgents(initialPositions, initialStatuses) {
    initialPositions.forEach((nodeId, i) => {
        const pos = cy.getElementById(String(nodeId)).position();
        if (!pos) return;
        const agent = cy.add({
            data: { id: `a${i}`, label: `A${i}` },
            position: { x: pos.x, y: pos.y },
            classes: 'agent',
            grabbable: false
        });
        if (initialStatuses[i] === 1) {
            agent.addClass('settled');
        }
    });
}

/**
 * Color nodes with unsettled agents.
 */
function updateNodeStyles(positionsRound, statusesRound) {
    // Clear previous unsettled markers
    cy.nodes().removeClass('has-unsettled');
    // Mark any node that hosts at least one unsettled agent
    statusesRound.forEach((status, i) => {
        if (status === 1) {
            const nodeId = positionsRound[i];
            const node = cy.getElementById(String(nodeId));
            if (node) node.addClass('has-unsettled');
        }
    });
}

function animateAgents(positions, statuses, animDuration) {
    let round = 1;
    const total = positions.length;
    const duration = animDuration || 200;
    const pause = animDuration || 200;

    function step() {
        if (round >= total) {
            document.getElementById('round-display').textContent = `Round: ${total - 1} (Done)`;
            return;
        }
        document.getElementById('round-display').textContent = `Round: ${round}`;

        // Update node colors for this round
        updateNodeStyles(positions[round], statuses[round]);

        positions[round].forEach((nodeId, i) => {
            const agent = cy.getElementById(`a${i}`);
            const target = cy.getElementById(String(nodeId)).position();
            if (!agent || !target) return;
            if (statuses[round][i] === 0) {
                agent.addClass('settled');
            } else {
                agent.removeClass('settled');
            }
            agent.animate({ position: { x: target.x, y: target.y } }, { duration });
        });
        round++;
        animationTimeout = setTimeout(step, duration + pause);
    }
    animationTimeout = setTimeout(step, 100);
}

// Find where animateAgents is called and pass the animation duration from the input
// Example: animateAgents(positions, statuses, parseInt(document.getElementById('animationDurationInput').value, 10) || 200);
// If this is called elsewhere, update those calls accordingly.
