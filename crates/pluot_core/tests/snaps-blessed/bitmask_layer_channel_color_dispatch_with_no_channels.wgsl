// The switch case list below is substituted in with one case per
// channel, which calls that channel's generated color getter.
// Depends on the getter functions (see `get_channel_color`) also being injected.
fn get_channel_fill_color(channel_index: u32, label_index: u32) -> vec3<f32> {
    switch (channel_index) {
        
        default: { return vec3<f32>(0.0, 0.0, 0.0); }
    }
}
