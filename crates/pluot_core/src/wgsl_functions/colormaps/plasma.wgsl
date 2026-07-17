// Reference: https://github.com/vitessce/vitessce/blob/main/packages/gl/src/glsl/index.js
fn plasma(x_4: f32) -> vec4<f32> {
  let e0 = 0.0;
  let v0 = vec4<f32>(0.050980392156862744,0.03137254901960784,0.5294117647058824,1.0);
  let e1 = 0.13;
  let v1 = vec4<f32>(0.29411764705882354,0.011764705882352941,0.6313725490196078,1.0);
  let e2 = 0.25;
  let v2 = vec4<f32>(0.49019607843137253,0.011764705882352941,0.6588235294117647,1.0);
  let e3 = 0.38;
  let v3 = vec4<f32>(0.6588235294117647,0.13333333333333333,0.5882352941176471,1.0);
  let e4 = 0.5;
  let v4 = vec4<f32>(0.796078431372549,0.27450980392156865,0.4745098039215686,1.0);
  let e5 = 0.63;
  let v5 = vec4<f32>(0.8980392156862745,0.4196078431372549,0.36470588235294116,1.0);
  let e6 = 0.75;
  let v6 = vec4<f32>(0.9725490196078431,0.5803921568627451,0.2549019607843137,1.0);
  let e7 = 0.88;
  let v7 = vec4<f32>(0.9921568627450981,0.7647058823529411,0.1568627450980392,1.0);
  let e8 = 1.0;
  let v8 = vec4<f32>(0.9411764705882353,0.9764705882352941,0.12941176470588237,1.0);
  let a0 = smoothstep(e0,e1,x_4);
  let a1 = smoothstep(e1,e2,x_4);
  let a2 = smoothstep(e2,e3,x_4);
  let a3 = smoothstep(e3,e4,x_4);
  let a4 = smoothstep(e4,e5,x_4);
  let a5 = smoothstep(e5,e6,x_4);
  let a6 = smoothstep(e6,e7,x_4);
  let a7 = smoothstep(e7,e8,x_4);
  return max(mix(v0,v1,a0)*step(e0,x_4)*step(x_4,e1),
    max(mix(v1,v2,a1)*step(e1,x_4)*step(x_4,e2),
    max(mix(v2,v3,a2)*step(e2,x_4)*step(x_4,e3),
    max(mix(v3,v4,a3)*step(e3,x_4)*step(x_4,e4),
    max(mix(v4,v5,a4)*step(e4,x_4)*step(x_4,e5),
    max(mix(v5,v6,a5)*step(e5,x_4)*step(x_4,e6),
    max(mix(v6,v7,a6)*step(e6,x_4)*step(x_4,e7),mix(v7,v8,a7)*step(e7,x_4)*step(x_4,e8)
  )))))));
}
