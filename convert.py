# a utility to convert the old, csv-like format to the new json format

from collections import Counter
import sys
import json


def convert(f):
    count = Counter()
    j = {"nodes": {}}
    roots = set()
    for line in f.readlines():
        line = line.strip()
        root_prefix = "## root: "
        if line.startswith(root_prefix):
            roots.add(line[len(root_prefix) :])
        elif line.startswith("#"):
            j["comment"] = line
        else:
            eclass, cost, op, *children = (x.strip() for x in line.split(","))
            node_id = f"{eclass}__{count[eclass]}"
            j["nodes"][node_id] = {
                "op": op,
                "cost": float(cost),
                "eclass": eclass,
                "children": [f"{x}__0" for x in children],
            }
            count[eclass] += 1

    j["root_eclasses"] = list(roots)
    return json.dumps(j, indent=2)


if __name__ == "__main__":
    for filename in sys.argv[1:]:
        print(f"converting {filename}")
        with open(filename) as f:
            j = convert(f)
        outname = filename.replace(".csv", "") + ".json"
        outname = outname.replace(".json.json", ".json")
        with open(outname, "w") as f:
            f.write(j)
