# Awesome Rust Visualization

> Rust crates for data visualization and plotting.

The initial list has been populated with tools found primarily via GitHub and crates.io (using both tags and keyword searches).

## Contents
- [CPU-based rendering in pure Rust](#cpu-based-pure-rust)
- [GPU-based rendering in pure Rust](#gpu-pure-rust)
  - [wgpu-based tools](#wgpu-based)
- [Bindings to rendering code in other languages](#bindings)
  - [Grammar-based tools](#grammar-based)
  - [Web-based](#web-based)
- [Terminal and ASCII outputs](#text-based-outputs)

Please let me know if I got any categorizations incorrect (I mostly did brief checks of the README for each crate).


## Pure Rust

By "Pure Rust", this means the visualization logic is in pure Rust. However, it is not entirely accurate as some of these are still "bindings" to lower-level graphics libraries like Cairo. Even WGPU could be considered a "binding" to the lower-level Metal/Vulkan/etc APIs.


## CPU-based, Pure Rust

- https://github.com/plotters-rs/plotters
- https://github.com/SouthamptonRust/rustplot
- https://github.com/DougLau/splot
- https://github.com/coder543/dataplotlib
- https://github.com/limads/papyri
- https://github.com/ibrahimcesar/velociplot
- https://github.com/Ameyanagi/ruviz
- https://github.com/jonfres/fluent-plots
- https://github.com/BlondeBurrito/plotrs
- https://github.com/rtbo/plotive
- https://gitlab.com/Neek-sss/strafe/-/tree/master/strafe-plot

## GPU-based, Pure Rust

### WGPU-based
- https://github.com/jonmmease/avenger
- https://github.com/Ameyanagi/ruviz

### Coupled to GUI framework
- https://github.com/Joylei/plotters-iced (iced)
- https://github.com/ulikoehler/liveplot-rs (egui)
- https://github.com/pierreaubert/gpui-toolkit/tree/main/gpui-px (gpui)
- https://github.com/bgkillas/kalc-plot (kalc, egui?)
- https://github.com/emilk/egui_plot (egui)
- https://github.com/donkeyteethUX/iced_plot (iced)
- https://github.com/JakkuSakura/gpui-plot (gpui)
- https://github.com/longbridge/gpui-component/tree/main/crates/ui/src/chart (gpui)
- https://github.com/eliotbo/bevy_plot (bevy)
- https://github.com/rtbo/plotive/tree/main/iced (iced)


## Text-based outputs



## Bindings

This section includes bindings (to high-level visualization libraries implemented in other langauges) and wrappers and specification-based tools here.
Some of the above "Pure Rust" 


- https://github.com/cpmech/plotpy (matplotlib)
- https://github.com/SiegeLord/RustGnuplot (gnuplot)
- https://github.com/ploteria/ploteria (gnuplot)
- https://gitlab.com/ruivieira/matplotrust (matplotlib)
- https://github.com/sixalphaone/graphplot (typst)
- https://github.com/Latias94/dear-imgui-rs/tree/main/extensions/dear-implot (implot)
- https://github.com/DJDuque/pgfplots (latex)

### Grammar-based

- https://github.com/georgestagg/ggsql (vega-lite)
- https://github.com/procyon-rs/vega_lite_4.rs (vega-lite)
- https://github.com/wangjiawen2013/charton (vega-lite ?)
- https://github.com/cuprous-au/vega-view (vega-lite)


### Web-based

- https://github.com/plotly/plotly.rs (plotly)
- https://github.com/alceal/plotlars (plotly)
- https://github.com/yuankunzhang/charming (apache echarts)
- https://github.com/stevedonovan/flot-rs (flot)
- https://github.com/ondt/mapplot (google maps)
- https://github.com/micouy/plotka (multiple)


## Related Lists

- [awesome-biological-visualizations](https://github.com/keller-mark/awesome-biological-visualizations)
