import React, { useState } from 'react';
import { NO_BRUSH } from '@pluot/react';
import { PluotWrapper } from './PluotWrapper.jsx';

// This wrapper demonstrates the brushing props of the Pluot component:
// a brush shape (Rect / Polygon / RangeX / RangeY) and a units mode
// (Data / Pixels / Normalized) for each axis, plus a controlled `brush`
// state that is echoed back below the plot as it is drawn/edited/cleared.
export function BrushWrapper(props) {
  const { plotParams, ...otherProps } = props;

  const [brushMode, setBrushMode] = useState("Rect");
  const [brushUnitsMode, setBrushUnitsMode] = useState("Data");
  // NO_BRUSH (not undefined) is the empty state, so this stays controlled throughout.
  const [brush, setBrush] = useState(NO_BRUSH);

  return (
    <div>
      <PluotWrapper
        {...otherProps}
        plotParams={plotParams}
        brushMode={brushMode}
        brushUnitsModeX={brushUnitsMode}
        brushUnitsModeY={brushUnitsMode}
        enableBrushCreate
        enableBrushEdit
        enableBrushClear
        brush={brush}
        onBrush={(state) => setBrush(state)}
        onBrushEnd={(state) => setBrush(state)}
        onBrushClear={() => setBrush(NO_BRUSH)}
      />
      <div style={{ margin: '10px 0' }}>
        <label>Brush shape:&nbsp;
          <select value={brushMode} onChange={(e) => setBrushMode(e.target.value)}>
            <option value="Rect">Rect</option>
            <option value="Polygon">Polygon (lasso)</option>
            <option value="RangeX">RangeX (horizontal range)</option>
            <option value="RangeY">RangeY (vertical range)</option>
          </select>
        </label>
        &nbsp;
        <label>Brush units:&nbsp;
          <select value={brushUnitsMode} onChange={(e) => setBrushUnitsMode(e.target.value)}>
            <option value="Data">Data (follows the camera)</option>
            <option value="Pixels">Pixels</option>
            <option value="Normalized">Normalized</option>
          </select>
        </label>
        <div>
          {brush === NO_BRUSH
            ? "Long-click (1.5s) inside the plot, then drag, to brush."
            : `${brush.status} ${brush.shape} with ${brush.vertices.length} vertices`}
        </div>
      </div>
    </div>
  );
}
