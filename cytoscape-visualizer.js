// cytoscape-visualizer.js

let cy = null; // Cytoscape instance
let animationTimeout = null; // setTimeout handle
const DEBUG = false;

// --- Animation State ---
let isPaused = true;
let currentFilteredStep = 0; // Index of the *next* step to display in the filtered sequence
let totalFilteredSteps = 0; // Total steps after filtering (Scout/Chase/Follow)
let animDuration = 300; // Default animation duration per step
let pauseDuration = 150; // Default pause between steps

// --- Data Storage ---
let filteredPositions = [];
let filteredStatuses = [];
let filteredLeaders = [];
let filteredLevels = [];
let filteredNodeStates = []; // <--- ADDED: To store node state info per step
let originalNodes = [];
let originalEdges = [];

// --- UI Element References ---
let roundDisplay = null;
let playPauseBtn = null;
let nextStepBtn = null;
let showAgentsCheck = null;
let nodeTooltip = null;

// --- Helper Functions ---

/**
 * Robustly compute an HSL color for an agent based on its leader ID and level.
 */
function computeAgentColor(leaderId, level) {
  const id = Number(leaderId);
  const lvl = Number(level);
  const safeId = Number.isFinite(id) ? id : 0;
  const safeLvl = Number.isFinite(lvl) ? lvl : 0;
  const hue = (safeId * 137.5) % 360;
  const rawLight = 80 - safeLvl * 15;
  const lightness = Math.max(20, Math.min(80, rawLight));
  return `hsl(${hue}, 70%, ${lightness}%)`;
}

/**
 * Filters the simulation data based on Scout/Chase/Follow flags.
 * This determines *which steps* are included in the animation sequence.
 * It does NOT filter based on the "Show Agents" toggle.
 */
function filterSimulationDataForSteps(originalData, flags) {
  const { showScout, showChase, showFollow } = flags;
  const positions = [];
  const statuses = [];
  const leaders = [];
  const levels = [];
  const nodeStates = []; // <--- ADDED: Initialize temporary array for node states

  // Check if essential data exists
  if (!originalData.positions || originalData.positions.length === 0) {
    // Return empty structure including nodeStates
    return { positions, statuses, leaders, levels, nodeStates };
  }

  // Always keep the initial state (step 0)
  positions.push(originalData.positions[0]);
  if (originalData.statuses?.[0]) statuses.push(originalData.statuses[0]);
  if (originalData.leaders?.[0]) leaders.push(originalData.leaders[0]);
  if (originalData.levels?.[0]) levels.push(originalData.levels[0]);
  if (originalData.node_settled_states?.[0]) nodeStates.push(originalData.node_settled_states[0]); // <--- ADDED: Keep initial node state

  // Iterate through the rest of the steps
  for (let i = 1; i < originalData.positions.length; i++) {
    const label = originalData.positions[i][0].toLowerCase();
    let keepStep = true;

    // Apply Scout/Chase/Follow filters
    if (label.includes("scout") && !showScout) keepStep = false;
    else if (label.includes("chase") && !showChase) keepStep = false;
    else if (label.includes("follow") && !showFollow) keepStep = false;

    if (keepStep) {
      positions.push(originalData.positions[i]);
      if (originalData.statuses?.[i]) statuses.push(originalData.statuses[i]);
      if (originalData.leaders?.[i]) leaders.push(originalData.leaders[i]);
      if (originalData.levels?.[i]) levels.push(originalData.levels[i]);
      if (originalData.node_settled_states?.[i]) nodeStates.push(originalData.node_settled_states[i]); // <--- ADDED: Keep node state if step is kept
    } else {
      if (DEBUG)
        console.log(
          `Filtering out step ${i} (${originalData.positions[i][0]}) due to Scout/Chase/Follow flag.`
        );
    }
  }

  console.log(
    `filterSimulationDataForSteps: Original steps: ${originalData.positions.length}, Steps included in animation: ${positions.length}`
  );
  // Return the filtered node states as well
  return { positions, statuses, leaders, levels, nodeStates }; // <--- MODIFIED: Return nodeStates
}


/**
 * Updates the visibility of agent elements based on the "Show Agents" checkbox.
 */
