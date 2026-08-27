# Dot Plot Category Selection Semantics

- A "no value provided" category selection means "every category of the group-by column," in its stored order — matching standard dot plot behavior (as in scanpy's `sc.pl.dotplot`).
- An explicitly empty category selection must be treated literally as "no categories," not silently treated as "all categories."
