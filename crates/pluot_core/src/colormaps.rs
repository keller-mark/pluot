//! Rust-native equivalents of the WGSL colormap functions in
//! `wgsl_functions/colormaps/`, for sampling colormaps on the CPU (e.g. for
//! legends or the `lacks_gpu` render path) without needing a GPU context.
//!
//! Each function here is a direct port of its WGSL counterpart, keeping the
//! same piecewise `smoothstep`/`mix` structure so the two stay in sync.
//!
//! Reference: <https://github.com/vitessce/vitessce/blob/main/packages/gl/src/glsl/index.js>

use crate::render_traits::QuantitativeColormap;

type Rgba = [f32; 4];

fn step(edge: f32, x: f32) -> f32 {
    if x < edge { 0.0 } else { 1.0 }
}

fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn mix(a: Rgba, b: Rgba, t: f32) -> Rgba {
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
        a[3] + (b[3] - a[3]) * t,
    ]
}

fn scale(a: Rgba, s: f32) -> Rgba {
    [a[0] * s, a[1] * s, a[2] * s, a[3] * s]
}

fn max4(a: Rgba, b: Rgba) -> Rgba {
    [a[0].max(b[0]), a[1].max(b[1]), a[2].max(b[2]), a[3].max(b[3])]
}

/// One `mix(v_lo, v_hi, smoothstep(e_lo, e_hi, x)) * step(e_lo, x) * step(x, e_hi)` segment.
fn segment(e_lo: f32, v_lo: Rgba, e_hi: f32, v_hi: Rgba, x: f32) -> Rgba {
    let a = smoothstep(e_lo, e_hi, x);
    scale(mix(v_lo, v_hi, a), step(e_lo, x) * step(x, e_hi))
}

pub fn autumn(x: f32) -> Rgba {
    segment(0.0, [1.0, 0.0, 0.0, 1.0], 1.0, [1.0, 1.0, 0.0, 1.0], x)
}

pub fn bone(x: f32) -> Rgba {
    let v0 = [0.0, 0.0, 0.0, 1.0];
    let v1 = [0.32941176470588235, 0.32941176470588235, 0.4549019607843137, 1.0];
    let v2 = [0.6627450980392157, 0.7843137254901961, 0.7843137254901961, 1.0];
    let v3 = [1.0, 1.0, 1.0, 1.0];
    max4(
        segment(0.0, v0, 0.376, v1, x),
        max4(segment(0.376, v1, 0.753, v2, x), segment(0.753, v2, 1.0, v3, x)),
    )
}

pub fn cool(x: f32) -> Rgba {
    let v0 = [0.49019607843137253, 0.0, 0.7019607843137254, 1.0];
    let v1 = [0.4549019607843137, 0.0, 0.8549019607843137, 1.0];
    let v2 = [0.3843137254901961, 0.2901960784313726, 0.9294117647058824, 1.0];
    let v3 = [0.26666666666666666, 0.5725490196078431, 0.9058823529411765, 1.0];
    let v4 = [0.0, 0.8, 0.7725490196078432, 1.0];
    let v5 = [0.0, 0.9686274509803922, 0.5725490196078431, 1.0];
    let v6 = [0.0, 1.0, 0.34509803921568627, 1.0];
    let v7 = [0.1568627450980392, 1.0, 0.03137254901960784, 1.0];
    let v8 = [0.5764705882352941, 1.0, 0.0, 1.0];
    max4(
        segment(0.0, v0, 0.13, v1, x),
        max4(
            segment(0.13, v1, 0.25, v2, x),
            max4(
                segment(0.25, v2, 0.38, v3, x),
                max4(
                    segment(0.38, v3, 0.5, v4, x),
                    max4(
                        segment(0.5, v4, 0.63, v5, x),
                        max4(
                            segment(0.63, v5, 0.75, v6, x),
                            max4(segment(0.75, v6, 0.88, v7, x), segment(0.88, v7, 1.0, v8, x)),
                        ),
                    ),
                ),
            ),
        ),
    )
}

pub fn copper(x: f32) -> Rgba {
    let v0 = [0.0, 0.0, 0.0, 1.0];
    let v1 = [1.0, 0.6274509803921569, 0.4, 1.0];
    let v2 = [1.0, 0.7803921568627451, 0.4980392156862745, 1.0];
    max4(segment(0.0, v0, 0.804, v1, x), segment(0.804, v1, 1.0, v2, x))
}

