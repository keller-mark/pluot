import React, { useMemo, useState } from 'react';
import { PluotWrapper } from './PluotWrapper.jsx';

// The class_labels column referenced below only takes integer values 0-4
// (see the Category10 colormap used for `color_key` in scatterplot.mdx).
const LABEL_VALUES = [0, 1, 2, 3, 4];

function toggleValue(values, value) {
  return values.includes(value)
    ? values.filter((v) => v !== value)
    : [...values, value].sort();
}

function LabelMultiSelect(props) {
  const { title, values, onChange } = props;
  return (
    <div style={{ display: 'inline-block', marginRight: '2rem' }}>
      <div>{title}</div>
      {LABEL_VALUES.map((value) => (
        <label key={value} style={{ marginRight: '0.75rem' }}>
          <input
            type="checkbox"
            checked={values.includes(value)}
            onChange={() => onChange(toggleValue(values, value))}
          />
          &nbsp;{value}
        </label>
      ))}
    </div>
  );
}

// This wrapper demonstrates `ZarrEmphasisCriteria`: filtering and selection
// criteria on `ZarrPointLayer` reference their categorical/quantitative
// column by zarr array path (`codes_key`), rather than embedding the column
// data inline. Here, both filtering and selection criteria reference the
// same `/n_1000000/class_labels` column, but with independent sets of
// included label values.
export function ScatterplotSelectFilterWrapper(props) {
  const { colorKey, ...otherProps } = props;

  // Filter-included label values: points whose label is not in this set are
  // omitted from rendering entirely.
  const [filterValues, setFilterValues] = useState(LABEL_VALUES);
  // Selection-included label values: filter-included points whose label is
  // not in this set are still rendered, but de-emphasized (background color).
  const [selectValues, setSelectValues] = useState([0, 1]);

  const plotParams = useMemo(() => ({
    layers: [
      {
        layer_type: "ZarrPointLayer",
        layer_params: {
          layer_id: "layer_1",
          data_unit_mode_x: "Data",
          data_unit_mode_y: "Data",
          point_radius_unit_mode_x: "Pixels",
          point_radius_unit_mode_y: "Pixels",
          point_shape_mode: "Circle",
          point_radius: 2.0,
          bounds: null,
          point_opacity: 0.5,

          x_key: "/n_1000000/x_coords",
          y_key: "/n_1000000/y_coords",
          color_key: colorKey,

          filtering_criteria: [
            {
              criteria_mode: "Categorical",
              criteria_params: {
                codes_key: colorKey,
                included_codes: filterValues,
              },
            },
          ],
          selection_criteria: [
            {
              criteria_mode: "Categorical",
              criteria_params: {
                codes_key: colorKey,
                included_codes: selectValues,
              },
            },
          ],
        }
      }
    ]
  }), [colorKey, filterValues, selectValues]);

  return (
    <div>
      <PluotWrapper
        {...otherProps}
        plotParams={plotParams}
      />
      <div style={{ margin: '10px 0' }}>
        <LabelMultiSelect title="Filtered labels (background)" values={filterValues} onChange={setFilterValues} />
        <LabelMultiSelect title="Selected labels (foreground)" values={selectValues} onChange={setSelectValues} />
      </div>
    </div>
  );
}