function updateAgentVisibility() {
  if (!cy) return;
  const show = showAgentsCheck?.checked ?? true;
  // console.log(`updateAgentVisibility: Setting agent opacity to ${show ? 1 : 0}`);
  cy.elements(".agent").style({
    opacity: show ? 1 : 0, // Control visibility via opacity
  });
  // Also update node highlighting based on the *current step's* data
  // Use Math.max to prevent negative index if currentFilteredStep is 0
  const safeStepIndex = Math.max(0, Math.min(currentFilteredStep, totalFilteredSteps - 1));
  if (filteredPositions.length > safeStepIndex) {
    updateNodeStyles(
      filteredPositions[safeStepIndex][1],
      filteredStatuses[safeStepIndex]?.[1] || []
    );
  }
}

/**
 * Highlight nodes currently hosting any UNSETTLED agents.
 * Only highlights if agents are currently visible.
 */
function updateNodeStyles(posRound, statRound) {
  if (!cy) return;

  const agentsVisible = showAgentsCheck?.checked ?? true;
  cy.nodes().removeClass("has-unsettled"); // Reset all

  if (agentsVisible && posRound && statRound) { // Add checks for posRound/statRound
    statRound.forEach((status, i) => {
      if (status === 1 && i < posRound.length) { // Ensure index is valid for posRound
        // 1: UNSETTLED
        const nodeId = String(posRound[i]);
        const node = cy.getElementById(nodeId);
        if (node?.length > 0) {
          node.addClass("has-unsettled");
        } else if (DEBUG) {
          console.warn(
            `updateNodeStyles: Node ${nodeId} not found for unsettled agent A${i}`
          );
        }
      }
    });
  }
}

/**
 * Updates the text and disabled state of control buttons.
 */
function updateControlStates() {
  if (!playPauseBtn || !nextStepBtn) return;

  const atEnd = currentFilteredStep >= totalFilteredSteps - 1; // True if on the last step or beyond

  if (isPaused) {
    playPauseBtn.textContent =
      atEnd && totalFilteredSteps > 1 ? "🔄 Reset" : "▶️ Play";
    playPauseBtn.disabled = totalFilteredSteps <= 1; // Disable Play/Reset if only initial state
  } else {
    playPauseBtn.textContent = "⏸️ Pause";
    playPauseBtn.disabled = false; // Pause is always enabled when playing
  }
  // Disable Next if on the last step, or if playing
  nextStepBtn.disabled =
    currentFilteredStep >= totalFilteredSteps - 1 || !isPaused;
}

/**
 * Updates the round/step display text.
 */
function updateDisplay() {
  if (!roundDisplay) return;
  if (totalFilteredSteps === 0) {
    roundDisplay.textContent = "No simulation data.";
    return;
  }
  if (totalFilteredSteps === 1) {
    const label = filteredPositions[0]?.[0] || "Initial State"; // Safer access
    roundDisplay.textContent = `Initial State: ${label} (No steps to animate)`;
    return;
  }

  const currentLabel = filteredPositions[currentFilteredStep]?.[0] ?? "End";
  // Clamp step number for display to be within [0, totalFilteredSteps - 1]
  const stepNum = Math.max(
    0,
    Math.min(currentFilteredStep, totalFilteredSteps - 1)
  );
  roundDisplay.textContent = `Step: ${stepNum} / ${
    totalFilteredSteps - 1
  } (${currentLabel})`;

  // Explicitly handle display when animation finishes (currentFilteredStep might become >= totalFilteredSteps)
  if (currentFilteredStep >= totalFilteredSteps && totalFilteredSteps > 0) { // Add check for totalFilteredSteps > 0
    const finalLabel = filteredPositions[totalFilteredSteps - 1]?.[0] || "Final"; // Safer access
    roundDisplay.textContent = `Done: ${finalLabel} (Step ${
      totalFilteredSteps - 1
    }/${totalFilteredSteps - 1})`;
  }
}

/**
 * Updates the content and position of the node hover tooltip.
 */
