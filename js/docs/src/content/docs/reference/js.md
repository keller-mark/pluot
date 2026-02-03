---
title: Usage from JavaScript
description: How to render plots from JavaScript.
sidebar:
    # Set a custom order for the link (lower numbers are displayed higher up)
    order: 20
---

## Vanilla JavaScript

```sh frame="none"
npm install @pluot/core
```

TODO

## React component

The `@pluot/react` NPM package provides the `<Pluot />` React component.

```sh frame="none"
npm install @pluot/react
```

### Props

- width
- height
- plotId
- plotType
- storeName
- plotParams
- mode = "2d"
- marginBottom = 0.0
- marginLeft = 0.0
- marginTop = 0.0
- marginRight =  0.0
- aspectRatioMode = "contain" // "ignore", "contain", "cover"
- format = "vector" // "vector", "raster"



### Example

```jsx
import React from 'react';
import { Pluot } from '@pluot/react';
import { FetchStore } from 'zarrita';

const store = new FetchStore('https://example.com/my_dataset.zarr');

export function MyPlot(props) {
    return (
        <Pluot
            width={500}
            height={500}
            plotId={"docs-example-scatterplot"}
            plotType={"LayeredPlot"}
            store={store}
            plotParams={{
                x_key: "/n_1000000/x_coords",
                y_key: "/n_1000000/y_coords",
                color_key: "/n_1000000/class_labels",
                point_radius: 5.0,
            }}
            mode={"2d"}
        />
    );
}
```