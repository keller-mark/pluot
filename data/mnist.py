# /// script
# requires-python = ">=3.12"
# dependencies = [
#   "zarr==3.1.1",
#   "numpy>=2.0.0",
#   "pandas",
#   "umap-learn==0.5.5",
#   "scikit-learn==1.7.1",
#   "vega-datasets==0.9.0"
# ]
# ///


import numpy as np
import pandas as pd
#import matplotlib.pyplot as plt
#import seaborn as sns

import umap
from sklearn import datasets
from sklearn.datasets import make_blobs, make_classification, make_gaussian_quantiles
from vega_datasets import data as vega_data
from os.path import join
import zarr

RANDOM_SEED = 1111


# Disable compression until Zarrs-via-WASM supports Blosc and Zstd.
# Note: no longer needed.
# Reference: https://github.com/zarr-developers/zarr-python/issues/3389
no_compression = dict(filters=None, compressors=None, serializer="auto")

# Create a bar plot dataset
wheat_df = vega_data.wheat()
z = zarr.open(join("out", "wheat.zarr"))
z.create_array(name="/year", data=wheat_df["year"].values.astype(np.dtypes.StringDType), **no_compression)
z.create_array(name="/wheat", data=wheat_df["wheat"].astype(int), **no_compression)




mnist = datasets.fetch_openml("mnist_784")

mapper = umap.UMAP(random_state=RANDOM_SEED).fit(mnist.data)
dens_mapper = umap.UMAP(densmap=True, random_state=RANDOM_SEED).fit(mnist.data)

# convert the transformed data into dataframe
umap_df = pd.DataFrame(np.column_stack((mapper.embedding_, mnist.target)), columns=['X', 'Y', "Targets"])
densmap_df = pd.DataFrame(np.column_stack((dens_mapper.embedding_, mnist.target)), columns=['X', 'Y', "Targets"])

umap_df["Targets"] = umap_df["Targets"].astype(int)
densmap_df["Targets"] = densmap_df["Targets"].astype(int)


#sns.scatterplot(data=umap_df, x="X", y="Y", hue="Targets", s=0.3)
#sns.scatterplot(data=densmap_df, x="X", y="Y", hue="Targets", s=0.3)


z = zarr.open(join("out", "mnist.zarr"))

z.create_array(name="/umap/x_coords", data=umap_df["X"].astype(float).values, **no_compression)
z.create_array(name="/umap/y_coords", data=umap_df["Y"].astype(float).values, **no_compression)
z.create_array(name="/umap/class_labels", data=umap_df["Targets"].astype(int).values, **no_compression)
z.create_array(name="/umap/class_labels_str", data=umap_df["Targets"].astype(str).values.astype(np.dtypes.StringDType), **no_compression)

z.create_array(name="/densmap/x_coords", data=densmap_df["X"].astype(float).values, **no_compression)
z.create_array(name="/densmap/y_coords", data=densmap_df["Y"].astype(float).values, **no_compression)
z.create_array(name="/densmap/class_labels", data=densmap_df["Targets"].astype(int).values, **no_compression)
z.create_array(name="/densmap/class_labels_str", data=densmap_df["Targets"].astype(str).values.astype(np.dtypes.StringDType), **no_compression)

# Create fake datasets of other sizes
# Reference: https://scikit-learn.org/stable/datasets/sample_generators.html#generators-for-classification-and-clustering
z = zarr.open(join("out", "gaussian_quantiles.zarr"))

sizes = [100, 1000, 10000, 100000, 1000000, 10000000]

for size in sizes:
    X, Y = make_gaussian_quantiles(n_samples = size, n_features=3, n_classes=5, random_state=RANDOM_SEED)
    x_coords = X[:, 0]
    y_coords = X[:, 1]
    z_coords = X[:, 2]
    class_labels = Y

    z.create_array(name=f"/n_{size}/x_coords", data=x_coords.astype(float), **no_compression)
    z.create_array(name=f"/n_{size}/y_coords", data=y_coords.astype(float), **no_compression)
    z.create_array(name=f"/n_{size}/z_coords", data=z_coords.astype(float), **no_compression)
    z.create_array(name=f"/n_{size}/class_labels", data=class_labels.astype(int), **no_compression)
    z.create_array(name=f"/n_{size}/class_labels_str", data=class_labels.astype(np.dtypes.StringDType), **no_compression)


z = zarr.open(join("out", "gaussian_quantiles_compressed.zarr"))

sizes = [100, 1000, 10000, 100000, 1000000, 10000000]

compressors = zarr.codecs.BloscCodec(cname='zstd', clevel=3, shuffle=zarr.codecs.BloscShuffle.bitshuffle)

for size in sizes:
    X, Y = make_gaussian_quantiles(n_samples = size, n_features=3, n_classes=5, random_state=RANDOM_SEED)
    x_coords = X[:, 0]
    y_coords = X[:, 1]
    z_coords = X[:, 2]
    class_labels = Y

    z.create_array(name=f"/n_{size}/x_coords", data=x_coords.astype(float), compressors=compressors)
    z.create_array(name=f"/n_{size}/y_coords", data=y_coords.astype(float), compressors=compressors)
    z.create_array(name=f"/n_{size}/z_coords", data=z_coords.astype(float), compressors=compressors)
    z.create_array(name=f"/n_{size}/class_labels", data=class_labels.astype(int), compressors=compressors)
    z.create_array(name=f"/n_{size}/class_labels_str", data=class_labels.astype(np.dtypes.StringDType), compressors=compressors)
