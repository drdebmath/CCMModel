# CCM Model

This project simulates a multi-agent system with agents navigating and coordinating on a graph. The codebase is primarily written in Python, with supporting scripts in JavaScript and HTML.

## Background

The graph considered is an anonymous port labeled graph. The agents can move from one node to another in synchrnous rounds. 

WIP: [Near-Linear Time Dispersion](https://arxiv.org/html/2310.04376v3) by Sudo et al. 

## Project Structure

- `agent.py`: Defines the `Agent` class and related enums (as dictionaries) for agent roles, phases, statuses, and node statuses. Contains the agent logic and state transitions.
- `simulation_wrapper.py`: Runs the main simulation loop, initializing agents and stepping through rounds.
- `main.py`: (If present) Likely handles overall orchestration or entry point for running the simulation.
- `simulation-runner.js`: JavaScript file for running or visualizing simulations (details depend on implementation).
- `index.html`: Web interface for running or visualizing the simulation (if applicable).

## Key Concepts

- **Agents**: Each agent has a state including `status`, `role`, `phase`, and location. State changes are logged for traceability.
- **Graph**: Agents move and interact on a graph, with nodes and ports representing locations and connections.
- **Simulation Loop**: The simulation proceeds in rounds, with each agent computing its actions and updating its state.

## How to Run

Run the simulation directly in the browser. The project utilizes [Pyodide](https://pyodide.org/en/stable/index.html) to run python compiled into webassembly for the browser. The website is at [drdebmath.github.io/CCMModel](https://drdebmath.github.io/CCMModel)

## Customization

- Modify `agent.py` to change agent behavior, roles, or state transitions.
- Update `simulation_wrapper.py` to change the simulation parameters or loop.
- Use or adapt `index.html` and `simulation-runner.js` for visualization or web-based interaction.

## Logging and Debugging

- Agent state changes are printed to the console for debugging and traceability.
- Use the print statements to follow the simulation's progress and agent decisions.

## License

MIT

---

For questions or contributions, contact @drdebmath.
