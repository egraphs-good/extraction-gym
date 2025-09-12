# %%
import matplotlib.pyplot as plt
import numpy as np
import pandas as pd
import glob
import json
import os
plt.rcParams["figure.figsize"] = (10, 8)
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
df['benchset'] = df['name'].astype('string').str.split('/').str.get(1)
df.benchset.unique()

# %%
extractors = df.extractor.unique().tolist()
extractors
# %%
extractors = ['bottom-up', 'faster-greedy-dag', 'beam-1', 'beam-1-new', 'beam-2', 'beam-4', 'beam-8', 'beam-16', 'beam-1a', 'faster-ilp-cbc-timeout']

# Filter by extractors
df = df[df.extractor.isin(extractors)]

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
df.groupby(['benchset', 'extractor']).dag.count().unstack().plot(kind='bar', logy=True)

# %%
# Create a grouped bar chart comparing beam methods against 'faster-ilp-cbc-timeout' baseline by benchset
beam_methods = ['beam-1', 'beam-2', 'beam-4', 'beam-8', 'faster-greedy-dag']
benchset = df.groupby(['benchset', 'extractor'])['dag'].sum().unstack()

baseline = benchset['faster-ilp-cbc-timeout']
speedup_vs_baseline = benchset[beam_methods].div(baseline, axis=0).replace([np.inf, -np.inf], np.nan)
ax = speedup_vs_baseline.plot(kind='bar')
plt.axhline(1.0, color='gray', linestyle='--', linewidth=1)
plt.ylabel('DAG cost vs faster-ilp-cbc-timeout')
plt.xlabel('Benchset')
plt.title('Beam  vs faster-ilp-cbc-timeout (lower is better)')
plt.legend(title='Extractor')
plt.tight_layout()
plt.ylim(0.0, 2.0)
plt.show()
