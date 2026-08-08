import React, { useMemo } from 'react';
import { PluotWrapper } from './PluotWrapper.jsx';

// The purpose of this wrapper component is to define a dotplot-specific
// onHover function to transform the picking result into a tooltip dict.


function onHoverDotPlot(info) {
  const dotPlotInfo = info?.layer_results?.[0]?.info;
  // Note: Sort the dict based on the keys, since rust returns hashmap in any order
  return {
    'Cell Type': dotPlotInfo.obs_value,
    'Gene': dotPlotInfo.var_name,
    'Fraction expressing': dotPlotInfo.fraction_expressing,
    'Mean expression': dotPlotInfo.mean_expression,
  }
}

export function DotPlotWrapper(props) {

  return <PluotWrapper {...props} onHover={onHoverDotPlot} />
}
