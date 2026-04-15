import { useMemo } from "react";
import { checkWebGpuFeatureDetection } from "@pluot/core";

export function useWebGpuFeatureDetection() {
  const [supportsWebGpu, supportsWebGpuMessage] = useMemo(checkWebGpuFeatureDetection, []);
  return { supportsWebGpu, supportsWebGpuMessage };
}
