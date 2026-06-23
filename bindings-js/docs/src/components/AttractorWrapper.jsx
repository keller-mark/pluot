import React, { useMemo } from 'react';
import * as zarr from 'zarrita';
import { PluotWrapper } from './PluotWrapper.jsx';



function getAttractorData(attractorType, N) {
  const xData = new Float64Array(N);
  const yData = new Float64Array(N);

  // Adapted from https://observablehq.com/@rreusser/selecting-the-right-opacity-for-2d-point-clouds
  switch (attractorType) {
    case 'Uniform grid':
      const w = Math.floor(Math.sqrt(N));
      for (var i = 0; i <= N; i++) {
        xData[i] = ((i % w) / w - 0.5) * initialAxisDimensions[0];
        yData[i] =
          (Math.floor(i / w) / w - 0.5) * initialAxisDimensions[1];
      }
      break;
    case 'Random':
      for (var i = 0; i <= N; i++) {
        xData[i] = (Math.random() - 0.5) * initialAxisDimensions[0];
        yData[i] = (Math.random() - 0.5) * initialAxisDimensions[1];
      }
      break;
    case 'Rossler': {
      let xn = 2.644838333129883,
        yn = 4.060488700866699,
        zn = 2.8982460498809814;
      let xn1, yn1, zn1;
      let a = 0.2;
      let b = 0.2;
      let c = 5.7;
      let dt = 0.006;
      for (var i = 0; i <= N; i++) {
        let dx = -yn - zn;
        let dy = xn + a * yn;
        let dz = b + zn * (xn - c);

        let xh = xn + 0.5 * dt * dx;
        let yh = yn + 0.5 * dt * dy;
        let zh = zn + 0.5 * dt * dz;

        dx = -yh - zh;
        dy = xh + a * yh;
        dz = b + zh * (xh - c);

        let xn1 = xn + dt * dx;
        let yn1 = yn + dt * dy;
        let zn1 = zn + dt * dz;

        xData[i] = xn1;
        yData[i] = yn1;

        xn = xn1;
        yn = yn1;
        zn = zn1;
      }
      break;
    }
    case 'Nose-Hoover attractor': {
      let xn = 2.644838333129883,
        yn = 4.060488700866699,
        zn = 2.8982460498809814;
      let xn1, yn1, zn1;
      let dt = 0.01;
      for (var i = 0; i <= N; i++) {
        let dx = yn;
        let dy = -xn + yn * zn;
        let dz = 1.5 - yn * yn;

        let xh = xn + 0.5 * dt * dx;
        let yh = yn + 0.5 * dt * dy;
        let zh = zn + 0.5 * dt * dz;

        dx = yh;
        dy = -xh + yh * zh;
        dz = 1.5 - yh * yh;

        let xn1 = xn + dt * dx;
        let yn1 = yn + dt * dy;
        let zn1 = zn + dt * dz;

        xData[i] = xn1;
        yData[i] = yn1;

        xn = xn1;
        yn = yn1;
        zn = zn1;
      }
      break;
    }
    case 'TSUCS 2 attractor': {
      let xn = 5,
        yn = 5,
        zn = 5;
      let xn1, yn1, zn1;
      let dt = 0.001;
      for (var i = 0; i <= N; i++) {
        let dx = 40.0 * (yn - xn) + 0.16 * xn * zn;
        let dy = 55.0 * xn - xn * zn + 20.0 * yn;
        let dz = 1.833 * zn + xn * yn - 0.65 * xn * xn;

        let xh = xn + 0.5 * dt * dx;
        let yh = yn + 0.5 * dt * dy;
        let zh = zn + 0.5 * dt * dz;

        dx = 40.0 * (yh - xh) + 0.16 * xh * zh;
        dy = 55.0 * xh - xh * zh + 20.0 * yh;
        dz = 1.833 * zh + xh * yh - 0.65 * xh * xh;

        let xn1 = xn + dt * dx;
        let yn1 = yn + dt * dy;
        let zn1 = zn + dt * dz;

        xData[i] = xn1 * 0.1;
        yData[i] = zn1 * 0.1;

        xn = xn1;
        yn = yn1;
        zn = zn1;
      }
      break;
    }
  };
  return [xData, yData];
}

async function fillAttractorStore(store, attractorType, N) {
  const [xData, yData] = getAttractorData(attractorType, N);
  console.log(xData, yData)

  const h = zarr.root(store);
	const xArr = await zarr.create(h.resolve(`/${attractorType}/${N}/X`), {
    shape: [N],
		chunk_shape: [N],
		data_type: "float64",
    fill_value: 0,
    codecs: [
      {
  			"name": "bytes",
  			"configuration": {
  				"endian": "little"
  			}
  		}
		]
  });
	const yArr = await zarr.create(h.resolve(`/${attractorType}/${N}/Y`), {
    shape: [N],
		chunk_shape: [N],
		data_type: "float64",
    fill_value: 0,
    codecs: [
      {
  			"name": "bytes",
  			"configuration": {
  				"endian": "little"
  			}
  		}
		]
  });
	const colorArr = await zarr.create(h.resolve(`/${attractorType}/${N}/color`), {
    shape: [N],
		chunk_shape: [N],
		data_type: "int64",
    fill_value: 0,
    codecs: [
      {
  			"name": "bytes",
  			"configuration": {
  				"endian": "little"
  			}
  		}
		]
	});

  await zarr.set(xArr, null, { data: xData, shape: [N], stride: [1] });
  await zarr.set(yArr, null, { data: yData, shape: [N], stride: [1] });
  await zarr.set(colorArr, null, { data: new BigInt64Array(N), shape: [N], stride: [1] });
}

export function AttractorWrapper(props) {
  const {
    attractorType = "Rossler",
    numPoints =  1000000
  } = props;

  // Create the FetchStore based on the url.
  const store = useMemo(() => {
    const memoryStore = new Map();
    fillAttractorStore(memoryStore, attractorType, numPoints);

    return memoryStore;
  }, [attractorType, numPoints]);



  return (
    <PluotWrapper
        plotId={"scatterplot-attractor"}
        plotType={"LayeredPlot"}
        storeUrl={store}
        plotParams={{
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
                point_radius: null,
                bounds: null,
                point_opacity: null,

                x_key: `/${attractorType}/${numPoints}/X`,
                y_key: `/${attractorType}/${numPoints}/Y`,
                color_key: `/${attractorType}/${numPoints}/color`,
              }
            }
          ]
        }}
      viewMode={"2d"}
      cameraMatrix={[
        0.05, 0.0, 0.0, 0.0,
        0.0, 0.05, 0.0, 0.0,
        0.0, 0.0, 1.0, 0.0,
        0.0, 0.0, 0.0, 1.0
      ]}
    />

  );
}
