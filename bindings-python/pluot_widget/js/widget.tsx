import React, { useMemo } from "react";
import { createRoot } from "react-dom/client";
import type { AnyModel, Render } from "@anywidget/types";
import { Pluot, setStoreByName } from "@pluot/react";
import {
  createKernelStore,
  createZarrGetter,
  useModelValue,
  useThrottledCameraMatrix,
  type WidgetModel,
} from "./hooks";

function PluotWidget({ model }: { model: AnyModel<WidgetModel> }) {
  const width = useModelValue(model, "width");
  const height = useModelValue(model, "height");
  const marginTop = useModelValue(model, "margin_top");
  const marginRight = useModelValue(model, "margin_right");
  const marginBottom = useModelValue(model, "margin_bottom");
  const marginLeft = useModelValue(model, "margin_left");
  const aspectRatioMode = useModelValue(model, "aspect_ratio_mode");
  const aspectRatioAlignmentMode = useModelValue(model, "aspect_ratio_alignment_mode");
  const viewMode = useModelValue(model, "view_mode");
  const plotId = useModelValue(model, "plot_id");
  const plotType = useModelValue(model, "plot_type");
  const storesMetadata = useModelValue(model, "stores_metadata");
  const plotParams = useModelValue(model, "plot_params");
  const format = useModelValue(model, "format");
  const minTimeout = useModelValue(model, "min_timeout");

  const zarrGet = useMemo(() => createZarrGetter(model), [model]);

  // Registered during render rather than in an effect, so that the kernel-backed
  // stores exist before <Pluot/> issues its first read.
  useMemo(() => {
    for (const storeName of Object.keys(storesMetadata ?? {})) {
      setStoreByName(storeName, createKernelStore(zarrGet, storeName));
    }
  }, [zarrGet, storesMetadata]);

  const [cameraMatrix, setCameraMatrix] = useThrottledCameraMatrix(model);

  return (
    <Pluot
      width={width}
      height={height}
      plotId={plotId}
      plotType={plotType}
      plotParams={plotParams}
      stores={storesMetadata}
      registerStores={false}
      viewMode={viewMode}
      format={format}
      marginTop={marginTop}
      marginRight={marginRight}
      marginBottom={marginBottom}
      marginLeft={marginLeft}
      aspectRatioMode={aspectRatioMode}
      aspectRatioAlignmentMode={aspectRatioAlignmentMode}
      cameraMatrix={cameraMatrix}
      setCameraMatrix={setCameraMatrix}
      minTimeout={minTimeout}
    />
  );
}

const render: Render<WidgetModel> = ({ model, el }) => {
  const root = createRoot(el);
  root.render(<PluotWidget model={model} />);
  return () => root.unmount();
};

export default { render };
