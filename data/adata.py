# /// script
# requires-python = ">=3.12"
# dependencies = [
#   "zarr>=3",
#   "numpy>=2.0.0",
#   "pandas",
#   "scanpy>=1.12.3"
# ]
# ///

import scanpy as sc
from os.path import join
import matplotlib.pyplot as plt

adata = sc.datasets.pbmc68k_reduced()

# Run t-SNE to be able to demonstrate sc.pl.tsne.
sc.tl.tsne(adata)

adata_path = join("out", "pbmc68k.adata.zarr")
adata.write_zarr(adata_path)

adata.write_h5ad(join("out", "pbmc68k.h5ad"))

markers = ['C1QA', 'PSAP', 'CD79A', 'CD79B', 'CST3', 'LYZ']
sc.pl.dotplot(adata, markers, groupby='bulk_labels', show=False)
#plt.savefig("dotplot.png")
