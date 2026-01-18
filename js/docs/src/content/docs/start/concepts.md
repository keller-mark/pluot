---
title: Key Concepts
description: A guide in my new Starlight docs site.
sidebar:
    # Set a custom order for the link (lower numbers are displayed higher up)
    order: 20
---

## Rust core

Plot rendering functions are implemented in Rust.
We use the `wgpu` Rust crate for efficient raster-based plotting via WebGPU.

## Programming language bindings

Many programming languages offer ways to call Rust code despite being a different language.
Such functions are referred to as bindings.
Important tools for implementing bindings to Rust code are PyO3 and Maturin in Python, `wasm-pack` and `wasm-bindgen` in JavaScript, and `extendr` in R.
These make it possible to do both of the following:
- Call a Rust function from a different language `lang`, and return some bytes from Rust to this non-Rust language
  - Within this called Rust function, call a non-Rust function defined by `lang` and use its return value in the Rust function


## Lazy (async) data loading

A naive way to implement a plotting function is to pass arrays of data as parameters to the function (e.g., `scatterplot(x_arr, y_arr)` which renders every XY coordinate that is passed).

However, in order to scale to large datasets, we need mechanisms by which the visualization rendering code can make requests to particular chunks of data (potentially at particular [resolutions](https://en.wikipedia.org/wiki/Pyramid_(image_processing))).
In addition, large datasets that we want to visualize are often hosted remotely (e.g., in object storage systems like S3 buckets).
Our plot rendering functions must be `async` as they may need to load this data asynchronously.

When plotting functions are used via their programming language bindings, in addition to returning rendered pixels to the calling language, we need to ensure that Rust can make `async` requests for data from the calling language.
For example, when plotting a Numpy array, rather than passing this array to Rust up-front, we wait for our Rust plotting function to make a request for a slice of the Numpy array (e.g., the data currently visible in the viewport).
In order to make such a request, our Rust code will call a Python function which will return bytes corresponding to a subset of the Numpy array.
Finally, our Rust `async` plotting function will render the Numpy data and return the graphical output (either the pixels or the vector nodes).


```rs
// Pseudocode
async fn render_plot(params: PlotParams) -> Vec[u8] {
    // When called from a different programming language,
    // `get_data` will be an async function defined in this
    // language (not Rust).
    let plot_data: Bytes = get_data(&params).await;

    // Next, we use WGPU to plot the data we receive.
    let pixels = render_internal(&params, &plot_data).await;

    // Finally, we return the pixels to the calling language.
    return pixels;
}
```


## Headless plotting

In Pluot, our Rust plot rendering logic is decoupled from any particular windowing or GUI system, meaning Pluot performs "headless" plotting.
Instead, the plot rendering functions return bytes representing either pixels (in the raster case) or vector nodes/SVG strings (in the vector case).

What to do with the returned bytes is up to the caller of plot rendering function.

## Interactive plotting

In order to implement interactive plotting, the caller of the plot rendering function must handle user interactions: hovering, clicking, dragging (panning, brushing, lassoing), scrolling (zooming), etc.
Upon such an interaction, the caller must update its state, then re-render the plot by calling the plot rendering function with updated parameters.
Crucially, the plot rendering function must be performant enough to achieve high frame rates.

We provide a React component that supports these interactions, enabling interactive plotting in web applications.

To support similar interactions in a desktop application context, analogous interaction handlers must be implemented in (/ported to) whatever GUI framework is being used.

### Timeouts

As noted above, we are often plotting data that is stored remotely, requiring network requests to retreive the data prior to rendering it in a visualization.
We must account for slow network connections and request failures.
Pluot handles this with a `timeout` parameter that is passed to the plot rendering function.


Recall that Pluot is designed to work in both static and interactive plotting scenarios.
When creating static plots, we often want to wait for all data to be received prior to plot rendering.
This differs from interactive scenarios, in which we often want to render visualizations incrementally, so that the user begins to see a subset of data while the rest is still loading.
In interactive scenarios, we can set `timeout` to a small value such as `100ms`, after which Pluot will return some pixels regardless of whether all data has been received.
These returned pixels will be accompanied by a flag to indicate to the caller whether the visualization is complete or not.
(How to use this flag value is up to the caller, for instance, to show a loading indicator.)
In the latter case, the caller can wait an animation frame and call the plot rendering function again.

### Coordinated Multiple Views

Pluot's plot rendering functions are concerned with rendering a single plot.
By extension, Pluot is agnostic to any particular implementation of coordinated multiple views (i.e., linked interactive plots).
This enables developers to use their favorite state management library, and decouples Pluot from the state management library du jour.

For example, when using Pluot as a React component in a web application, you could implement CMV with [Use-Coordination](https://github.com/keller-mark/use-coordination). Alternatively, you could use plain React `useState`.

## Layer-based API

We provide a layer-based API that enables developers to implement custom plotting functions.
Several core layers are implemented, including ScatterplotLayer and LineLayer.

For more details on how to compose the existing layers or implement custom layers, see the [Rust API](/reference/rust/) documentation.
