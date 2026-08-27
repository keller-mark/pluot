# AnnData Dot Plot Layer

- Loads the group-by column, gene index, observation index, and requested expression values from an AnnData-backed source, limited to the specifically requested genes.
- For each gene/category pair, computes a color from mean expression and a size from fraction of cells expressing above a configurable cutoff — matching standard dot plot semantics (as in scanpy's `sc.pl.dotplot`).
- Renders the dots as a point-based sublayer, plus axis sublayers for the gene and category axes.
- Supports picking (see dot plot picking output requirement).
