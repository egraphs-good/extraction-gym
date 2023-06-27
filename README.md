# Extraction Gym

A suite of benchmarks to test e-graph extraction algorithms.

Add your algorithm in `src/extract` and then add a line in `src/main.rs`. 
To run, type `make`.

## Data

Please add data! It's just a csv with the following schema:

```
eclass:str, cost:float, node_name:str, eclass_children:str*
```

There is a special `##` directive to specify the root(s) of the e-graph.
You can have multiple of these on separate lines.

### Example

Here's an e-graph with `f(g(x)) = h(y)`, and everything has cost `1` except for `h` which has cost `7.5`:

```csv
## root: 2
0,   1, x
1,   1, g, 0
2,   1, f, 1
# this is a comment, starting the second "term"
3,   1, y
2, 7.5, h, 3
```