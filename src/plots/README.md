# plots

## coordinate systems

The coordinate system for the plot is coupled to the camera and its view matrix.

In this document, we want to clarify the relationship between the camera, its view matrix, and the coordinate system.

When the camera view matrix is the identity matrix, what does this mean for the coordinate system?
How does this map onto the screen quad ((-1, 1), used in the shader)?
How does this map onto the texture coordinates?
<!-- Reference: https://github.com/gfx-rs/wgpu?tab=readme-ov-file#coordinate-systems -->
How does this map onto the data (intrinsic) coordinates?

How is the coordinate system affected by the aspect ratio of the viewport?
Basically, do we want the identity matrix to correspond to background-size "contain" or "cover" (in CSS terms)?
- Contain: Scales the image as large as possible within its container without cropping or stretching the image.
- Cover: Scales the image (while preserving its ratio) to completely fill the container, leaving no empty space. If the proportions of the image differ from the viewport, the image is cropped either vertically or horizontally.
<!-- Reference: https://developer.mozilla.org/en-US/docs/Web/CSS/background-size -->

### 2D camera and coordinate system

#### intrinsic (non-physical) coordinate system

For example, in a 2D UMAP scatterplot, we only have an intrinsic coordinate system (the X and Y values).

How is the coordinate system affected by the plot margins (marginLeft, marginTop, marginRight, marginBottom)?
Given the margins and the viewport width/height, we define an "adjusted screen quad" whose (-1 to 1) corresponds to the area inside the margins, but when rendered to screen it only corresponds to the smaller (adjusted_min_coord, adjusted_max_coord).

The (adjusted) screen quad should correspond directly to (-1, 1) in the data.


#### physical coordinate system

In a 2D image, the pixels have a physical size.
According to the OME model, the physical size of a pixel is 1 micrometer (um) by default.
<!-- Reference: https://www.openmicroscopy.org/Schemas/Documentation/Generated/OME-2016-06/ome.html -->

When the camera view matrix is the identity matrix, and the aspect ratio is 1:1 (square), the screen quad should correspond to a 2000 x 2000 pixel square ((-1mm, -1mm) in bottom left to (1mm, 1mm) in top right).
<!-- Reference: https://github.com/hms-dbmi/viv/blob/08a74203b99f54bc62307c741944ed61e33e810c/packages/layers/src/utils.js#L169 -->


### 3D (orbit) camera and coordinate system

#### intrinsic (non-physical) coordinate system

#### physical coordinate system
