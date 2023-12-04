#!/usr/bin/env python3
import glob
import json
import statistics
import sys


def load_jsons(files):
    js = []
    for file in files:
        try:
            with open(file) as f:
                j = json.load(f)
                j["json_path"] = file
                js.append(j)
        except Exception as e:
            print(f"Error loading {file}")
            raise e
    return js


def process(js, extractors):
    by_name = {}
    for j in js:
        n, e = j["name"], j["extractor"]
        by_name.setdefault(n, {})[e] = j

    print("extractors:", extractors)
    assert len(extractors) == 2
    e1, e2 = extractors

    e1_cumulative=0
    e2_cumulative=0

    summaries = {}

    for name, d in by_name.items():
        try:
            if d[e1]["tree"] !=  d[e2]["tree"]:
                print(name, " differs in tree cost: ", d[e1]["tree"], d[e2]["tree"]);
            if d[e1]["dag"] !=  d[e2]["dag"]:
                print(name, " differs in dag cost: ", d[e1]["dag"], d[e2]["dag"]);
                
            tree_ratio = d[e1]["tree"] / d[e2]["tree"]
            dag_ratio = d[e1]["dag"] / d[e2]["dag"]
            micros_ratio = max(1, d[e1]["micros"]) / max(1, d[e2]["micros"])
            
            e1_cumulative += d[e1]["micros"];
            e2_cumulative += d[e2]["micros"];
            
            summaries[name] = {
                "tree": tree_ratio,
                "dag": dag_ratio,
                "micros": micros_ratio,
            }
        except Exception as e:
            print(f"Error processing {name}")
            raise e
 
    print(f"cumulative time for {e1}: {e1_cumulative/1000:.0f}ms")
    print(f"cumulative time for {e2}: {e2_cumulative/1000:.0f}ms")

    print(f"{e1} / {e2}")

    print("geo mean")
    tree_summary = statistics.geometric_mean(s["tree"] for s in summaries.values())
    dag_summary = statistics.geometric_mean(s["dag"] for s in summaries.values())
    micros_summary = statistics.geometric_mean(s["micros"] for s in summaries.values())

    print(f"tree: {tree_summary:.4f}")
    print(f"dag: {dag_summary:.4f}")
    print(f"micros: {micros_summary:.4f}")

    print("quantiles")

    def quantiles(key):
        xs = [s[key] for s in summaries.values()]
        qs = statistics.quantiles(xs, n=4)
        with_extremes = [min(xs)] + qs + [max(xs)]
        return ", ".join(f"{x:.4f}" for x in with_extremes)

    print(f"tree:   {quantiles('tree')}")
    print(f"dag:    {quantiles('dag')}")
    print(f"micros: {quantiles('micros')}")


if __name__ == "__main__":
    print()
    print(" ------------------------ ")
    print(" ------- plotting ------- ")
    print(" ------------------------ ")
    print()
    files = sys.argv[1:] or glob.glob("output/**/*.json", recursive=True)
    js = load_jsons(files)
    print(f"Loaded {len(js)} jsons.")

    extractors = sorted(set(j["extractor"] for j in js))

    for ex1 in extractors:
        for ex2 in extractors:
            if ex1 == ex2:
                continue
            print(f"###################################################\n{ex1} vs {ex2}\n\n")
            process(js, [ex1, ex2])
            print("\n\n")
