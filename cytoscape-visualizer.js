// cytoscape-visualizer.js

let cy = null;
let animationTimeout = null;
const DEBUG = false;

/**
 * Robustly compute an HSL color for an agent based on its leader ID and level.
 * - Hue: quasi-evenly distributed by leaderId (golden-ratio multiplier)
 * - Lightness: darker as the level increases
 *
 * Any non-finite or missing value gracefully degrades to a safe default so we
 * never emit an invalid `hsl(NaN, …)` string that Cytoscape will reject.
 */
function computeAgentColor (leaderId, level) {
  const id   = Number(leaderId);
  const lvl  = Number(level);

  // Fallbacks to prevent NaN
  const safeId  = Number.isFinite(id)  ? id  : 0;
  const safeLvl = Number.isFinite(lvl) ? lvl : 0;

  const hue       = (safeId * 137.5) % 360;       // golden-angle spacing
  const rawLight  = 80 - safeLvl * 15;            // base 80 → darker per level
  const lightness = Math.max(20, Math.min(80, rawLight));

  return `hsl(${hue}, 70%, ${lightness}%)`;
}

/**
 * Draw graph + animate agents in Cytoscape.
 */
export function drawCytoscape (containerId, data) {
  if (cy) {
    cy.destroy();
    if (animationTimeout) clearTimeout(animationTimeout); // Clear existing timeout
  }

  const container = document.getElementById(containerId);
  if (!container) {
    console.error(`No #${containerId}`);
    return;
  }
  container.innerHTML = ''; // Clear previous drawing

  const roundDisplay = document.getElementById('round-display');
  if (!roundDisplay && DEBUG) console.warn('#round-display missing');

  // Detect dark-mode state from <html class="… dark …">
  const isDark = document.documentElement.classList.contains('dark');

  cy = cytoscape({
    container,
    elements: data.nodes.concat(data.edges),
    style: [
      // base nodes
      {
        selector: 'node',
        style: {
          label: 'data(id)',
          'background-color': isDark ? '#3b82f6' : '#93c5fd',
          width: 30,
          height: 30,
          'text-valign': 'center',
          'text-halign': 'center',
          'font-size': 12,
          color: isDark ? '#f1f5f9' : '#1e293b',
          'font-weight': 600
        }
      },
      // nodes with at least one unsettled agent
      {
        selector: 'node.has-unsettled',
        style: {
          // Use a distinct color for unsettled nodes
          'background-color': isDark ? '#facc15' : '#fbbf24' // yellow-400 or amber-400
        }
      },
      // edges
      {
        selector: 'edge',
        style: {
          width: 2,
          'line-color': isDark ? '#94a3b8' : '#cbd5e1', // slate-400 or slate-300
          'target-arrow-shape': 'none', // No arrows for undirected look
          'source-arrow-shape': 'none',
          // Labels for ports can make it cluttered, maybe only show on hover later
          'source-label': 'data(srcPort)',
          'target-label': 'data(dstPort)',
          'source-text-offset': 25,
          'target-text-offset': 25,
          'font-size': 9,
          'text-background-color': isDark ? '#0f172a' : '#ffffff', // slate-900 or white
          'text-background-opacity': 0.9,
          'text-background-padding': '2px',
          'text-background-shape': 'roundrectangle',
          color: isDark ? '#f1f5f9' : '#1e293b' // slate-100 or slate-800
        }
      },
      // agents
      {
        selector: '.agent',
        style: {
          shape: 'ellipse',
          'background-color': ele => ele.data('agentColor') || (isDark ? '#e2e8f0' : '#475569'), // Default color if missing
          width: 26,
          height: 26,
          label: 'data(label)',
          'font-size': 10, // Slightly smaller font for agents
          'text-valign': 'center',
          'text-halign': 'center',
          color: '#ffffff', // White text usually works on colored backgrounds
          'font-weight': 'bold',
          'text-outline-width': 1, // Add outline for better contrast
          'text-outline-color': isDark ? '#1e293b' : '#475569', // Dark outline
          'z-index': 10 // Ensure agents are above edges/nodes
        }
      },
      // settled agent: solid border matching its color
      {
        selector: '.agent.settled',
        style: {
          'border-width': 3, // Thicker border for settled
          'border-color': ele => ele.data('agentColor') || (isDark ? '#10b981' : '#059669') // Use agent color or fallback green
        }
      },
      // settled_wait agent: dashed border matching its color
      {
        selector: '.agent.settled_wait',
        style: {
          'border-width': 3,
          'border-style': 'dashed',
          'border-color': ele => ele.data('agentColor') || (isDark ? '#f59e0b' : '#d97706') // Use agent color or fallback amber
        }
      }
    ],
    layout: {
      name: 'preset', // Use preset layout based on calculated positions
      positions: node => {
        const nData = data.nodes.find(n => n.data.id === node.id());
        // Provide a default position if somehow missing
        return nData ? nData.position : { x: 0, y: 0 };
      },
       zoom: 1, // Adjust initial zoom if needed
       pan: { x: 0, y: 0 }, // Adjust initial panning if needed
    }
  });

  cy.ready(() => {
    // Ensure initial data exists and has the correct structure
    const initialPositions = data.positions?.[0]?.[1] || [];
    const initialStatuses  = data.statuses?.[0]?.[1]  || [];
    const initialLeaders   = data.leaders?.[0]?.[1]   || [];
    const initialLevels    = data.levels?.[0]?.[1]    || [];

    console.log("Initial Positions:", initialPositions);
    console.log("Initial Statuses:", initialStatuses);

    if (initialPositions.length === 0) {
        console.warn("No initial positions found in data.");
        // Optionally display an error to the user
        if(roundDisplay) roundDisplay.textContent = "Error: No initial agent data.";
        return;
    }

    addAgents(
      initialPositions,
      initialStatuses,
      initialLeaders,
      initialLevels
    );
    updateNodeStyles(initialPositions, initialStatuses);

    cy.fit(undefined, 50); // Fit the viewport to the graph

    const animDurationInput = document.getElementById('animationDurationInput');
    const animDuration = animDurationInput ? parseInt(animDurationInput.value, 10) : 300;
    if (isNaN(animDuration) || animDuration < 50) {
        console.warn("Invalid animation duration, defaulting to 300ms");
        animDuration = 300;
    }


    animateAgents(
      data.positions,
      data.statuses,
      data.leaders,
      data.levels,
      animDuration
    );
  });
}

