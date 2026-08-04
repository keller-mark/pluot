// Reference: https://github.com/d3/d3-scale-chromatic/blob/main/src/sequential-single/Blues.js
fn blues(x_1: f32) -> vec4<f32> {
  let e0 = 0.0;
  let v0 = vec4<f32>(0.9686274509803922,0.984313725490196,1.0,1.0);
  let e1 = 0.13;
  let v1 = vec4<f32>(0.8705882352941177,0.9215686274509803,0.9686274509803922,1.0);
  let e2 = 0.25;
  let v2 = vec4<f32>(0.7764705882352941,0.8588235294117647,0.9372549019607843,1.0);
  let e3 = 0.38;
  let v3 = vec4<f32>(0.6196078431372549,0.792156862745098,0.8823529411764706,1.0);
  let e4 = 0.5;
  let v4 = vec4<f32>(0.4196078431372549,0.6823529411764706,0.8392156862745098,1.0);
  let e5 = 0.63;
  let v5 = vec4<f32>(0.25882352941176473,0.5725490196078431,0.7764705882352941,1.0);
  let e6 = 0.75;
  let v6 = vec4<f32>(0.12941176470588237,0.44313725490196076,0.7098039215686275,1.0);
  let e7 = 0.88;
  let v7 = vec4<f32>(0.03137254901960784,0.3176470588235294,0.611764705882353,1.0);
  let e8 = 1.0;
  let v8 = vec4<f32>(0.03137254901960784,0.18823529411764706,0.4196078431372549,1.0);
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
