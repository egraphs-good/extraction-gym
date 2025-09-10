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
extractors = ['faster-ilp-cbc-timeout', 'bottom-up', 'faster-greedy-dag', 'beam-1', 'beam-2', 'beam-4', 'beam-8', 'beam-16']

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

# Find the minimum dag cost for each benchmark
# Determine the benchmark identifier column
min_dag_per_benchmark = df.groupby('name', dropna=False)['dag'].min().reset_index().rename(columns={'dag': 'min_dag'})
df = df.merge(min_dag_per_benchmark, on='name', how='left')
df['dag_ratio'] = df['dag'] / df['min_dag']
df.head()

# %%

for extractor in extractors:
    time = df[df.extractor == extractor].dag_ratio.to_numpy()
    time.sort()
    plt.plot(time, label=extractor)
plt.legend()
plt.xlabel("Run")
plt.ylabel("DAG cost")
plt.ylim(0.99, 1.2)
plt.title("Extractor DAG cost Comparison")
plt.show()

# %%