/**
 * Create agent glyphs for the initial round (step 0).
 */
function addAgents (posRound, statRound, leaderRound, levelRound) {
    if (!cy) {
        console.error("Cytoscape instance (cy) not available in addAgents.");
        return;
    }
    console.log(`Adding ${posRound.length} agents for initial state.`);

    posRound.forEach((nodeId, i) => {
        const nodeElement = cy.getElementById(String(nodeId));
        if (!nodeElement || nodeElement.length === 0) {
            console.warn(`Node element with ID ${nodeId} not found for agent A${i}. Skipping agent.`);
            return; // Skip if the target node doesn't exist
        }
        const pos = nodeElement.position();
        if (!pos) {
             console.warn(`Could not get position for node ${nodeId}. Skipping agent A${i}.`);
             return; // Skip if position invalid
        }

        const leaderId = leaderRound?.[i];
        const level = levelRound?.[i];
        const clr = computeAgentColor(leaderId, level);
        const status = statRound[i]; // 0: SETTLED, 1: UNSETTLED, 2: SETTLED_WAIT

        const agentData = {
            id:         `a${i}`,
            label:      `A${i}`, // Agent ID label
            agentColor: clr,
            leader:     leaderId,
            level:      level,
        };

        const agent = cy.add({
            data: agentData,
            position: { x: pos.x, y: pos.y }, // Start at the node's position
            classes: 'agent',
            grabbable: false, // Agents shouldn't be manually moved
        });

        // Apply status classes
        if (status === 0) agent.addClass('settled');
        else if (status === 2) agent.addClass('settled_wait');
        // No specific class needed for UNSETTLED (default appearance)

        // Debug log for each agent added
        if (DEBUG) console.log(`Added agent a${i} at node ${nodeId}, pos: (${pos.x.toFixed(1)}, ${pos.y.toFixed(1)}), color: ${clr}, status: ${status}`);
    });
    // Force a layout update or redraw if needed, though Cytoscape usually handles it
    // cy.layout({ name: 'preset' }).run(); // Re-run preset layout maybe?
}


/**
 * Highlight nodes currently hosting any UNSETTLED agents.
 */
function updateNodeStyles (posRound, statRound) {
    if (!cy) return;

    // Reset all nodes first
    cy.nodes().removeClass('has-unsettled');

    // Add class to nodes with unsettled agents
    statRound.forEach((status, i) => {
        // Status 1 corresponds to UNSETTLED
        if (status === 1) {
            const nodeId = String(posRound[i]);
            const node = cy.getElementById(nodeId);
            if (node && node.length > 0) { // Check if node exists
                node.addClass('has-unsettled');
            } else if (DEBUG) {
                console.warn(`Node ${nodeId} not found when trying to mark as unsettled for agent A${i}`);
            }
        }
    });
}


