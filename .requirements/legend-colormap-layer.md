# Quantitative Colormap Legend

- Provide a composite layer that renders a legend for a quantitative colormap: a horizontal gradient bar (low value on the left, high value on the right), built from 256 interpolated color steps.
- Support an optional title displayed above the gradient, and a linear axis with two tick labels displayed below the gradient.
- Should be positionable within the margin of plots that use it (e.g. the right margin of a dot plot).
- End tick labels must not visually overhang the plot's edge — the first and last tick should align inward, similar to d3's outer-tick-fitting behavior.