pub fn density(x: f32) -> Rgba {
    let v0 = [0.21176470588235294, 0.054901960784313725, 0.1411764705882353, 1.0];
    let v1 = [0.34901960784313724, 0.09019607843137255, 0.3137254901960784, 1.0];
    let v2 = [0.43137254901960786, 0.17647058823529413, 0.5176470588235295, 1.0];
    let v3 = [0.47058823529411764, 0.30196078431372547, 0.6980392156862745, 1.0];
    let v4 = [0.47058823529411764, 0.44313725490196076, 0.8352941176470589, 1.0];
    let v5 = [0.45098039215686275, 0.592156862745098, 0.8941176470588236, 1.0];
    let v6 = [0.5254901960784314, 0.7254901960784313, 0.8901960784313725, 1.0];
    let v7 = [0.6941176470588235, 0.8392156862745098, 0.8901960784313725, 1.0];
    let v8 = [0.9019607843137255, 0.9450980392156862, 0.9450980392156862, 1.0];
    max4(
        segment(0.0, v0, 0.13, v1, x),
        max4(
            segment(0.13, v1, 0.25, v2, x),
            max4(
                segment(0.25, v2, 0.38, v3, x),
                max4(
                    segment(0.38, v3, 0.5, v4, x),
                    max4(
                        segment(0.5, v4, 0.63, v5, x),
                        max4(
                            segment(0.63, v5, 0.75, v6, x),
                            max4(segment(0.75, v6, 0.88, v7, x), segment(0.88, v7, 1.0, v8, x)),
                        ),
                    ),
                ),
            ),
        ),
    )
}

pub fn greys(x: f32) -> Rgba {
    segment(0.0, [0.0, 0.0, 0.0, 1.0], 1.0, [1.0, 1.0, 1.0, 1.0], x)
}

pub fn hot(x: f32) -> Rgba {
    let v0 = [0.0, 0.0, 0.0, 1.0];
    let v1 = [0.9019607843137255, 0.0, 0.0, 1.0];
    let v2 = [1.0, 0.8235294117647058, 0.0, 1.0];
    let v3 = [1.0, 1.0, 1.0, 1.0];
    max4(
        segment(0.0, v0, 0.3, v1, x),
        max4(segment(0.3, v1, 0.6, v2, x), segment(0.6, v2, 1.0, v3, x)),
    )
}

pub fn inferno(x: f32) -> Rgba {
    let v0 = [0.0, 0.0, 0.01568627450980392, 1.0];
    let v1 = [0.12156862745098039, 0.047058823529411764, 0.2823529411764706, 1.0];
    let v2 = [0.3333333333333333, 0.058823529411764705, 0.42745098039215684, 1.0];
    let v3 = [0.5333333333333333, 0.13333333333333333, 0.41568627450980394, 1.0];
    let v4 = [0.7294117647058823, 0.21176470588235294, 0.3333333333333333, 1.0];
    let v5 = [0.8901960784313725, 0.34901960784313724, 0.2, 1.0];
    let v6 = [0.9764705882352941, 0.5490196078431373, 0.0392156862745098, 1.0];
    let v7 = [0.9764705882352941, 0.788235294117647, 0.19607843137254902, 1.0];
    let v8 = [0.9882352941176471, 1.0, 0.6431372549019608, 1.0];
    max4(
        segment(0.0, v0, 0.13, v1, x),
        max4(
            segment(0.13, v1, 0.25, v2, x),
            max4(
                segment(0.25, v2, 0.38, v3, x),
                max4(
                    segment(0.38, v3, 0.5, v4, x),
                    max4(
                        segment(0.5, v4, 0.63, v5, x),
                        max4(
                            segment(0.63, v5, 0.75, v6, x),
                            max4(segment(0.75, v6, 0.88, v7, x), segment(0.88, v7, 1.0, v8, x)),
                        ),
                    ),
                ),
            ),
        ),
    )
}

