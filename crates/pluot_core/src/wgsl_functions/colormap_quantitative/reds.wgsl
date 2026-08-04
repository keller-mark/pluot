// Reference: https://github.com/d3/d3-scale-chromatic/blob/main/src/sequential-single/Reds.js
fn reds(x_1: f32) -> vec4<f32> {
  let e0 = 0.0;
  let v0 = vec4<f32>(1.0,0.9607843137254902,0.9411764705882353,1.0);
  let e1 = 0.13;
  let v1 = vec4<f32>(0.996078431372549,0.8784313725490196,0.8235294117647058,1.0);
  let e2 = 0.25;
  let v2 = vec4<f32>(0.9882352941176471,0.7333333333333333,0.6313725490196078,1.0);
  let e3 = 0.38;
  let v3 = vec4<f32>(0.9882352941176471,0.5725490196078431,0.4470588235294118,1.0);
  let e4 = 0.5;
  let v4 = vec4<f32>(0.984313725490196,0.41568627450980394,0.2901960784313726,1.0);
  let e5 = 0.63;
  let v5 = vec4<f32>(0.9372549019607843,0.23137254901960785,0.17254901960784313,1.0);
  let e6 = 0.75;
  let v6 = vec4<f32>(0.796078431372549,0.09411764705882353,0.11372549019607843,1.0);
  let e7 = 0.88;
  let v7 = vec4<f32>(0.6470588235294118,0.058823529411764705,0.08235294117647059,1.0);
  let e8 = 1.0;
  let v8 = vec4<f32>(0.403921568627451,0.0,0.050980392156862744,1.0);
  let a0 = smoothstep(e0,e1,x_1);
  let a1 = smoothstep(e1,e2,x_1);
  let a2 = smoothstep(e2,e3,x_1);
  let a3 = smoothstep(e3,e4,x_1);
  let a4 = smoothstep(e4,e5,x_1);
  let a5 = smoothstep(e5,e6,x_1);
  let a6 = smoothstep(e6,e7,x_1);
  let a7 = smoothstep(e7,e8,x_1);
  return max(mix(v0,v1,a0)*step(e0,x_1)*step(x_1,e1),
    max(mix(v1,v2,a1)*step(e1,x_1)*step(x_1,e2),
    max(mix(v2,v3,a2)*step(e2,x_1)*step(x_1,e3),
    max(mix(v3,v4,a3)*step(e3,x_1)*step(x_1,e4),
    max(mix(v4,v5,a4)*step(e4,x_1)*step(x_1,e5),
    max(mix(v5,v6,a5)*step(e5,x_1)*step(x_1,e6),
    max(mix(v6,v7,a6)*step(e6,x_1)*step(x_1,e7),
    mix(v7,v8,a7)*step(e7,x_1)*step(x_1,e8))))))));
}
