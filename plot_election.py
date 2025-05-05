#!/usr/bin/env python3
"""
plot_election.py
================
Visualises leader‑election performance *for a fixed number of nodes* while
changing a structural metric of the underlying graph.

For every distinct N in the data set we create:

    ┌───────────────┐
    │  Scatter plot │  rounds  vs.   {edges, avg_deg, max_deg}
    └───────────────┘
    ┌───────────────┐
    │   Box  plot   │  rounds distribution for each X‑value (port randomisations)
    └───────────────┘

Colour   →  graph_type  
Marker   →  target_degree

Usage
-----
    python plot_election.py -i simulation_results.json -o plots_fixedN
"""

import argparse, json, os, warnings
from typing import Tuple, List

import pandas as pd
import seaborn as sns
import matplotlib.pyplot as plt

# ── Plot appearance ─────────────────────────────────────────────────────────
sns.set_theme(style="whitegrid")
PLOT_FMT  = "png"
SCATTER_KWS = dict(alpha=0.8, s=60)

# ── Data Loading ────────────────────────────────────────────────────────────
def load_results(path: str) -> pd.DataFrame:
    with open(path, "r") as f:
        raw = json.load(f)
    df = pd.DataFrame(raw)
    if df.empty:
        raise ValueError("input file contains no records")

    numeric_cols = ["num_nodes", "num_edges", "max_degree_actual",
                    "rounds", "edge_traversals", "max_edge_traversals"]
    for c in numeric_cols:
        if c in df.columns:
            df[c] = pd.to_numeric(df[c], errors="coerce")

    # add average degree if not present
    if "avg_degree" not in df.columns:
        df["avg_degree"] = 2 * df["num_edges"] / df["num_nodes"]

    # flag failed / timed‑out runs
    ok = (df["error"].isna()) & (df["timeout"] == False)
    df_ok = df[ok].copy()
    if df_ok.empty:
        raise ValueError("no successful runs in data set")

    return df_ok


# ── Plot helpers ────────────────────────────────────────────────────────────
def _ensure_dir(d: str):
    os.makedirs(d, exist_ok=True)

def _scatter_fixed_N(dfN: pd.DataFrame,
                     x: str, x_label: str,
                     out_dir: str, N: int):
    """Average rounds (per graph config) vs chosen X feature, for one N."""
    grp_cols = ["graph_type", "target_degree", x]
    df_avg = dfN.groupby(grp_cols)["rounds"].mean().reset_index()

    plt.figure(figsize=(8,6))
    sns.scatterplot(
        data=df_avg, x=x, y="rounds",
        hue="graph_type", style="target_degree",
        **SCATTER_KWS
    )
    plt.title(f"Average rounds vs {x_label}  (N = {N})")
    plt.xlabel(x_label)
    plt.ylabel("Average rounds")
    plt.legend(title="Graph type / Δ", bbox_to_anchor=(1.02,1), loc="upper left")
    plt.tight_layout()
    fname = os.path.join(out_dir, f"N{N}_scatter_rounds_vs_{x}.{PLOT_FMT}")
    plt.savefig(fname, dpi=300)
    plt.close()
    print("  saved", fname)


def _box_fixed_N(dfN: pd.DataFrame,
                 x: str, x_label: str,
                 out_dir: str, N: int):
    """Rounds distribution vs X feature (each data point = one run)."""
    plt.figure(figsize=(9,6))
    sns.boxplot(
        data=dfN, x=x, y="rounds",
        hue="graph_type", showfliers=False
    )
    plt.title(f"Rounds distribution vs {x_label}  (N = {N})")
    plt.xlabel(x_label)
    plt.ylabel("Rounds")
    plt.legend(title="Graph type", bbox_to_anchor=(1.02,1), loc="upper left")
    plt.tight_layout()
    fname = os.path.join(out_dir, f"N{N}_box_rounds_vs_{x}.{PLOT_FMT}")
    plt.savefig(fname, dpi=300)
    plt.close()
    print("  saved", fname)