pub fn jet(x: f32) -> Rgba {
    let v0 = [0.0, 0.0, 0.5137254901960784, 1.0];
    let v1 = [0.0, 0.23529411764705882, 0.6666666666666666, 1.0];
    let v2 = [0.0196078431372549, 1.0, 1.0, 1.0];
    let v3 = [1.0, 1.0, 0.0, 1.0];
    let v4 = [0.9803921568627451, 0.0, 0.0, 1.0];
    let v5 = [0.5019607843137255, 0.0, 0.0, 1.0];
    max4(
        segment(0.0, v0, 0.125, v1, x),
        max4(
            segment(0.125, v1, 0.375, v2, x),
            max4(
                segment(0.375, v2, 0.625, v3, x),
                max4(segment(0.625, v3, 0.875, v4, x), segment(0.875, v4, 1.0, v5, x)),
            ),
        ),
    )
}

pub fn magma(x: f32) -> Rgba {
    let v0 = [0.0, 0.0, 0.01568627450980392, 1.0];
    let v1 = [0.10980392156862745, 0.06274509803921569, 0.26666666666666666, 1.0];
    let v2 = [0.30980392156862746, 0.07058823529411765, 0.4823529411764706, 1.0];
    let v3 = [0.5058823529411764, 0.1450980392156863, 0.5058823529411764, 1.0];
    let v4 = [0.7098039215686275, 0.21176470588235294, 0.47843137254901963, 1.0];
    let v5 = [0.8980392156862745, 0.3137254901960784, 0.39215686274509803, 1.0];
    let v6 = [0.984313725490196, 0.5294117647058824, 0.3803921568627451, 1.0];
    let v7 = [0.996078431372549, 0.7607843137254902, 0.5294117647058824, 1.0];
    let v8 = [0.9882352941176471, 0.9921568627450981, 0.7490196078431373, 1.0];
    max4(
        segment(0.0, v0, 0.13, v1, x),
        max4(
            segment(0.13, v1, 0.25, v2, x),
            max4(
                segment(0.25, v2, 0.38, v3, x),
                max4(
                    segment(0.38, v3, 0.5, v4, x),
                    max4(
                        segment(0.5, v4, 0.63, v5, x),
                        max4(
                            segment(0.63, v5, 0.75, v6, x),
                            max4(segment(0.75, v6, 0.88, v7, x), segment(0.88, v7, 1.0, v8, x)),
                        ),
                    ),
                ),
            ),
        ),
    )
}

pub fn plasma(x: f32) -> Rgba {
    let v0 = [0.050980392156862744, 0.03137254901960784, 0.5294117647058824, 1.0];
    let v1 = [0.29411764705882354, 0.011764705882352941, 0.6313725490196078, 1.0];
    let v2 = [0.49019607843137253, 0.011764705882352941, 0.6588235294117647, 1.0];
    let v3 = [0.6588235294117647, 0.13333333333333333, 0.5882352941176471, 1.0];
    let v4 = [0.796078431372549, 0.27450980392156865, 0.4745098039215686, 1.0];
    let v5 = [0.8980392156862745, 0.4196078431372549, 0.36470588235294116, 1.0];
    let v6 = [0.9725490196078431, 0.5803921568627451, 0.2549019607843137, 1.0];
    let v7 = [0.9921568627450981, 0.7647058823529411, 0.1568627450980392, 1.0];
    let v8 = [0.9411764705882353, 0.9764705882352941, 0.12941176470588237, 1.0];
    max4(
        segment(0.0, v0, 0.13, v1, x),
        max4(
            segment(0.13, v1, 0.25, v2, x),
            max4(
                segment(0.25, v2, 0.38, v3, x),
                max4(
                    segment(0.38, v3, 0.5, v4, x),
                    max4(
                        segment(0.5, v4, 0.63, v5, x),
                        max4(
                            segment(0.63, v5, 0.75, v6, x),
                            max4(segment(0.75, v6, 0.88, v7, x), segment(0.88, v7, 1.0, v8, x)),
                        ),
                    ),
                ),
            ),
        ),
    )
}

pub fn spring(x: f32) -> Rgba {
    segment(0.0, [1.0, 0.0, 1.0, 1.0], 1.0, [1.0, 1.0, 0.0, 1.0], x)
}

