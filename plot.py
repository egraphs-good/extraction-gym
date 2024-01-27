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

def getXYSequence(js, e0, reference, ref_data):

    e0_list = []
    e0_data = [j for j in js if j["extractor"] == e0]

    for j in e0_data:
        jv = j;
        jv["improvement"] = ref_data[j["name"]]["dag"]- j["dag"] 
        e0_list.append(jv)

    e0_value = []
    for jv in e0_list:
        e0_value.append([jv["micros"]/1000000, jv["improvement"]]);

    return e0_value;

# Returns a sequence of [time, improvement] pairs for the given extractor.
# The improvement is the total improvement in dag-cost compared to the 
# reference extractor.
def getSequence(js, e0, reference, ref_data):

    e0_cumulative=0
    e0_list = []

    e0_data = [j for j in js if j["extractor"] == e0]

    for j in e0_data:
        e0_cumulative += j["micros"]
        jv = j;

        reference = ref_data[j["name"]]["dag"]
        jv["improvement"] =  (100* (reference - j["dag"]) / reference) / len(e0_data);
        e0_list.append(jv)

    e0_cumulative = int(e0_cumulative / (1000 * 1000))

    #sort each by runtime ascending
    e0_list.sort(key=lambda x: x["micros"])
    improvement = 0;
    for e in e0_list:
        improvement += e["improvement"]
                
    print (f"Extractor {e0}, total dag-cost improvement: {improvement:.1f} in {e0_cumulative}s")
    e0_value = []
    per_problem =0.0 #microseconds available per problem.
    
    for i in range(0, e0_cumulative + 1):
        finished = 0;
        spent = 0;
        for jv in e0_list:
            if (jv["micros"] < per_problem):
                finished+=1
                spent += jv["micros"]
            else:
                break #list is sorted, so we can stop here
        active = len(e0_list) - finished

        if active == 0:
            break
        per_problem = (1000000*i - spent) /active

        saving = 0
        
        for jv in e0_list:
            if jv["micros"] < per_problem:
                saving += jv["improvement"]
            else:
                break

        e0_value.append([i, saving, finished]);

    return e0_value

# This assumes an extractor is run on the all the egraph benchmarks at the same time.
# So given 500 egraphs, each will receive 1/500th of a second of that first second's
# CPU time. Say 10 egraphs finish processing with their extractor with less than 
# 1/500th of a second's CPU time, i.e. they have a runtime of less than 2ms. Then 
# for the 2nd second of CPU time, each egraph will get 1/490th of a second of CPU time.
# 
# Continuing the example, if those 10 egraphs which were processed in the first 1/500th
# of a second, each improved on the cost versus the reference implementation by 10%, 
# then the graph will plot an improvemement of 1/50th of 10%, that is 0.2% at 1 second.
# 
# At 2 seconds, the improvement will be the sum of the percentage improvement of all 
# the extractors which finished in less than 1/500th + 1/490th of a second, that is
# that finished with a total runtime of less than 4.04ms.
#
# This will continue until the timeout on the extractor is reached.
def graph(js):
    reference = "faster-greedy-dag"
    if not any(j["extractor"] == reference for j in js):
        print(f"Warning: no jsons found for {reference}")
        return

    extractors = set(j["extractor"] for j in js)
    
    # Tree cost extraction is solved.
    for item in ["bottom-up", "faster-bottom-up", reference]:
        if item in extractors:
            extractors.remove(item)

    ref_data = {}
    for j in js:
        if j["extractor"] == reference:
            ref_data[j["name"]] = j

    series = {}
    for e in extractors:
        series[e] = getSequence(js, e, reference, ref_data);

    #Plot in a graph.
    import matplotlib.pyplot as plt

    for s, values in series.items():
        x_values = [i[0] for i in values]
        y_values = [i[1] for i in values]
        plt.plot(x_values, y_values, label=s)

    plt.xlabel('Cumulative time (s)')
    plt.ylabel('Cumulative percentage improvement in DAG cost')
    plt.legend()
    plt.title('Improvement of extractors compared to ' + reference)
    plt.savefig('dag_cost_improvement.svg')

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
 
    print(f"cumulative tree cost for {e1}: {sum(d[e1]['tree'] for d in by_name.values()):.0f}")
    print(f"cumulative tree cost for {e2}: {sum(d[e2]['tree'] for d in by_name.values()):.0f}")
    print(f"cumulative dag cost for {e1}: {sum(d[e1]['dag'] for d in by_name.values()):.0f}")
    print(f"cumulative dag cost for {e2}: {sum(d[e2]['dag'] for d in by_name.values()):.0f}")

    print(f"Cumulative time for {e1}: {e1_cumulative/1000:.0f}ms")
    print(f"Cumulative time for {e2}: {e2_cumulative/1000:.0f}ms")

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

    for i in range(len(extractors)):
        for j in range(i + 1, len(extractors)):
            ex1, ex2 = extractors[i], extractors[j]
            if ex1 == ex2:
                continue
            print(f"###################################################\n{ex1} vs {ex2}\n\n")
            process(js, [ex1, ex2])
            print("\n\n")
    graph(js)
