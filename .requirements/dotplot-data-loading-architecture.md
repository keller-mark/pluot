# Dot Plot Data Loading: Small, Independently-Cached Functions

- Data loading for dot plot visualizations should be built from small, purpose-built functions operating directly on low-level I/O, not a generalized grouping/predicate abstraction.
- Each independently loadable/computable piece of data (gene names, group-by column, individual gene expression columns, per-gene per-category summaries) must be cached separately, so requesting or changing one gene/category doesn't invalidate or block on any other.
- Generalized/abstracted prior data-loading machinery should be fully replaced, not kept alongside the new approach.
