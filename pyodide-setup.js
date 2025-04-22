// pyodide-setup.js

// Export a promise that resolves with the initialized Pyodide instance
export const pyodideReady = loadPyodide({ indexURL: "https://cdn.jsdelivr.net/pyodide/v0.27.5/full/" })
  .then(async (pyodide) => {
    console.log("Pyodide loaded.");
    // Removed matplotlib as it's not needed for the web visualization part
    await pyodide.loadPackage(["networkx", "micropip"]);
    console.log("Python packages (networkx, micropip) loaded.");
    return pyodide;
  })
  .catch(err => {
    console.error("Pyodide loading failed:", err);
    throw err; // Re-throw error to be caught by caller
  });