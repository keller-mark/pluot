import React from 'react';
import { PluotWrapper } from './PluotWrapper.jsx';

// The purpose of this wrapper component is to define a dotplot-specific
// onHover function to transform the picking result into a tooltip dict.

const numFormatter = new Intl.NumberFormat('en-US', {
  minimumFractionDigits: 2,
  maximumFractionDigits: 2
});

// TODO: Move this logic to Rust, so that the behavior can be used
// across platforms, which would remove the need for this DotPlot-specific Wrapper component.
function onHoverDotPlot(info) {
  const dotPlotInfo = info?.layer_results?.[0]?.info;
  // Note: Sort the dict based on the keys, since rust returns hashmap in any order
  return {
    'Cell Type': dotPlotInfo.obs_value,
    'Gene': dotPlotInfo.var_name,
    'Fraction expressing': numFormatter.format(dotPlotInfo.fraction_expressing),
    'Mean expression': numFormatter.format(dotPlotInfo.mean_expression),
  }
}

export function DotPlotWrapper(props) {
  return (
    <PluotWrapper {...props} onHover={onHoverDotPlot} enableTooltip />
  );
}
