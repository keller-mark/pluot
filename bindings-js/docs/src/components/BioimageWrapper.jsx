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

async function queryFn(ctx) {
  const { store } = ctx.meta;
  const group = await zarr.open(store, { kind: "group" });
  const zattrs = group.attrs;

  const ngffAttrs = v05.ImageSchema.parse(zattrs);

  console.log(ngffAttrs)

  const channels = ngffAttrs.ome.omero?.channels;

  if (Array.isArray(channels)) {

    // Based on the channel metadata, we want to set up BOTH the:
    // - Controls
    // - Callback function that returns plotParams, given the current control values.

    const controls = {};
    channels.forEach((c, i) => {
      const channelName = c.label ?? `Channel ${i}`;
      controls[channelName] = folder({
        [`channel_${i}___visible`]: {
          // TODO: use c.active if provided
          value: true,
          label: 'Visible',
        },
        [`channel_${i}___color`]: {
          // TODO: use c.color if provided.
          // If not, check omero.rdefs.model.
          value: {
            r: 255,
            g: 255,
            b: 255,
          },
          label: 'Color',
        },
        [`channel_${i}___window`]: {
          // TODO: use c.window.start, c.window.end if provided
          // OR use array dtype if not.
          value: [0.0, 90000.0],
          // TODO: use c.window.min, c.window.max if provided
          // OR use array dtype if not.
          min: 0.0,
          max: 100000.0,
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
              target_z: 40,
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
