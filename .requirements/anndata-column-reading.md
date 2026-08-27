# AnnData Column Reading (Dense & Sparse)

- Support reading a single data column (e.g. one gene's expression) from dense, CSC-sparse, and CSR-sparse storage layouts, dispatching automatically based on the array's storage encoding so callers don't need to know which layout is in use.
- Preserve each array's actual numeric type end-to-end rather than always widening/casting to a single common type, including at individual value access.
- Reads of large on-disk arrays must be chunked, bounded by a fixed size threshold per read, discarding each chunk once processed, to avoid loading excessive data into memory at once.
