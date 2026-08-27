# Dot Plot Progressive Rendering

- The dot plot should render incrementally rather than waiting for all data to load: the gene axis appears immediately (known from parameters alone), the category axis appears once its data loads, and each gene's points appear as soon as that gene's data loads.
- Whatever subset of genes has finished loading must be passed through to rendering immediately — the plot must not wait for every requested gene to finish before showing any points.
- Loading/timeout ("bailed early") state should follow the same detection pattern already used by other layers, and should reflect whether all requested genes' data has finished loading.
