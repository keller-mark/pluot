Code loosley inspired by DeckGL, but will diverge further. Primarily only using its Model abstraction, without porting other things directly.

The Layer/View/Deck abstractions will diverge further.

```rs
struct ViewParams {
  width
  height
  camera_view
  margin_left
  margin_right
  margin_top
  margin_bottom
  timeout
  cache_enabled
  device_pixel_ratio
  // ... anything else at the view level (not layer-specific)
}

struct Model {
  attributes
  instanced_attributes
}

struct ScatterplotLayer {
}

impl DrawToSvg for ScatterplotLayer {
   async fn draw(self, ) -> SvgNode {

   }
}

impl DrawToCanvas for ScatterplotLayer {
  async fn get_model(self, device, queue) {
     // use memoization
  }
  async fn draw(self, device, queue, encoder) {
    // create pass from encoder
    let model = self.get_model(device, queue);
    model.draw(pass)
    // finish render pass
  }
}

async fn render_svg(layers: Vec<Layer>, view_params: ViewParams) {

}

async fn render_canvas(layers: Vec<Layer>, view_params: ViewParams) {

}
```

TODO: how would composite, and then custom composite layer subclassses work? Can they somehow inherit the parent's DrawToCanvas/DrawToSvg trait implementations as their default implementations?
