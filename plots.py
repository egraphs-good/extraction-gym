# %%
import matplotlib.pyplot as plt
import numpy as np
import pandas as pd
import glob
import json
import os
plt.rcParams["figure.figsize"] = (10,8)
plt.rcParams["figure.dpi"] = 120

# %%
# Read json files from ./output
output_dir = "output"
data = []
for filepath in glob.glob(os.path.join(output_dir, '**', '*.json'), recursive=True):
    with open(filepath, 'r') as f:
        try:
            data.append(json.load(f))
        except:
            continue
print(len(data))
df = pd.DataFrame(data)
df.head()

# %%
extractors = df.extractor.unique().tolist()
extractors

# %%
for extractor in extractors:
    time = df[df.extractor == extractor].micros.to_numpy() * 1e-6
    time.sort()
    plt.plot(time, label=extractor)
plt.legend()
plt.xlabel("Run")
plt.ylabel("Time (seconds)")
plt.yscale("log")
plt.title("Extractor Time Comparison")
plt.show()
# %%
df[df.extractor == 'ilp-scip']

# %%
for extractor in extractors:
    time = df[df.extractor == extractor].dag.to_numpy()
    time.sort()
    plt.plot(time, label=extractor)
plt.legend()
plt.xlabel("Run")
plt.ylabel("DAG cost")
plt.yscale("log")
plt.title("Extractor DAG cost Comparison")
plt.show()

# %%
