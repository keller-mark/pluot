---
name: pluot-reference-materials
description: Use when the user asks you to reference specific code, documentation files, or source repositories of dependencies or related tools.
---


The (git-ignored) `references` directory (at the root of the repo) is intended as a space to temporarily place read-only copies of code or documentation files/folders.
These copies allow us to reference the logic or specifications of dependencies, related tools, or data formats by reading local files within the `references` directory.

## Pulling in a new reference

If the user asks you to pull in a new reference material/document, they will specify a URL to a file or github repository. If the user specifies a github repository, use `degit` to pull in a lightweight copy of the repo or a user-specified subset of it (this avoids pulling in the full repo commit history). Store the files/folders in a new subdirectory of `references`.

## Examples

- The AnnData on-disk format: https://github.com/scverse/anndata/blob/main/docs/fileformat-prose.md
- The SpatialData design document: https://github.com/scverse/spatialdata/blob/main/docs/design_doc.md
- The OME-NGFF specification and RFCs: https://github.com/ome/ngff
- OME-Zarr RFC-5 transformations: https://github.com/clbarnes/ome_zarr_transformations_conformance
- Vitessce Bitmask Layer files:
  - https://github.com/vitessce/vitessce/blob/main/packages/gl/src/BitmaskLayerBeta.js
  - https://github.com/vitessce/vitessce/blob/main/packages/gl/src/bitmask-layer-beta-shaders.js
  - https://github.com/vitessce/vitessce/blob/main/packages/gl/src/bitmask-utils.js
  - https://github.com/vitessce/vitessce/blob/main/packages/gl/src/bitmask-utils.test.js
- AnnData.js handling of sparse arrays: https://github.com/ilan-gold/anndata.js/blob/main/src/sparse_array.ts
- Vitessce handling of sparse arrays: https://github.com/vitessce/vitessce/blob/main/packages/file-types/zarr/src/anndata-loaders/ObsFeatureMatrixAnndataLoader.js
- D3 scales: https://github.com/d3/d3-scale
- D3 axes: https://github.com/d3/d3-axis