pub fn summer(x: f32) -> Rgba {
    segment(0.0, [0.0, 0.5019607843137255, 0.4, 1.0], 1.0, [1.0, 1.0, 0.4, 1.0], x)
}

pub fn viridis(x: f32) -> Rgba {
    let v0 = [0.26666666666666666, 0.00392156862745098, 0.32941176470588235, 1.0];
    let v1 = [0.2784313725490196, 0.17254901960784313, 0.47843137254901963, 1.0];
    let v2 = [0.23137254901960785, 0.3176470588235294, 0.5450980392156862, 1.0];
    let v3 = [0.17254901960784313, 0.44313725490196076, 0.5568627450980392, 1.0];
    let v4 = [0.12941176470588237, 0.5647058823529412, 0.5529411764705883, 1.0];
    let v5 = [0.15294117647058825, 0.6784313725490196, 0.5058823529411764, 1.0];
    let v6 = [0.3607843137254902, 0.7843137254901961, 0.38823529411764707, 1.0];
    let v7 = [0.6666666666666666, 0.8627450980392157, 0.19607843137254902, 1.0];
    let v8 = [0.9921568627450981, 0.9058823529411765, 0.1450980392156863, 1.0];
    max4(
        segment(0.0, v0, 0.13, v1, x),
        max4(
            segment(0.13, v1, 0.25, v2, x),
            max4(
                segment(0.25, v2, 0.38, v3, x),
                max4(
                    segment(0.38, v3, 0.5, v4, x),
                    max4(
                        segment(0.5, v4, 0.63, v5, x),
                        max4(
                            segment(0.63, v5, 0.75, v6, x),
                            max4(segment(0.75, v6, 0.88, v7, x), segment(0.88, v7, 1.0, v8, x)),
                        ),
                    ),
                ),
            ),
        ),
    )
}

pub fn winter(x: f32) -> Rgba {
    segment(0.0, [0.0, 0.0, 1.0, 1.0], 1.0, [0.0, 1.0, 0.5019607843137255, 1.0], x)
}

/// Sample a [`QuantitativeColormap`] at `x`, the CPU-side equivalent of calling
/// the matching WGSL function from `shader_modules::colormaps`.
pub fn sample(colormap: QuantitativeColormap, x: f32) -> Rgba {
    match colormap {
        QuantitativeColormap::Plasma => plasma(x),
        QuantitativeColormap::Viridis => viridis(x),
        QuantitativeColormap::Greys => greys(x),
        QuantitativeColormap::Magma => magma(x),
        QuantitativeColormap::Jet => jet(x),
        QuantitativeColormap::Bone => bone(x),
        QuantitativeColormap::Copper => copper(x),
        QuantitativeColormap::Density => density(x),
        QuantitativeColormap::Inferno => inferno(x),
        QuantitativeColormap::Cool => cool(x),
        QuantitativeColormap::Hot => hot(x),
        QuantitativeColormap::Spring => spring(x),
        QuantitativeColormap::Summer => summer(x),
        QuantitativeColormap::Autumn => autumn(x),
        QuantitativeColormap::Winter => winter(x),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn endpoints_match_control_points() {
        assert_eq!(autumn(0.0), [1.0, 0.0, 0.0, 1.0]);
        assert_eq!(autumn(1.0), [1.0, 1.0, 0.0, 1.0]);
        assert_eq!(viridis(0.0), [0.26666666666666666, 0.00392156862745098, 0.32941176470588235, 1.0]);
        assert_eq!(viridis(1.0), [0.9921568627450981, 0.9058823529411765, 0.1450980392156863, 1.0]);
    }

    #[test]
    fn out_of_range_is_zeroed() {
        assert_eq!(hot(-0.5), [0.0, 0.0, 0.0, 0.0]);
        assert_eq!(hot(1.5), [0.0, 0.0, 0.0, 0.0]);
    }

    #[test]
    fn interior_control_points_are_hit_exactly() {
        let v1 = [0.9019607843137255, 0.0, 0.0, 1.0];
        assert_eq!(hot(0.3), v1);
    }

    #[test]
    fn sample_dispatches_to_matching_function() {
        assert_eq!(sample(QuantitativeColormap::Jet, 0.5), jet(0.5));
        assert_eq!(sample(QuantitativeColormap::Winter, 0.25), winter(0.25));
    }
}
