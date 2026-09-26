import React from 'react';
import { PluotWrapper } from './PluotWrapper.jsx';

// The purpose of this wrapper component is to define a heatmap-specific
// onHover function to transform the picking result into a tooltip dict.

const numFormatter = new Intl.NumberFormat('en-US', {
  minimumFractionDigits: 2,
  maximumFractionDigits: 2
});

function onHoverHeatmap(info) {
  const heatmapInfo = info?.layer_results?.[0]?.info;
  if (!heatmapInfo) {
    // The heatmap layer may not have rendered/prepared yet.
    return undefined;
  }
  return {
    'Cell': heatmapInfo.obs_name,
    'Gene': heatmapInfo.var_name,
    'Expression': numFormatter.format(Number(heatmapInfo.value)),
  }
}

export function HeatmapWrapper(props) {
  return (
    <PluotWrapper {...props} onHover={onHoverHeatmap} enableTooltip />
  );
}
