Files are generated from the respective egglog with, e.g:
cargo run  -- --to-svg ..lots_of_paths_through_cycle.egg
cargo run  -- --to-json ..lots_of_paths_through_cycle.egg

Then the json manually edited to mark the root class.

tree - should be optimal for every extractor
tree with cycles - should be optimal after self-loops are removed
lots_of_paths_through_cycle - will be slow if the extractor explores cycles path-by-path.