function updateTooltip(node, event) {
    if (!nodeTooltip || !node || !node.length) {
        hideTooltip();
        return;
    }

    const nodeId = node.id(); // nodeId is already a string
    // Use the current step index, ensuring it's valid [0, totalFilteredSteps - 1]
    const stepIndex = Math.max(
        0,
        Math.min(currentFilteredStep, totalFilteredSteps - 1)
    );

    let content = `<strong>Node ${nodeId}</strong><br>`;
    let foundSettled = false;

    // Ensure agent list data exists for the current step index
    if (
        filteredPositions.length > stepIndex &&
        filteredStatuses.length > stepIndex &&
        filteredLeaders.length > stepIndex &&
        filteredLevels.length > stepIndex
    ) {
        const positionsAtStep = filteredPositions[stepIndex][1];
        const statusesAtStep = filteredStatuses[stepIndex]?.[1] || [];
        const leadersAtStep = filteredLeaders[stepIndex]?.[1] || [];
        const levelsAtStep = filteredLevels[stepIndex]?.[1] || [];

        // --- Access Node State Data for the current step ---
        // filteredNodeStates[stepIndex] = [label, {nodeId: stateData, ...}]
        // So filteredNodeStates[stepIndex]?.[1] gets the dictionary of node states for this step
        const nodeStatesDict = filteredNodeStates[stepIndex]?.[1] || {};
        // Get the specific state data for the hovered node ID from the dictionary
        const settledState = nodeStatesDict[nodeId];

        // --- Find agent(s) at this node with SETTLED (0) or SETTLED_WAIT (2) status ---
        for (let i = 0; i < positionsAtStep.length; i++) {
            if (String(positionsAtStep[i]) === nodeId) {
                // Agent i is at the hovered node
                const status = statusesAtStep[i];
                if (status === 0 || status === 2) {
                    // AgentStatus["SETTLED"] or AgentStatus["SETTLED_WAIT"]
                    const leaderId = leadersAtStep[i];
                    const level = levelsAtStep[i];
                    const statusText = status === 0 ? "SETTLED" : "SETTLED_WAIT";

                    // --- Get specific state values from settledState (found above) ---
                    // Use nullish coalescing operator (??) for safe defaults if state or property is missing
                    // IMPORTANT: Ensure these keys match EXACTLY what's sent from Python's snapshot()
                    const parentPort = settledState?.parent_port ?? '(N/A)';
                    const checkedPort = settledState?.checked_port ?? '(N/A)';
                    const maxScoutedPort = settledState?.max_scouted_port ?? '(N/A)';
                    const nextPort = settledState?.next_port ?? '(N/A)'; // Python uses 'next_port' key

                    // --- Build Tooltip Content ---
                    content += `Settled Agent: A${i}<br>`;
                    content += `  Status: ${statusText}<br>`; // Use   for indentation
                    content += `  Leader: A${leaderId}<br>`;
                    content += `  Level: ${level}<br>`;
                    // --- Display actual node state data ---
                    content += `  Parent Port: ${parentPort}<br>`;
                    content += `  Checked Port: ${checkedPort}<br>`;
                    content += `  Max Scouted: ${maxScoutedPort}<br>`;
                    content += `  Next Port: ${nextPort}`;
                    // --- End actual data display ---

                    foundSettled = true;
                    break; // Assume only one settled agent per node for tooltip simplicity
                }
            }
        }
    } // End check for valid agent list data at stepIndex

    if (!foundSettled) {
        content += "No settled agent at this step.";
    }

    nodeTooltip.innerHTML = content;

    // Position the tooltip near the mouse event
    const padding = 15; // Increased padding
    let x = event.originalEvent.pageX + padding;
    let y = event.originalEvent.pageY + padding;

    // Basic boundary check to keep tooltip on screen
    const tooltipRect = nodeTooltip.getBoundingClientRect(); // Use this after setting innerHTML
    if (x + tooltipRect.width > window.innerWidth) {
        x = event.originalEvent.pageX - tooltipRect.width - padding;
    }
    if (y + tooltipRect.height > window.innerHeight) {
        y = event.originalEvent.pageY - tooltipRect.height - padding;
    }
    // Prevent negative coordinates
    if (x < 0) x = padding;
    if (y < 0) y = padding;

    nodeTooltip.style.left = `${x}px`;
    nodeTooltip.style.top = `${y}px`;
    nodeTooltip.classList.remove("hidden");
}


/**
 * Hides the node hover tooltip.
 */
function hideTooltip() {
  if (nodeTooltip) {
    nodeTooltip.classList.add("hidden");
  }
}

/**
 * Performs the visual updates and animation for a single step.
 */
