Code inspired by DeckGL layer/view concepts, as well as [deck-to-svg](https://github.com/keller-mark/deck-to-svg).

Note: [At first](https://github.com/keller-mark/pluot/pull/107), I started to port the LumaGL `Model` implementation from deck.gl-native, but then backtracked to `Layer.draw` calls that directly set up the WebGPU buffers, bind groups, and render pipeline.
