import React, { useMemo } from 'react';
import * as zarr from 'zarrita';
import * as v05 from "zod-ome-ngff/0.5";
import { button, folder } from 'leva';
import { range } from 'lodash-es';
import {
  useQuery,
  QueryClient,
  QueryClientProvider,
} from '@tanstack/react-query';
import { PluotWrapper } from './PluotWrapper.jsx';

// Create a client
const queryClient = new QueryClient();

function hexToRgb(hex) {
  // Strip a leading "#" if present
  const cleaned = hex.replace(/^#/, "");

  if (!/^[0-9a-fA-F]{6}$/.test(cleaned)) {
    throw new Error(`Invalid hex color: "${hex}"`);
  }

  const r = parseInt(cleaned.slice(0, 2), 16);
  const g = parseInt(cleaned.slice(2, 4), 16);
  const b = parseInt(cleaned.slice(4, 6), 16);

  return { r, g, b };
}

async function queryFn(ctx) {
  const { store } = ctx.meta;
  const group = await zarr.open(store, { kind: "group" });
  const zattrs = group.attrs;

  const ngffAttrs = v05.ImageSchema.parse(zattrs);

  console.log(ngffAttrs)

  const channels = ngffAttrs.ome.omero?.channels;

  const hasZ = ngffAttrs.ome.multiscales?.[0]?.axes?.find(axisObj => axisObj.name === "z");

  if (Array.isArray(channels)) {

    // Based on the channel metadata, we want to set up BOTH the:
    // - Controls
    // - Callback function that returns plotParams, given the current control values.

    const controls = {};

    if (hasZ) {
      controls["target_z"] = {
        // TODO: load array to find number of Z slices
        // TODO: use omero.rdefs.defaultZ if provided,
        // OR derive based on num Z slices if not.
        value: 40,
        min: 0,
        max: 100,
        label: 'Z slice',
      };
    }

    channels.forEach((c, i) => {
      const channelName = c.label ?? `Channel ${i}`;

      // TODO: check omero.rdefs.model to determine whether RGB or not.
      // TODO: use a default palette for fallback colors based on channel index,
      // rather than always falling back to white.
      const channelColorObj = c.color
        ? hexToRgb(c.color)
        : { r: 255, g: 255, b: 255 };

      controls[channelName] = folder({
        [`channel_${i}___visible`]: {
          // Use c.active if provided
          value: c.active ?? true,
          label: 'Visible',
        },
        [`channel_${i}___color`]: {
          value: channelColorObj,
          label: 'Color',
        },
        [`channel_${i}___window`]: {
          // TODO: use array dtype if window.start/end are not present.
          value: [c.window?.start ?? 0.0, c.window?.end ?? 1.0],
          // TODO: use array dtype if window.min/max are not present.
          min: c.window?.min ?? 0.0,
          max: c.window?.max ?? 10000.0,
          label: 'Window',
        },
      }, { order: i });
    });

    const numChannels = channels.length;

    const getPlotParams = (currControls) => {
      const channelSettings = range(numChannels).map(cIndex => ({
        c_index: cIndex,
        window: currControls[`channel_${cIndex}___window`],
        color: [
          currControls[`channel_${cIndex}___color`].r / 255.0,
          currControls[`channel_${cIndex}___color`].g / 255.0,
          currControls[`channel_${cIndex}___color`].b / 255.0,
        ],
      })).filter(cObj => {
        const isVisible = currControls[`channel_${cObj.c_index}___visible`];
        return isVisible;
      });

      return {
        layers: [
          {
            layer_type: "AxisLinearLayer",
            layer_params: {
              layer_id: "bottom_axis",
              position: "Bottom",
            }
          },
          {
            layer_type: "AxisLinearLayer",
            layer_params: {
              layer_id: "left_axis",
              position: "Left",
            }
          },
          {
            layer_type: "OmeZarrMultiscaleLayer",
            layer_params: {
              layer_id: "ome_zarr_multiscale_layer",
              // TODO: use omero.rdefs.defaultZ if provided,
              // OR derive based on num Z slices if not.
              target_z: currControls['target_z'],
              // TODO: use omero.rdefs.defaultT if provided,
              // OR derive based on num T slices if not.
              target_t: 0,
              channel_settings: channelSettings,
              opacity: 1.0,
            },
          }
        ]
      };
    }; // End getPlotParams.
    return { controls, getPlotParams };
  }
}

export function BioimageWrapperInner(props) {
  const {
    plotId,
    storeUrl,
    cameraMatrix = null,
  } = props;

  // Create the FetchStore based on the url.
  const store = useMemo(() => {
    return new zarr.FetchStore(storeUrl);
  }, [storeUrl]);

  // Then, load the OME-NGFF metadata.
  // Create the plotSpecificControls to pass to PluotWrapper.
  // Populate the OmeZarrMultiscaleLayer params based on the control values. Perhaps plotParams should be a function.

  const controlsAndGetPlotParams = useQuery({
    queryKey: [storeUrl],
    meta: { store },
    queryFn,
  });

  if (controlsAndGetPlotParams.isPending) {
    return (<p>Loading...</p>);
  }
  if (controlsAndGetPlotParams.isError) {
    return (<p>Error.</p>);
  }

  const { controls, getPlotParams } = controlsAndGetPlotParams.data;

  return (
    <PluotWrapper
      plotId={plotId}
      plotType={"LayeredPlot"}
      storeUrl={store}
      plotParams={getPlotParams}
      viewMode={"2d"}
      marginLeft={100}
      marginTop={0}
      marginRight={0}
      marginBottom={100}
      plotSpecificOptions={controls}
      cameraMatrix={cameraMatrix}
      enablePicking={false}
    />
  );
}

export function BioimageWrapper(props) {
  return (
    <QueryClientProvider client={queryClient}>
      <BioimageWrapperInner {...props} />
    </QueryClientProvider>
  );
}
