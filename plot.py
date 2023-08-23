#!/usr/bin/env python3
import glob
import json
import statistics
import sys
import os

def load_jsons(files):
    js = []
    for file in files:
        try:
            with open(file) as f:
                j = json.load(f)
                if j["dag"] >= 1000.0:
                    # 1000.0 = Inf
                    j["dag"] = 1000.0
                j["json_path"] = file
                j["json_dir"] = os.path.dirname(file)
                js.append(j)
        except Exception as e:
            print(f"Error loading {file}")
            raise e
    return js


def process(js):
    names = set(j["name"] for j in js)
    extractors = set(j["extractor"] for j in js)
    for e in extractors:
        e_names = set(j["name"] for j in js if j["extractor"] == e )
        assert e_names == names

    dirs_extractors = sorted(set((j["json_dir"], j["extractor"]) for j in js))
    dir_extractor_to_data = {}
    for j in js:
        d, n, e = j["json_dir"], j["name"], j["extractor"]
        dir_extractor_to_data.setdefault((d, e), []).append(j)

    for directory, extractor in dirs_extractors:
        d = dir_extractor_to_data[(directory, extractor)]
        print(f"---- {directory} -- {extractor} results:")
        try:
            dag_mean = statistics.mean(s['dag'] for s in d)
            micros_mean = statistics.mean(s['micros'] for s in d)
            
            def quantiles(key):
                xs = [s[key] for s in d]
                qs = statistics.quantiles(xs, n=4)
                with_extremes = [min(xs)] + qs + [max(xs)]
                return ", ".join(f"{x:.2f}" for x in with_extremes)

            print(f"dag         mean: {dag_mean:.4f}")
            print(f"micros      mean: {micros_mean:.4f}")
            print(f"dag    quantiles:    {quantiles('dag')}")
            print(f"micros quantiles: {quantiles('micros')}")
        except Exception as e:
            print(f"Error processing {extractor}")
            raise e

if __name__ == "__main__":
    print()
    print(" ------------------------ ")
    print(" ------- plotting ------- ")
    print(" ------------------------ ")
    print()
    files = sys.argv[1:] or glob.glob("output/**/*.json", recursive=True)
    js = load_jsons(files)
    print(f"Loaded {len(js)} jsons.")
    process(js)
