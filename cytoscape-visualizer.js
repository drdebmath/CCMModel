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
                    'background-color': isDark ? '#3b82f6' : '#93c5fd', // blue-500 vs blue-300
                    width: 30,
                    height: 30,
                    'text-valign': 'center',
                    'text-halign': 'center',
                    'font-size': 12,
                    'color': isDark ? '#f1f5f9' : '#1e293b', // no background behind text
                    'font-weight': '600'
                }
            },
            {
                selector: 'node.has-unsettled',
                style: {
                    'background-color': isDark ? '#facc15' : 'orange' // yellow-400 vs orange
                }
            },
            {
                selector: 'edge',
                style: {
                    width: 2,
                    'line-color': isDark ? '#94a3b8' : '#cbd5e1', // slate-400 vs light gray
                    'target-arrow-color': isDark ? '#94a3b8' : '#cbd5e1',
                    'source-label': 'data(srcPort)',
                    'target-label': 'data(dstPort)',
                    'source-text-offset': 25,
                    'target-text-offset': 25,
                    'font-size': 9,
                    'text-background-color': isDark ? '#0f172a' : '#ffffff', // edge label backgrounds only
                    'text-background-opacity': 0.9,
                    'text-background-shape': 'roundrectangle',
                    'color': isDark ? '#f1f5f9' : '#1e293b'
                }
            },
            {
                selector: '.agent',
                style: {
                    shape: 'ellipse',
                    'background-color': isDark ? '#f87171' : '#ef4444', // red-400 vs red-500
                    width: 20,     
                    height: 20,    
                    label: 'data(label)',
                    'font-size': 10,
                    'text-valign': 'center',
                    'text-halign': 'center',
                    'color': isDark ? '#f9fafb' : '#ffffff',
                    'text-background-opacity': 0, 
                    'z-index': 10
                }
            },
            {
                selector: '.agent.settled',
                style: {
                    'background-color': isDark ? '#22d3ee' : '#10b981' // cyan-400 vs green-500
                }
            },
            {
                selector: '.agent.settled_wait',
                style: {
                    'background-color': isDark ? '#fbbf24' : '#f59e0b' // yellow-400 vs yellow-500
                }
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
        if (initialStatuses[i] === 2) {
            agent.addClass('settled_wait');
        }
    });
}

/**
 * Color nodes with unsettled agents.
 */
function updateNodeStyles(positionsRound, statusesRound) {
    cy.nodes().removeClass('has-unsettled');
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
            if (statuses[round][i] === 2) {
                agent.addClass('settled_wait');
            } else {
                agent.removeClass('settled_wait');
            }
            agent.animate({ position: { x: target.x, y: target.y } }, { duration });
        });

        round++;
        animationTimeout = setTimeout(step, duration + pause);
    }

    animationTimeout = setTimeout(step, 100);
}