/**
 * Animate agents through all recorded rounds (steps).
 */
function animateAgents (positions, statuses, leaders, levels, animDuration) {
  // Start from step 1 (index 1) since step 0 is the initial state
  let stepIndex = 1;
  const totalSteps = positions.length; // Total number of snapshots including initial
  const duration = Math.max(50, animDuration); // Ensure minimum duration
  const pause = duration * 0.5; // Pause between steps, e.g., 50% of duration

  const roundDisplay = document.getElementById('round-display');

  function step () {
    if (!cy) {
        console.error("Cytoscape instance lost during animation.");
        return; // Stop if cy is gone
    }

    if (stepIndex >= totalSteps) {
      if (roundDisplay) {
          const finalLabel = positions[totalSteps - 1][0]; // Label of the last step
          roundDisplay.textContent = `Done: ${finalLabel} (Step ${totalSteps - 1})`;
      }
      console.log("Animation finished.");
      animationTimeout = null; // Clear timeout reference
      return; // End of animation
    }

    // --- Get data for the CURRENT step ---
    // Ensure data exists for the current step index
    const currentStepData = positions[stepIndex];
    if (!currentStepData || !Array.isArray(currentStepData) || currentStepData.length < 2) {
        console.error(`Invalid data structure at step index ${stepIndex}. Stopping animation.`);
        if(roundDisplay) roundDisplay.textContent = `Error at step ${stepIndex}`;
        animationTimeout = null;
        return;
    }

    const currentLabel = currentStepData[0];
    const currentPositions = currentStepData[1];
    // Safely get other data, providing empty arrays as fallbacks
    const currentStatuses = statuses?.[stepIndex]?.[1] || [];
    const currentLeaders  = leaders?.[stepIndex]?.[1] || [];
    const currentLevels   = levels?.[stepIndex]?.[1] || [];

     // Check if data arrays have content
    if (!Array.isArray(currentPositions) || currentPositions.length === 0) {
        console.error(`No position data found for step ${stepIndex} (${currentLabel}). Stopping animation.`);
         if(roundDisplay) roundDisplay.textContent = `Error: No position data at step ${stepIndex}`;
         animationTimeout = null;
        return;
    }


    if (roundDisplay) {
        roundDisplay.textContent = `Step: ${stepIndex}/${totalSteps - 1} (${currentLabel})`;
    }

    // Update node highlighting based on current statuses
    updateNodeStyles(currentPositions, currentStatuses);

    // --- Animate each agent to its position in the CURRENT step ---
    currentPositions.forEach((nodeId, i) => {
      const agentId = `a${i}`;
      const agent = cy.getElementById(agentId);
      if (!agent || agent.length === 0) {
        if (DEBUG) console.warn(`Agent ${agentId} not found during animation step ${stepIndex}.`);
        return; // Skip if agent doesn't exist
      }

      // Update agent color based on current leader/level
      const leaderId = currentLeaders[i];
      const level = currentLevels[i];
      const clr = computeAgentColor(leaderId, level);
      agent.data('agentColor', clr); // Update data
      // agent.style('background-color', clr); // Update style directly - might be redundant if classes handle it

       // Update agent status classes
      const status = currentStatuses[i];
      agent.removeClass('settled settled_wait'); // Clear previous status classes
      if (status === 0) agent.addClass('settled');
      else if (status === 2) agent.addClass('settled_wait');

      // Get the target node and its position
      const targetNodeId = String(nodeId);
      const targetNode = cy.getElementById(targetNodeId);
      if (!targetNode || targetNode.length === 0) {
         if (DEBUG) console.warn(`Target node ${targetNodeId} for agent ${agentId} not found at step ${stepIndex}. Agent will not move.`);
         return; // Skip animation if target node doesn't exist
      }
      const targetPosition = targetNode.position();
       if (!targetPosition) {
           if (DEBUG) console.warn(`Could not get position for target node ${targetNodeId} at step ${stepIndex}. Agent ${agentId} will not move.`);
           return; // Skip if position is invalid
       }

      // Animate the agent to the target node's position
      agent.animate(
        { position: { x: targetPosition.x, y: targetPosition.y } },
        { duration: duration } // Use the calculated animation duration
      );
    });

    // Increment step index for the next iteration
    stepIndex++;

    // Schedule the next step after the pause
    animationTimeout = setTimeout(step, duration + pause);
  }

  // Kick-off the animation after a short delay
  // Clear any previous timeout just in case
  if (animationTimeout) clearTimeout(animationTimeout);
  animationTimeout = setTimeout(step, 100); // Initial delay
}