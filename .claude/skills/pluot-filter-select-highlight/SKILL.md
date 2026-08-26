---
name: pluot-filter-select-highlight
description: Use when modifying the filtering, selection, or highlighting logic in Pluot.
---

We want to ensure the following is the case across layer implementations:

- Filtering refers to whether particular data items (or sets of data items) are considered at all during either rendering or computation. For example, if the filtering logic specifies that only immune cell types are included, then we do not render non-immune cell types (as data points, along categorical axes, in legends, etc.) In other words, when immune cell types are the only cell types that meet the current filtering criteria, we do not include non-immune cell types in any visual encoding (or upstream computation of a visual encoding, such as when computing distributions or averages).
- Selection refers to a visual emphasis on one or more data items or sets of data items (e.g., immune cell types). Items (or sets of items) which meet the filtering criteria, yet are un-selected, should still be rendered (as data points, along categorical axes, in legends), but should be de-emphasized visually (e.g., greyed-out or with reduced opacity or reduced size).
- Highlighting refers to ephemeral emphasis on one or a few data items (observation or feature) (e.g., a particular cell or gene). This emphasis should correspond to a visual encoding such as an outline.


When filtering is `None`, this means that all data items (or sets of data items) are included. When selection is `None`, this means that all data items (or sets of data items) that meet the filtering criteria are selected. An explicit empty array means that nothing should be included/selected. When highlight is `None`, this means that nothing is currently highlighted.


Filtering criteria determines the "background" items and visual representation.
Selection critiera determines the "foreground" items and visual representation.


## Selection criteria can be orthgonal to filtering criteria

When filtering criteria is defined in terms of included categories of items (e.g., cell types), selection criteria is not limited to a subset of the filter-included categories. Rather, selection criteria may be entirely orthogonal to the filtering criteria, for instance, selection of cells which express a certain gene (regardless of cell type).

## Types of filtering and selection criteria

- Integer instance ID column (one per dataset item) plus list of included instance IDs
- Categorical column (one category code per dataset item) plus list of included category codes (leverage the categories+codes dictionary format when implementing)
- Quantitative column (one value per dataset item) plus the included range/extent (or a min/max to specify only a lower/upper bound and implicitly infinity/negative-infinity in the other direction).


## GPU-accelerated implementation of filtering, selection, and highlighting

Not yet implemented for all layer types; work in progress.

For each primitive (i.e., not composite) layer drawing function (i.e., in `crates/pluot_core/src/layers`), the layer parameters must accept selection and filtering criteria alongside the associated NumericData data buffers.
We will reuse NumericData buffers when possible (e.g., when filter criteria exactly equals selection criteria).
For filter-excluded data items, the WGSL logic will omit/hide these items (i.e., not render them) when encountered in the shaders.
Filter-excluded items will also be ignored in picking logic (i.e., not returned as hits when picking).
For filter-included, but selection-excluded data items, the WGSL logic will de-emphasize these data items (i.e., gray-out or render with reduced opacity).
For filter-included, select-included, but not highlighted items, these will render with normal emphasis using the specified visual encoding (color, fill/stroke, size, opacity)
For highlighted data items, the WGSL logic will further emphasize these data items (i.e., render with a dark stroke or render with increased size).

GPGPU compute operations (i.e., in `crates/pluot_core/src/compute`) must support analogous filtering and selection logic.
For example, an operation which computes the distribution of items-per-category will compute counts for "background" (filter-included) and "foreground" (filter-included and selection-included), where filtering and selection criteria may rely on independent/orthogonal categorical or quantitative variables themselves.


<!-- TODO: also define principles for handing missingness / null items in nullable data arrays? -->
