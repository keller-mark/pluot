// Reference: https://github.com/vitessce/vitessce/blob/main/packages/gl/src/glsl/index.js

fn viridis(x_1: f32) -> vec4<f32> {
  let e0 = 0.0;
  let v0 = vec4<f32>(0.26666666666666666,0.00392156862745098,0.32941176470588235,1.0);
  let e1 = 0.13;
  let v1 = vec4<f32>(0.2784313725490196,0.17254901960784313,0.47843137254901963,1.0);
  let e2 = 0.25;
  let v2 = vec4<f32>(0.23137254901960785,0.3176470588235294,0.5450980392156862,1.0);
  let e3 = 0.38;
  let v3 = vec4<f32>(0.17254901960784313,0.44313725490196076,0.5568627450980392,1.0);
  let e4 = 0.5;
  let v4 = vec4<f32>(0.12941176470588237,0.5647058823529412,0.5529411764705883,1.0);
  let e5 = 0.63;
  let v5 = vec4<f32>(0.15294117647058825,0.6784313725490196,0.5058823529411764,1.0);
  let e6 = 0.75;
  let v6 = vec4<f32>(0.3607843137254902,0.7843137254901961,0.38823529411764707,1.0);
  let e7 = 0.88;
  let v7 = vec4<f32>(0.6666666666666666,0.8627450980392157,0.19607843137254902,1.0);
  let e8 = 1.0;
  let v8 = vec4<f32>(0.9921568627450981,0.9058823529411765,0.1450980392156863,1.0);
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
    max(mix(v6,v7,a6)*step(e6,x_1)*step(x_1,e7),mix(v7,v8,a7)*step(e7,x_1)*step(x_1,e8)
  )))))));
}
