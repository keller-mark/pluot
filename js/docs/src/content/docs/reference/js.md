---
title: Usage from JavaScript
description: A reference page in my new Starlight docs site.
sidebar:
    # Set a custom order for the link (lower numbers are displayed higher up)
    order: 20
---

Install the `pluot` JavaScript package from [NPM](www.npmjs.com/package/pluot).


```sh frame="none"
npm install pluot
```

## React component

The `pluot` package provides the `<Pluot />` React component.

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
import { Pluot } from 'pluot';

export function MyPlot(props) {
    return (
        <Pluot
            width={500}
            height={500}
            plotId={"docs-example-scatterplot"}
            plotType={"LayeredPlot"}
            storeName={"gaussian_quantiles_store"}
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