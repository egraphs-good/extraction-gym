This dataset comes from a CGRA mapping tool called 'FlexC': https://arxiv.org/abs/2309.09112.

The e-graphs were derived by running equality saturation on the dataflow of loop bodies found in C code.
A simple cost model considers e-node operations to be either free (cost 0), costly (cost 1), or unavailable (cost 10 000).
The 'unavailable' cost of 10 000 encodes an infinite cost: picking these nodes would result in CGRA mapping failure.
As this is dataflow rewriting, extraction should ideally consider DAG cost.