function doStepAnimation() {
  // Check if we are trying to animate beyond the last step
  if (!cy || currentFilteredStep >= totalFilteredSteps) {
    console.log(
      `doStepAnimation: Reached end or invalid state (Step: ${currentFilteredStep}, Total: ${totalFilteredSteps}). Stopping.`
    );
    // Don't pause if already paused, check if totalFilteredSteps > 0 before pausing
    if (!isPaused && totalFilteredSteps > 0) {
        pauseAnimation(); // Ensure state is paused
    }
    updateDisplay(); // Update display to show final state
    updateControlStates(); // Update controls for end state
    return;
  }

  console.log(`doStepAnimation: Executing step ${currentFilteredStep}`);

  // --- Get data for the current step ---
  const stepData = filteredPositions[currentFilteredStep];
  const currentLabel = stepData[0];
  const currentPositions = stepData[1];
  const currentStatuses = filteredStatuses?.[currentFilteredStep]?.[1] || [];
  const currentLeaders = filteredLeaders?.[currentFilteredStep]?.[1] || [];
  const currentLevels = filteredLevels?.[currentFilteredStep]?.[1] || [];

  // --- Update display and node styles ---
  updateDisplay();
  updateNodeStyles(currentPositions, currentStatuses);

  // --- Animate each agent ---
  currentPositions.forEach((nodeId, i) => {
    const agentId = `a${i}`;
    const agent = cy.getElementById(agentId);
    if (!agent || agent.length === 0) {
      // This might happen if agentCount > number of agents in data
      if (DEBUG)
        console.warn(
          `doStepAnimation: Agent element ${agentId} not found at step ${currentFilteredStep}.`
        );
      return;
    }

    // Update agent color/data even if hidden
    const leaderId = i < currentLeaders.length ? currentLeaders[i] : undefined; // Safer access
    const level = i < currentLevels.length ? currentLevels[i] : undefined; // Safer access
    const clr = computeAgentColor(leaderId, level);
    agent.data({ agentColor: clr, leader: leaderId, level: level });

    // Update agent status classes (border/style)
    const status = i < currentStatuses.length ? currentStatuses[i] : undefined; // Safer access
    agent.removeClass("settled settled_wait");
    if (status === 0) agent.addClass("settled");
    else if (status === 2) agent.addClass("settled_wait");

    // Get target node position
    const targetNodeId = String(nodeId);
    const targetNode = cy.getElementById(targetNodeId);
    const targetPosition = targetNode?.position();

    if (targetNode && targetPosition) {
      // Animate the agent to the target node's position
      agent.stop(true, false); // Stop previous animation, don't jump to end
      agent.animate(
        { position: { x: targetPosition.x, y: targetPosition.y } },
        { duration: animDuration }
      );
    } else {
      if (DEBUG)
        console.warn(
          `doStepAnimation: Target node ${targetNodeId} or position not found for agent ${agentId}. Agent won't move.`
        );
      // If target doesn't exist, maybe just put agent at origin? Or hide it?
      // agent.position({ x: 0, y: 0 }); // Move to origin as fallback
    }
  });
  // Ensure agent visibility is correct *after* data updates and animations start
  updateAgentVisibility();
}

/**
 * Schedules the next animation step if playing.
 */
function scheduleNextStep() {
  if (animationTimeout) clearTimeout(animationTimeout);
  animationTimeout = null;

  // Check if paused or if the *next* step would be beyond the end
  if (!isPaused && currentFilteredStep < totalFilteredSteps) {
    animationTimeout = setTimeout(
      () => {
        doStepAnimation(); // Execute the current step's animation FIRST

        currentFilteredStep++; // Move to the next step index

        if (currentFilteredStep >= totalFilteredSteps) {
          // We have just finished the last step
          console.log("scheduleNextStep: Reached end of animation.");
          if (!isPaused) pauseAnimation(); // Auto-pause at the end only if it was playing
          updateDisplay(); // Ensure final display text is correct
          updateControlStates();
        } else {
          scheduleNextStep(); // Schedule the one after that
        }
      },
      // Shorter delay only before first *animation* step (i.e., step 1, triggered when currentFilteredStep is 0)
      currentFilteredStep === 0 ? 50 : animDuration + pauseDuration
    );
  } else if (currentFilteredStep >= totalFilteredSteps) {
    // This case handles reaching the end if called manually (e.g., via Next button)
    console.log("scheduleNextStep: Already at or beyond end.");
    if (!isPaused) pauseAnimation(); // Ensure paused if manually stepped to end
    updateDisplay();
    updateControlStates();
  }
}

