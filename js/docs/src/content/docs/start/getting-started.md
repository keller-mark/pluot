---
title: Getting Started
description: A guide in my new Starlight docs site.
sidebar:
    # Set a custom order for the link (lower numbers are displayed higher up)
    order: 10
---

Pluot is an attempt at the lofty goal of 'write once, run everywhere' visualization software.

## Motivations

At its core, this project is motivated by code reuse.
It should be possible to implement a particular data visualization rendering function once, and then reuse the software in multiple contexts: as an interactive plot within a web application, or to generate a publication-quality static figure for a scientific paper.

We should not be satisfied with the status quo in which countless hours are spent building web-based interactive visualization tools, only to reach for entirely separate code in Python or R when it comes time to create a static plot.
Until now, achieving code reuse in these situations has required workarounds such as heavyweight browser automation tools or embedded JavaScript runtimes.
The recent technologies of WebAssembly and WebGPU, along with advancements in the Rust ecosystem, make the timing ripe to address this challenge in a way that avoids such workarounds.


## Further reading

- For more details on personal motivations and how this connects to reproducibility in science, see my [blog post](https://github.com/keller-mark/blog/blob/main/2026-01-12-pluot-motivations.md).
