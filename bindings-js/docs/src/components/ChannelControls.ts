/// TODO: given a Zarrita store pointing to an OME-NGFF image:
// Load its zattrs, parse via zod-ome-ngff, and then return the required plotSpecificOptions
// to pass to the Leva PlotControls function.
import * as zarr from 'zarrita';
import * as v05 from "zod-ome-ngff/0.5";
import { button, folder } from 'leva';


export async function getChannelControls(store: zarr.AsyncReadable) {
  const group = await zarr.open(store, { kind: "group" });
  const zattrs = group.attrs;

  const ngffAttrs = v05.ImageSchema.parse(zattrs);

  console.log(ngffAttrs)

  const channels = ngffAttrs.ome.omero?.channels;

  if (Array.isArray(channels)) {
    const controls: any = {};
    channels.forEach((c, i) => {
      const channelName = c.label ?? `Channel ${i}`;
      controls[channelName] = folder({
        [`channel_${i}___visible`]: {
          value: true,
          label: 'Visible',
        },
      }, { order: i });
    });
    return controls;
  }

  return {};
}