/**
 * Starts or resumes the animation playback. Handles Reset.
 */
function playAnimation() {
  if (!isPaused) return; // Already playing

  // Handle Reset case: If paused at the end, reset to beginning
  if (currentFilteredStep >= totalFilteredSteps - 1 && totalFilteredSteps > 1) {
    console.log("Resetting animation to step 0");
    currentFilteredStep = 0;
    // Need to manually reset agent positions/styles to step 0 state *immediately*
    doStepAnimation(); // Renders step 0 state
    updateControlStates(); // Update buttons immediately after reset
    // Short delay before starting the animation loop after reset
    isPaused = false; // Set to playing *before* setTimeout
    animationTimeout = setTimeout(() => {
        if (!isPaused) scheduleNextStep(); // Check if still playing before scheduling
    }, 50);
    return; // Don't immediately call scheduleNextStep below
  }

  console.log("playAnimation: Starting/Resuming.");
  isPaused = false;
  updateControlStates();
  // Start the animation loop by scheduling the *next* step
  scheduleNextStep();
}

/**
 * Pauses the animation playback.
 */
function pauseAnimation() {
  if (isPaused) return; // Already paused
  console.log("pauseAnimation: Pausing.");
  isPaused = true;
  if (animationTimeout) clearTimeout(animationTimeout);
  animationTimeout = null;
  // Stop ongoing animations smoothly without jumping to the end
  cy?.elements(".agent").stop(false, false);
  updateControlStates();
}

/**
 * Advances the animation to the next step manually.
 */
function nextStep() {
  if (!isPaused) {
    pauseAnimation(); // Pause first if playing
  }
  // Check if there is a next step to move to
  if (currentFilteredStep < totalFilteredSteps - 1) {
    console.log("nextStep: Moving to next step.");
    currentFilteredStep++;
    doStepAnimation(); // Execute the animation for the new step
    updateControlStates();
  } else {
    console.log("nextStep: Already at the last step.");
    // Ensure display shows the final state correctly if called on last step
    // Check if currentFilteredStep is exactly the last valid index
    if (currentFilteredStep === totalFilteredSteps - 1) {
        doStepAnimation(); // Ensure final step visuals are applied
    }
    updateDisplay();
    updateControlStates(); // Update buttons to show end state
  }
}


/**
 * Add agent elements to Cytoscape based on initial state data.
 * Does not control visibility here.
 */
function addAgents(posRound, statRound, leaderRound, levelRound) {
  if (!cy) {
    console.error("addAgents: Cytoscape instance not available.");
    return;
  }
  if (!posRound || posRound.length === 0) {
      console.warn("addAgents: No agent positions provided for initial state. Skipping agent creation.");
      cy.elements(".agent").remove(); // Ensure no old agents remain
      return;
  }

  console.log(`addAgents: Adding/replacing ${posRound.length} agent elements.`);
  cy.elements(".agent").remove(); // Clear existing agents first

  posRound.forEach((nodeId, i) => {
    const nodeElement = cy.getElementById(String(nodeId));
    const pos = nodeElement?.position();

    if (!nodeElement || !pos) {
      console.warn(
        `addAgents: Node element ${nodeId} or position not found for agent A${i}. Skipping.`
      );
      return;
    }
    // Safer access to leader/level arrays
    const leaderId = (leaderRound && i < leaderRound.length) ? leaderRound[i] : undefined;
    const level = (levelRound && i < levelRound.length) ? levelRound[i] : undefined;
    const clr = computeAgentColor(leaderId, level);

    const agentData = {
      id: `a${i}`,
      label: `A${i}`,
      agentColor: clr,
      leader: leaderId,
      level: level,
    };

    cy.add({
      data: agentData,
      position: { x: pos.x, y: pos.y }, // Initial position
      classes: "agent",
      grabbable: false,
    });
    // Status classes (borders) and visibility are applied during animation steps (doStepAnimation)
  });
}

/**
 * Main function called by main.js to draw the graph and set up the animation.
 */
