let cy = null;
let animationTimeout = null;
const DEBUG = false;

let isPaused = true;
let currentFilteredStep = 0;
let totalFilteredSteps = 0;
let animDuration = 300;
let pauseDuration = 150;

let filteredPositions = [];
let filteredStatuses = [];
let filteredLeaders = [];
let filteredLevels = [];
let filteredNodeStates = [];
let originalNodes = [];
let originalEdges = [];

let roundDisplay = null;
let playPauseBtn = null;
let nextStepBtn = null;
let prevStepBtn = null;
let showAgentsCheck = null;
let nodeTooltip = null;

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

function filterSimulationDataForSteps(originalData, flags) {
  const { showScout, showChase, showFollow } = flags;
  const positions = [];
  const statuses = [];
  const leaders = [];
  const levels = [];
  const nodeStates = [];

  if (!originalData.positions || originalData.positions.length === 0) {
    return { positions, statuses, leaders, levels, nodeStates };
  }

  positions.push(originalData.positions[0]);
  if (originalData.statuses?.[0]) statuses.push(originalData.statuses[0]);
  if (originalData.leaders?.[0]) leaders.push(originalData.leaders[0]);
  if (originalData.levels?.[0]) levels.push(originalData.levels[0]);
  if (originalData.node_settled_states?.[0]) nodeStates.push(originalData.node_settled_states[0]);

  for (let i = 1; i < originalData.positions.length; i++) {
    const label = originalData.positions[i][0].toLowerCase();
    let keepStep = true;

    if (label.includes("scout") && !showScout) keepStep = false;
    else if (label.includes("chase") && !showChase) keepStep = false;
    else if (label.includes("follow") && !showFollow) keepStep = false;

    if (keepStep) {
      positions.push(originalData.positions[i]);
      if (originalData.statuses?.[i]) statuses.push(originalData.statuses[i]);
      if (originalData.leaders?.[i]) leaders.push(originalData.leaders[i]);
      if (originalData.levels?.[i]) levels.push(originalData.levels[i]);
      if (originalData.node_settled_states?.[i]) nodeStates.push(originalData.node_settled_states[i]);
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
  return { positions, statuses, leaders, levels, nodeStates };
}


function updateAgentVisibility() {
  if (!cy) return;
  const show = showAgentsCheck?.checked ?? true;
  cy.elements(".agent").style({
    opacity: show ? 1 : 0,
  });
  const safeStepIndex = Math.max(0, Math.min(currentFilteredStep, totalFilteredSteps - 1));
  if (filteredPositions.length > safeStepIndex) {
    updateNodeStyles(
      filteredPositions[safeStepIndex][1],
      filteredStatuses[safeStepIndex]?.[1] || []
    );
  }
}

function updateNodeStyles(posRound, statRound) {
  if (!cy) return;

  const agentsVisible = showAgentsCheck?.checked ?? true;
  cy.nodes('.graph-node').removeClass("has-unsettled");

  if (agentsVisible && posRound && statRound) {
    statRound.forEach((status, i) => {
      if (status === 1 && i < posRound.length) {
        const nodeId = String(posRound[i]);
        const node = cy.getElementById(nodeId).filter('.graph-node');
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

function updateControlStates() {
  if (!playPauseBtn || !nextStepBtn || !prevStepBtn) return;

  const isAtOrPastEnd = currentFilteredStep >= totalFilteredSteps - 1;
  const isAtStart = currentFilteredStep <= 0;

  if (isPaused) {
    playPauseBtn.textContent =
      isAtOrPastEnd && totalFilteredSteps > 1 ? "🔄 Reset" : "▶️ Play";
    playPauseBtn.disabled = totalFilteredSteps <= 1;
  } else {
    playPauseBtn.textContent = "⏸️ Pause";
    playPauseBtn.disabled = false;
  }
  nextStepBtn.disabled = isAtOrPastEnd || !isPaused;
  prevStepBtn.disabled = isAtStart || !isPaused;
}

function updateDisplay() {
  if (!roundDisplay) return;
  if (totalFilteredSteps === 0) {
    roundDisplay.textContent = "No simulation data.";
    return;
  }
  if (totalFilteredSteps === 1) {
    const label = filteredPositions[0]?.[0] || "Initial State";
    roundDisplay.textContent = `Initial State: ${label} (No steps to animate)`;
    return;
  }

  const currentLabel = filteredPositions[currentFilteredStep]?.[0] ?? "End";
  const stepNum = Math.max(
    0,
    Math.min(currentFilteredStep, totalFilteredSteps - 1)
  );
  roundDisplay.textContent = `Step: ${stepNum} / ${
    totalFilteredSteps - 1
  } (${currentLabel})`;

  if (currentFilteredStep >= totalFilteredSteps && totalFilteredSteps > 0) {
    const finalLabel = filteredPositions[totalFilteredSteps - 1]?.[0] || "Final";
    roundDisplay.textContent = `Done: ${finalLabel} (Step ${
      totalFilteredSteps - 1
    }/${totalFilteredSteps - 1})`;
  }
}

function updateTooltip(node, event) {
    if (!nodeTooltip || !node || !node.length || !node.hasClass("graph-node")) {
        hideTooltip();
        return;
    }

    const nodeId = node.id();
    const stepIndex = Math.max(
        0,
        Math.min(currentFilteredStep, totalFilteredSteps - 1)
    );

    let content = `<strong>Node ${nodeId}</strong><br>`;
    let foundSettled = false;

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

        const nodeStatesDict = filteredNodeStates[stepIndex]?.[1] || {};
        const settledState = nodeStatesDict[nodeId];

        for (let i = 0; i < positionsAtStep.length; i++) {
            if (String(positionsAtStep[i]) === nodeId) {
                const status = statusesAtStep[i];
                if (status === 0 || status === 2) {
                    const leaderId = leadersAtStep[i];
                    const level = levelsAtStep[i];
                    const statusText = status === 0 ? "SETTLED" : "SETTLED_WAIT";

                    const parentPort = settledState?.parent_port ?? '(N/A)';
                    const checkedPort = settledState?.checked_port ?? '(N/A)';
                    const maxScoutedPort = settledState?.max_scouted_port ?? '(N/A)';
                    const nextPort = settledState?.next_port ?? '(N/A)';

                    content += `Settled Agent: A${i}<br>`;
                    content += `  Status: ${statusText}<br>`;
                    content += `  Leader: A${leaderId}<br>`;
                    content += `  Level: ${level}<br>`;
                    content += `  Parent Port: ${parentPort}<br>`;
                    content += `  Checked Port: ${checkedPort}<br>`;
                    content += `  Max Scouted: ${maxScoutedPort}<br>`;
                    content += `  Next Port: ${nextPort}`;

                    foundSettled = true;
                    break;
                }
            }
        }
    }

    if (!foundSettled) {
        content += "No settled agent at this step.";
    }

    nodeTooltip.innerHTML = content;

    const padding = 15;
    let x = event.originalEvent.pageX + padding;
    let y = event.originalEvent.pageY + padding;

    const tooltipRect = nodeTooltip.getBoundingClientRect();
    if (x + tooltipRect.width > window.innerWidth) {
        x = event.originalEvent.pageX - tooltipRect.width - padding;
    }
    if (y + tooltipRect.height > window.innerHeight) {
        y = event.originalEvent.pageY - tooltipRect.height - padding;
    }
    if (x < 0) x = padding;
    if (y < 0) y = padding;

    nodeTooltip.style.left = `${x}px`;
    nodeTooltip.style.top = `${y}px`;
    nodeTooltip.classList.remove("hidden");
}


function hideTooltip() {
  if (nodeTooltip) {
    nodeTooltip.classList.add("hidden");
  }
}

function doStepAnimation() {
  if (!cy || currentFilteredStep >= totalFilteredSteps) {
    console.log(
      `doStepAnimation: Reached end or invalid state (Step: ${currentFilteredStep}, Total: ${totalFilteredSteps}). Stopping.`
    );
    if (!isPaused && totalFilteredSteps > 0) {
        pauseAnimation();
    }
    updateDisplay();
    updateControlStates();
    return;
  }

  console.log(`doStepAnimation: Executing step ${currentFilteredStep}`);

  const stepData = filteredPositions[currentFilteredStep];
  const currentLabel = stepData[0];
  const currentPositions = stepData[1];
  const currentStatuses = filteredStatuses?.[currentFilteredStep]?.[1] || [];
  const currentLeaders = filteredLeaders?.[currentFilteredStep]?.[1] || [];
  const currentLevels = filteredLevels?.[currentFilteredStep]?.[1] || [];

  updateDisplay();
  updateNodeStyles(currentPositions, currentStatuses);

  currentPositions.forEach((nodeId, i) => {
    const agentId = `a${i}`;
    const agent = cy.getElementById(agentId);
    if (!agent || agent.length === 0) {
      if (DEBUG)
        console.warn(
          `doStepAnimation: Agent element ${agentId} not found at step ${currentFilteredStep}.`
        );
      return;
    }

    const leaderId = i < currentLeaders.length ? currentLeaders[i] : undefined;
    const level = i < currentLevels.length ? currentLevels[i] : undefined;
    const clr = computeAgentColor(leaderId, level);
    agent.data({ agentColor: clr, leader: leaderId, level: level });

    const status = i < currentStatuses.length ? currentStatuses[i] : undefined;
    agent.removeClass("settled settled_wait");
    if (status === 0) agent.addClass("settled");
    else if (status === 2) agent.addClass("settled_wait");

    const targetNodeId = String(nodeId);
    const targetNode = cy.getElementById(targetNodeId).filter('.graph-node');
    const targetPosition = targetNode?.position();

    if (targetNode && targetPosition) {
      agent.stop(true, false);
      agent.animate(
        { position: { x: targetPosition.x, y: targetPosition.y } },
        { duration: animDuration }
      );
    } else {
      if (DEBUG)
        console.warn(
          `doStepAnimation: Target node ${targetNodeId} or position not found for agent ${agentId}. Agent won't move.`
        );
    }
  });
  updateAgentVisibility();
}

function scheduleNextStep() {
  if (animationTimeout) clearTimeout(animationTimeout);
  animationTimeout = null;

  if (!isPaused && currentFilteredStep < totalFilteredSteps) {
    animationTimeout = setTimeout(
      () => {
        doStepAnimation();

        currentFilteredStep++;

        if (currentFilteredStep >= totalFilteredSteps) {
          console.log("scheduleNextStep: Reached end of animation.");
          if (!isPaused) pauseAnimation();
          updateDisplay();
          updateControlStates();
        } else {
          scheduleNextStep();
        }
      },
      currentFilteredStep === 0 ? 50 : animDuration + pauseDuration
    );
  } else if (currentFilteredStep >= totalFilteredSteps) {
    console.log("scheduleNextStep: Already at or beyond end.");
    if (!isPaused) pauseAnimation();
    updateDisplay();
    updateControlStates();
  }
}

function playAnimation() {
  if (!isPaused) return;

  if (currentFilteredStep >= totalFilteredSteps - 1 && totalFilteredSteps > 1) {
    console.log("Resetting animation to step 0");
    currentFilteredStep = 0;
    doStepAnimation();
    updateControlStates();
    isPaused = false;
    animationTimeout = setTimeout(() => {
        if (!isPaused) scheduleNextStep();
    }, 50);
    return;
  }

  console.log("playAnimation: Starting/Resuming.");
  isPaused = false;
  updateControlStates();
  scheduleNextStep();
}

function pauseAnimation() {
  if (isPaused) return;
  console.log("pauseAnimation: Pausing.");
  isPaused = true;
  if (animationTimeout) clearTimeout(animationTimeout);
  animationTimeout = null;
  cy?.elements(".agent").stop(false, false);
  updateControlStates();
}

function nextStep() {
  if (!isPaused) {
    pauseAnimation();
  }
  if (currentFilteredStep < totalFilteredSteps - 1) {
    console.log("nextStep: Moving to next step.");
    currentFilteredStep++;
    doStepAnimation();
    updateControlStates();
  } else {
    console.log("nextStep: Already at the last step.");
    if (currentFilteredStep === totalFilteredSteps - 1) {
        doStepAnimation();
    }
    updateDisplay();
    updateControlStates();
  }
}

function previousStep() {
  if (!isPaused) {
    pauseAnimation();
  }
  if (currentFilteredStep > 0) {
    console.log("previousStep: Moving to previous step.");
    currentFilteredStep--;
    doStepAnimation();
    updateControlStates();
  } else {
    console.log("previousStep: Already at the first step.");
    updateControlStates();
  }
}

function addAgents(posRound, statRound, leaderRound, levelRound) {
  if (!cy) {
    console.error("addAgents: Cytoscape instance not available.");
    return;
  }
  if (!posRound || posRound.length === 0) {
      console.warn("addAgents: No agent positions provided for initial state. Skipping agent creation.");
      cy.elements(".agent").remove();
      return;
  }

  console.log(`addAgents: Adding/replacing ${posRound.length} agent elements.`);
  cy.elements(".agent").remove();

  posRound.forEach((nodeId, i) => {
    const nodeElement = cy.getElementById(String(nodeId)).filter('.graph-node');
    const pos = nodeElement?.position();

    if (!nodeElement || !pos) {
      console.warn(
        `addAgents: Node element ${nodeId} or position not found for agent A${i}. Skipping.`
      );
      return;
    }
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
      position: { x: pos.x, y: pos.y },
      classes: "agent",
      grabbable: false,
    });
  });
}

export function drawCytoscape(containerId, originalData) {
  console.log("drawCytoscape: Initializing visualization.");

  if (cy) cy.destroy();
  cy = null;
  if (animationTimeout) clearTimeout(animationTimeout);
  animationTimeout = null;

  isPaused = true;
  currentFilteredStep = 0;
  totalFilteredSteps = 0;
  filteredPositions = [];
  filteredStatuses = [];
  filteredLeaders = [];
  filteredLevels = [];
  filteredNodeStates = [];
  originalNodes = originalData.nodes || [];
  originalEdges = originalData.edges || [];

  const container = document.getElementById(containerId);
  roundDisplay = document.getElementById("round-display");
  playPauseBtn = document.getElementById("playPauseBtn");
  nextStepBtn = document.getElementById("nextStepBtn");
  prevStepBtn = document.getElementById("prevStepBtn");
  showAgentsCheck = document.getElementById("showAgentsCheck");
  nodeTooltip = document.getElementById("node-tooltip");

  if (
    !container ||
    !roundDisplay ||
    !prevStepBtn ||
    !playPauseBtn ||
    !nextStepBtn ||
    !showAgentsCheck ||
    !nodeTooltip
  ) {
    console.error("drawCytoscape: Missing required HTML elements. Aborting.");
    if (roundDisplay) roundDisplay.textContent = "Error: UI elements missing.";
    return;
  }

  const flags = {
    showScout: document.getElementById("showScoutCheck")?.checked ?? true,
    showChase: document.getElementById("showChaseCheck")?.checked ?? true,
    showFollow: document.getElementById("showFollowCheck")?.checked ?? true,
  };
  originalData.node_settled_states = originalData.node_settled_states || [];

  const filteredData = filterSimulationDataForSteps(originalData, flags);
  filteredPositions = filteredData.positions;
  filteredStatuses = filteredData.statuses;
  filteredLeaders = filteredData.leaders;
  filteredLevels = filteredData.levels;
  filteredNodeStates = filteredData.nodeStates;
  totalFilteredSteps = filteredPositions.length;

  const animInput = document.getElementById("animationDurationInput");
  animDuration = Math.max(50, parseInt(animInput?.value, 10) || 300);
  pauseDuration = animDuration * 0.5;

  const isDark = document.documentElement.classList.contains("dark");
  cy = cytoscape({
    container,
    elements: originalNodes.concat(originalEdges),
    style: [
      {
        selector: "node.graph-node",
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
        selector: "node.graph-node.has-unsettled",
        style: { "background-color": isDark ? "#facc15" : "#fbbf24" },
      },
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
          "events": "no",
          "transition-property":
            "position background-color border-color opacity",
          "transition-duration": `${animDuration}ms`,
          "transition-timing-function": "ease-in-out",
        },
      },
      {
        selector: ".agent.settled",
        style: {
          "border-width": 3,
          "border-color": (ele) =>
            ele.data("agentColor") ? `color-mix(in srgb, ${ele.data("agentColor")} 80%, ${isDark ? '#10b981' : '#059669'})` : (isDark ? "#10b981" : "#059669"),
        },
      },
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
          const nodeData = originalNodes.find((n) => n.data.id === node.id());
          return nodeData?.position || { x: Math.random()*300, y: Math.random()*300 };
        },
      zoom: 1,
      pan: { x: 0, y: 0 },
      fit: true,
      padding: 50
    },
      zoom: 1,
      minZoom: 0.2,
      maxZoom: 3,
      zoomingEnabled: true,
      userZoomingEnabled: true,
      panningEnabled: true,
      userPanningEnabled: true,
      boxSelectionEnabled: false,
      autoungrabify: true,
  });

  cy.nodes().addClass("graph-node");

  cy.ready(() => {
    console.log("drawCytoscape: Cytoscape ready.");
    cy.resize();
    cy.fit(undefined, 50);
    if (totalFilteredSteps > 0) {
      addAgents(
        filteredPositions[0]?.[1],
        filteredStatuses[0]?.[1] || [],
        filteredLeaders[0]?.[1] || [],
        filteredLevels[0]?.[1] || []
      );
      currentFilteredStep = 0;
      cy.fit(undefined, 50);
      doStepAnimation();
    } else {
      console.log("drawCytoscape: No steps to display after filtering.");
      cy.elements(".agent").remove();
    }

    updateAgentVisibility();
    updateDisplay();
    updateControlStates();

    playPauseBtn.onclick = null;
    nextStepBtn.onclick = null;
    prevStepBtn.onclick = null;
    showAgentsCheck.onchange = null;
    cy.removeListener("mouseover");
    cy.removeListener("mouseout");
    cy.removeListener("pan zoom drag");

    playPauseBtn.onclick = () => {
      if (isPaused) playAnimation();
      else pauseAnimation();
    };
    nextStepBtn.onclick = nextStep;
    prevStepBtn.onclick = previousStep;
    showAgentsCheck.onchange = updateAgentVisibility;

    const graphNodeSelector = ".graph-node";
    cy.on("mouseover", graphNodeSelector, (e) => updateTooltip(e.target, e));
    cy.on("mouseout", graphNodeSelector, hideTooltip);
    cy.on("pan zoom drag", hideTooltip);

    console.log(
      "drawCytoscape: Initialization complete. Ready for interaction."
    );
  });
}