# ── NEW PLOTS ──────────────────────────────────────────────────────────────
def plot_heatmap_rounds(df: pd.DataFrame, out_dir: str):
    """One heat‑map per graph type: avg rounds for each (N, Δ_target) cell."""
    import numpy as np
    for gtype, grp in df.groupby("graph_type"):
        pivot = grp.pivot_table(index="num_nodes",
                                columns="target_degree",
                                values="rounds", aggfunc="mean")
        if pivot.empty: continue
        plt.figure(figsize=(6,4))
        sns.heatmap(pivot, annot=True, fmt=".0f", cmap="viridis", linewidths=.5)
        plt.title(f"Avg rounds – {gtype}")
        plt.ylabel("Nodes (N)")
        plt.xlabel("Target degree (d)")
        plt.tight_layout()
        fn = os.path.join(out_dir, f"heatmap_rounds_{gtype}.{PLOT_FMT}")
        plt.savefig(fn, dpi=300)
        plt.close()
        print("  saved", fn)


def plot_ecdf_rounds(df: pd.DataFrame, out_dir: str):
    """ECDF of rounds for every graph type (all runs)."""
    plt.figure(figsize=(7,5))
    sns.ecdfplot(data=df, x="rounds", hue="graph_type")
    plt.title("ECDF of convergence rounds")
    plt.xlabel("Rounds")
    plt.ylabel("Fraction of runs")
    plt.legend(title="Graph type")
    plt.tight_layout()
    fn = os.path.join(out_dir, f"ecdf_rounds_by_type.{PLOT_FMT}")
    plt.savefig(fn, dpi=300)
    plt.close()
    print("  saved", fn)


def plot_scatter_rounds_vs_total_trav(df: pd.DataFrame, out_dir: str):
    """Rounds vs total edge traversals (one dot per run)."""
    if "edge_traversals" not in df.columns: return
    plt.figure(figsize=(7,5))
    sns.scatterplot(data=df, x="edge_traversals", y="rounds",
                    hue="graph_type", alpha=0.7, s=45)
    plt.title("Rounds vs total edge traversals")
    plt.xlabel("Total edge traversals (run)")
    plt.ylabel("Rounds")
    plt.legend(title="Graph type", bbox_to_anchor=(1.02,1), loc="upper left")
    plt.tight_layout()
    fn = os.path.join(out_dir, f"scatter_rounds_vs_totalTrav.{PLOT_FMT}")
    plt.savefig(fn, dpi=300)
    plt.close()
    print("  saved", fn)


def plot_scatter_rounds_vs_max_trav(df: pd.DataFrame, out_dir: str):
    """Rounds vs max edge traversals of any single agent (one dot per run)."""
    if "max_edge_traversals" not in df.columns: return
    plt.figure(figsize=(7,5))
    sns.scatterplot(data=df, x="max_edge_traversals", y="rounds",
                    hue="graph_type", alpha=0.7, s=45)
    plt.title("Rounds vs max per‑agent edge traversals")
    plt.xlabel("Max edge traversals (run)")
    plt.ylabel("Rounds")
    plt.legend(title="Graph type", bbox_to_anchor=(1.02,1), loc="upper left")
    plt.tight_layout()
    fn = os.path.join(out_dir, f"scatter_rounds_vs_maxTrav.{PLOT_FMT}")
    plt.savefig(fn, dpi=300)
    plt.close()
    print("  saved", fn)


# ── Update make_plots ──────────────────────────────────────────────────────
def make_plots(df: pd.DataFrame, out_dir: str):
    _ensure_dir(out_dir)

    # existing per‑N plots
    features = [("num_edges","Number of edges"),
                ("avg_degree","Average degree"),
                ("max_degree_actual","Maximum degree")]
    for N in sorted(df["num_nodes"].dropna().unique()):
        dfN = df[df["num_nodes"] == N]
        for col, label in features:
            if col not in dfN.columns: continue
            _scatter_fixed_N(dfN, col, label, out_dir, N)
            _box_fixed_N(dfN, col, label, out_dir, N)

    # ── call the new global plots ──────────────────────────────────────────
    plot_heatmap_rounds(df, out_dir)
    plot_ecdf_rounds(df, out_dir)
    plot_scatter_rounds_vs_total_trav(df, out_dir)
    plot_scatter_rounds_vs_max_trav(df, out_dir)

# ── CLI ─────────────────────────────────────────────────────────────────────
def main():
    ap = argparse.ArgumentParser(description="Plots with fixed node count.")
    ap.add_argument("-i","--input", default="simulation_results.json",
                    help="JSON file produced by simulation.py")
    ap.add_argument("-o","--out",   default="plots_fixedN",
                    help="directory for figures")
    args = ap.parse_args()

    df = load_results(args.input)
    make_plots(df, args.out)
    print("All plots written to", args.out)

if __name__ == "__main__":
    main()