export function drawCytoscape(containerId, originalData) {
  console.log("drawCytoscape: Initializing visualization.");

  // --- Cleanup previous instance ---
  if (cy) cy.destroy();
  cy = null;
  if (animationTimeout) clearTimeout(animationTimeout);
  animationTimeout = null;

  // --- Reset State ---
  isPaused = true;
  currentFilteredStep = 0;
  totalFilteredSteps = 0;
  filteredPositions = [];
  filteredStatuses = [];
  filteredLeaders = [];
  filteredLevels = [];
  filteredNodeStates = []; // <--- ADDED: Reset node states
  originalNodes = originalData.nodes || [];
  originalEdges = originalData.edges || [];

  // --- Get UI Elements ---
  const container = document.getElementById(containerId);
  roundDisplay = document.getElementById("round-display");
  playPauseBtn = document.getElementById("playPauseBtn");
  nextStepBtn = document.getElementById("nextStepBtn");
  showAgentsCheck = document.getElementById("showAgentsCheck");
  nodeTooltip = document.getElementById("node-tooltip");

  if (
    !container ||
    !roundDisplay ||
    !playPauseBtn ||
    !nextStepBtn ||
    !showAgentsCheck ||
    !nodeTooltip
  ) {
    console.error("drawCytoscape: Missing required HTML elements. Aborting.");
    if (roundDisplay) roundDisplay.textContent = "Error: UI elements missing.";
    return;
  }

  // --- Read Config and Filter Data ---
  const flags = {
    showScout: document.getElementById("showScoutCheck")?.checked ?? true,
    showChase: document.getElementById("showChaseCheck")?.checked ?? true,
    showFollow: document.getElementById("showFollowCheck")?.checked ?? true,
  };
  // Make sure originalData.node_settled_states exists, default to empty array if not
  originalData.node_settled_states = originalData.node_settled_states || [];

  const filteredData = filterSimulationDataForSteps(originalData, flags);
  filteredPositions = filteredData.positions;
  filteredStatuses = filteredData.statuses;
  filteredLeaders = filteredData.leaders;
  filteredLevels = filteredData.levels;
  filteredNodeStates = filteredData.nodeStates; // <--- ADDED: Store filtered node states
  totalFilteredSteps = filteredPositions.length;

  const animInput = document.getElementById("animationDurationInput");
  animDuration = Math.max(50, parseInt(animInput?.value, 10) || 300);
  pauseDuration = animDuration * 0.5;

  // --- Setup Cytoscape Instance ---
  const isDark = document.documentElement.classList.contains("dark");
  cy = cytoscape({
    container,
    elements: originalNodes.concat(originalEdges),
    style: [
      // Nodes
      {
        selector: "node",
        style: {
          label: "data(id)",
          "background-color": isDark ? "#3b82f6" : "#93c5fd",
          width: 30,
          height: 30,
          "text-valign": "center",
          "text-halign": "center",
          "font-size": 12,
          color: isDark ? "#f1f5f9" : "#1e293b",
          "font-weight": 600,
          "border-width": 0,
        },
      },
      {
        selector: "node.has-unsettled",
        style: { "background-color": isDark ? "#facc15" : "#fbbf24" },
      }, // Highlight nodes with unsettled agents
      // Edges
      {
        selector: "edge",
        style: {
          width: 2,
          "line-color": isDark ? "#64748b" : "#cbd5e1",
          "target-arrow-shape": "none",
          "source-arrow-shape": "none",
          "source-label": "data(srcPort)",
          "target-label": "data(dstPort)",
          "source-text-offset": 25,
          "target-text-offset": 25,
          "font-size": 9,
          "text-background-color": isDark ? "#1e293b" : "#ffffff",
          "text-background-opacity": 0.8,
          "text-background-padding": "2px",
          "text-background-shape": "roundrectangle",
          color: isDark ? "#cbd5e1" : "#475569",
        },
      },
      // Agents (general appearance, including opacity for visibility)
      {
        selector: ".agent",
        style: {
          shape: "ellipse",
          "background-color": (ele) =>
            ele.data("agentColor") || (isDark ? "#e2e8f0" : "#475569"),
          width: 24,
          height: 24,
          label: "data(label)",
          "font-size": 10,
          "text-valign": "center",
          "text-halign": "center",
          color: "#ffffff",
          "font-weight": "bold",
          "text-outline-width": 1,
          "text-outline-color": (ele) =>
            ele.data("agentColor")
              ? isDark
                ? "#0f172a"
                : "#1e293b"
              : isDark
              ? "#1e293b"
              : "#475569",
          "z-index": 10,
          "border-width": 0,
          "border-style": "solid",
          "border-color": "#000",
          opacity: 1,
          "events": "no", // Agents generally don't need pointer events
          "transition-property":
            "position background-color border-color opacity",
          "transition-duration": `${animDuration}ms`,
          "transition-timing-function": "ease-in-out",
        },
      },
      // Settled agent style
      {
        selector: ".agent.settled",
        style: {
          "border-width": 3,
          "border-color": (ele) =>
            ele.data("agentColor") ? `color-mix(in srgb, ${ele.data("agentColor")} 80%, ${isDark ? '#10b981' : '#059669'})` : (isDark ? "#10b981" : "#059669"),
        },
      },
      // Settled_Wait agent style
      {
        selector: ".agent.settled_wait",
        style: {
          "border-width": 3,
          "border-style": "dashed",
          "border-color": (ele) =>
            ele.data("agentColor") ? `color-mix(in srgb, ${ele.data("agentColor")} 80%, ${isDark ? '#f59e0b' : '#d97706'})` : (isDark ? "#f59e0b" : "#d97706"),

        },
      },
    ],
    layout: {
      name: "preset",
      positions: (node) => {
          // Find the node data from originalNodes which includes position info
          const nodeData = originalNodes.find((n) => n.data.id === node.id());
          return nodeData?.position || { x: Math.random()*300, y: Math.random()*300 }; // Default random if not found
        },
      zoom: 1,
      pan: { x: 0, y: 0 },
      fit: true, // Fit the graph initially
      padding: 50 // Padding around the graph
    },
      // Interaction options
      zoom: 1,
      minZoom: 0.2,
      maxZoom: 3,
      zoomingEnabled: true,
      userZoomingEnabled: true,
      panningEnabled: true,
      userPanningEnabled: true,
      boxSelectionEnabled: false, // Usually not needed for this viz
      autoungrabify: true, // Prevent users from dragging nodes/agents
  });

  // --- Post-Initialization Setup ---
  cy.ready(() => {
    console.log("drawCytoscape: Cytoscape ready.");
    cy.resize();            // refresh viewport size
    cy.fit(undefined, 50);
    // Add Agent elements & Display Initial State (Step 0)
    if (totalFilteredSteps > 0) {
      addAgents(
        filteredPositions[0]?.[1], // Safer access
        filteredStatuses[0]?.[1] || [],
        filteredLeaders[0]?.[1] || [],
        filteredLevels[0]?.[1] || []
      );
      // Apply initial state visuals (step 0) without scheduling next step
      // Set currentFilteredStep to 0 before calling doStepAnimation for initial state
      currentFilteredStep = 0;
      doStepAnimation();
    } else {
      console.log("drawCytoscape: No steps to display after filtering.");
      cy.elements(".agent").remove(); // Remove if no steps at all
    }

    updateAgentVisibility(); // Set initial opacity based on checkbox
    updateDisplay(); // Set initial display text
    updateControlStates(); // Set initial button states
    // cy.fit(undefined, 50); // Fit viewport - might already be handled by layout.fit

    // --- Attach Event Listeners ---
    // Clear old listeners if any (safer if drawCytoscape is called multiple times)
    playPauseBtn.onclick = null;
    nextStepBtn.onclick = null;
    showAgentsCheck.onchange = null;
    cy.removeListener("mouseover");
    cy.removeListener("mouseout");
    cy.removeListener("pan zoom drag");

    // Attach new listeners
    playPauseBtn.onclick = () => {
      if (isPaused) playAnimation();
      else pauseAnimation();
    };
    nextStepBtn.onclick = nextStep;
    showAgentsCheck.onchange = updateAgentVisibility;

    // Tooltip Event Listeners
    const graphNodeSelector = "node:not(.agent)"; // Exclude agent nodes from tooltip
    cy.on("mouseover", graphNodeSelector, (e) => updateTooltip(e.target, e));
    cy.on("mouseout", graphNodeSelector, hideTooltip);
    cy.on("pan zoom drag", hideTooltip); // Hide on view change

    console.log(
      "drawCytoscape: Initialization complete. Ready for interaction."
    );
  }); // cy.ready end
} // drawCytoscape end