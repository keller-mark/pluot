// Reference: https://github.com/vitessce/vitessce/blob/main/packages/gl/src/glsl/index.js

fn density(x_5: f32) -> vec4<f32> {
  let e0 = 0.0;
  let v0 = vec4<f32>(0.21176470588235294,0.054901960784313725,0.1411764705882353,1.0);
  let e1 = 0.13;
  let v1 = vec4<f32>(0.34901960784313724,0.09019607843137255,0.3137254901960784,1.0);
  let e2 = 0.25;
  let v2 = vec4<f32>(0.43137254901960786,0.17647058823529413,0.5176470588235295,1.0);
  let e3 = 0.38;
  let v3 = vec4<f32>(0.47058823529411764,0.30196078431372547,0.6980392156862745,1.0);
  let e4 = 0.5;
  let v4 = vec4<f32>(0.47058823529411764,0.44313725490196076,0.8352941176470589,1.0);
  let e5 = 0.63;
  let v5 = vec4<f32>(0.45098039215686275,0.592156862745098,0.8941176470588236,1.0);
  let e6 = 0.75;
  let v6 = vec4<f32>(0.5254901960784314,0.7254901960784313,0.8901960784313725,1.0);
  let e7 = 0.88;
  let v7 = vec4<f32>(0.6941176470588235,0.8392156862745098,0.8901960784313725,1.0);
  let e8 = 1.0;
  let v8 = vec4<f32>(0.9019607843137255,0.9450980392156862,0.9450980392156862,1.0);
  let a0 = smoothstep(e0,e1,x_5);
  let a1 = smoothstep(e1,e2,x_5);
  let a2 = smoothstep(e2,e3,x_5);
  let a3 = smoothstep(e3,e4,x_5);
  let a4 = smoothstep(e4,e5,x_5);
  let a5 = smoothstep(e5,e6,x_5);
  let a6 = smoothstep(e6,e7,x_5);
  let a7 = smoothstep(e7,e8,x_5);
  return max(mix(v0,v1,a0)*step(e0,x_5)*step(x_5,e1),
    max(mix(v1,v2,a1)*step(e1,x_5)*step(x_5,e2),
    max(mix(v2,v3,a2)*step(e2,x_5)*step(x_5,e3),
    max(mix(v3,v4,a3)*step(e3,x_5)*step(x_5,e4),
    max(mix(v4,v5,a4)*step(e4,x_5)*step(x_5,e5),
    max(mix(v5,v6,a5)*step(e5,x_5)*step(x_5,e6),
    max(mix(v6,v7,a6)*step(e6,x_5)*step(x_5,e7),mix(v7,v8,a7)*step(e7,x_5)*step(x_5,e8)
  )))))));
}
