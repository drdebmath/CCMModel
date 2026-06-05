#!/usr/bin/env python3
import os, json, argparse, warnings
import pandas as pd, seaborn as sns, matplotlib.pyplot as plt
from concurrent.futures import ProcessPoolExecutor

# Set a visually appealing seaborn theme
sns.set_theme(style="whitegrid", palette="muted")

# Define x and y variables with labels
XY = [
    ("num_nodes", "Nodes"), ("num_edges", "Edges"),
    ("max_degree_actual", "Max Degree"), ("avg_degree", "Avg Degree")
]
YY = [
    ("rounds", "Rounds"), ("max_edge_traversals", "Max Edge Trav."),
    ("edge_traversals", "Total Edge Trav."), ("rounds_per_node", "Rounds/Node"),
    ("total_traversals_per_edge", "Total Trav./Edge")
]

def load(path: str) -> pd.DataFrame:
    if not os.path.exists(path): raise FileNotFoundError(f"{path} not found")
    with open(path) as f: data = json.load(f)
    df = pd.DataFrame(data)
    if df.empty: raise ValueError("No data in file")

    # Convert relevant columns to numeric
    for col in ["num_nodes", "num_edges", "max_degree_actual", "rounds", "edge_traversals", "max_edge_traversals", "target_degree"]:
        if col in df: df[col] = pd.to_numeric(df[col], errors="coerce")

    # Calculate derived metrics
    df["avg_degree"] = 2 * df["num_edges"] / df["num_nodes"]
    df["rounds_per_node"] = df["rounds"] / df["num_nodes"]
    df["total_traversals_per_edge"] = df["edge_traversals"] / df["num_edges"]
    df = df[df["error"].isna()]
    if "timeout" in df: df = df[~df["timeout"]]
    return df

def scatter(df, x, xlab, y, ylab, out, fmt):
    if any(col not in df for col in [x, y, "graph_type"]): return
    h = "target_degree" if "target_degree" in df else None
    # Create scatter plot with faceting
    g = sns.relplot(data=df, x=x, y=y, col="graph_type", hue=h, kind="scatter", col_wrap=3,
                    size="num_nodes" if "num_nodes" in df else None, sizes=(30, 200), alpha=0.7, s=40,
                    height=5, aspect=1.2)  # Adjusted size for better visibility
    g.set(xscale="log", yscale="log")
    g.set_xlabels(xlab)
    g.set_ylabels(ylab)
    g.fig.suptitle(f"{ylab} vs {xlab}", y=1.03)

    # Add custom x-ticks and rotate labels
    unique_x = sorted(df[x].unique())
    for ax in g.axes.flat:
        ax.set_xticks(unique_x)
        ax.set_xticklabels([f"{val:.0f}" for val in unique_x], rotation=45, ha='right')

    plt.tight_layout(rect=[0, 0, 1, 0.97])
    g.savefig(os.path.join(out, f"scatter_{y}_vs_{x}.{fmt}"))
    plt.close()

def box(df, x, xlab, y, ylab, out, fmt):
    if any(col not in df for col in [x, y, "graph_type"]): return
    plt.figure(figsize=(12, 6))  # Increased size for readability
    # Bin x if too many unique values, otherwise use exact values
    if df[x].nunique() > 15:
        x_binned = pd.cut(df[x], bins=10)
    else:
        x_binned = df[x]
    sns.boxplot(data=df, x=x_binned, y=y, hue="graph_type", showfliers=False)
    plt.yscale("log")
    plt.xlabel(xlab)
    plt.ylabel(ylab)
    plt.title(f"{ylab} vs {xlab}")
    plt.legend(title="Graph Type", bbox_to_anchor=(1.02, 1), loc="upper left")

    # Rotate x-tick labels
    plt.xticks(rotation=45, ha='right')

    plt.tight_layout(rect=[0, 0, 0.9, 1])
    plt.savefig(os.path.join(out, f"box_{y}_vs_{x}.{fmt}"))
    plt.close()

def line(df, x, xlab, y, ylab, out, fmt):
    if any(col not in df for col in [x, y, "graph_type"]): return
    style = "target_degree" if "target_degree" in df else None
    plt.figure(figsize=(12, 6))  # Increased size for clarity
    # Line plot aggregates multiple runs by default
    sns.lineplot(data=df, x=x, y=y, hue="graph_type", style=style)
    plt.xscale("log")
    plt.yscale("log")
    plt.title(f"{ylab} vs {xlab}")
    plt.xlabel(xlab)
    plt.ylabel(ylab)
    plt.legend(title="Graph Type" + (" / Degree" if style else ""), bbox_to_anchor=(1.02, 1), loc="upper left")

    # Add custom x-ticks and rotate labels
    unique_x = sorted(df[x].unique())
    plt.xticks(unique_x, [f"{val:.0f}" for val in unique_x], rotation=45, ha='right')

    plt.tight_layout(rect=[0, 0, 0.9, 1])
    plt.savefig(os.path.join(out, f"line_{y}_vs_{x}.{fmt}"))
    plt.close()

def make_all_plots(df, out, fmt, max_workers=4):
    os.makedirs(out, exist_ok=True)
    with ProcessPoolExecutor(max_workers=max_workers) as pool:
        tasks = []
        for x, xlab in XY:
            for y, ylab in YY:
                for fn in [scatter, box, line]:
                    tasks.append(pool.submit(fn, df.copy(), x, xlab, y, ylab, out, fmt))
        for t in tasks: t.result()

def cli():
    p = argparse.ArgumentParser()
    p.add_argument("-i", "--input", default="simulation_results.json")
    p.add_argument("-o", "--out", default="plots_enhanced")
    p.add_argument("--format", default="png")
    p.add_argument("--max-workers", type=int, default=os.cpu_count() or 1)
    return p.parse_args()

if __name__ == "__main__":
    args = cli()
    warnings.simplefilter("ignore")
    df = load(args.input)
    make_all_plots(df, args.out, args.format, args.max_workers)
    print(f"Plots saved in: {args.out}")