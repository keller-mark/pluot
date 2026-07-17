// Reference: https://github.com/vitessce/vitessce/blob/main/packages/gl/src/glsl/index.js
fn cool(x_2: f32) -> vec4<f32> {
  let e0 = 0.0;
  let v0 = vec4<f32>(0.49019607843137253,0.0,0.7019607843137254,1.0);
  let e1 = 0.13;
  let v1 = vec4<f32>(0.4549019607843137,0.0,0.8549019607843137,1.0);
  let e2 = 0.25;
  let v2 = vec4<f32>(0.3843137254901961,0.2901960784313726,0.9294117647058824,1.0);
  let e3 = 0.38;
  let v3 = vec4<f32>(0.26666666666666666,0.5725490196078431,0.9058823529411765,1.0);
  let e4 = 0.5;
  let v4 = vec4<f32>(0.0,0.8,0.7725490196078432,1.0);
  let e5 = 0.63;
  let v5 = vec4<f32>(0.0,0.9686274509803922,0.5725490196078431,1.0);
  let e6 = 0.75;
  let v6 = vec4<f32>(0.0,1.0,0.34509803921568627,1.0);
  let e7 = 0.88;
  let v7 = vec4<f32>(0.1568627450980392,1.0,0.03137254901960784,1.0);
  let e8 = 1.0;
  let v8 = vec4<f32>(0.5764705882352941,1.0,0.0,1.0);
  let a0 = smoothstep(e0,e1,x_2);
  let a1 = smoothstep(e1,e2,x_2);
  let a2 = smoothstep(e2,e3,x_2);
  let a3 = smoothstep(e3,e4,x_2);
  let a4 = smoothstep(e4,e5,x_2);
  let a5 = smoothstep(e5,e6,x_2);
  let a6 = smoothstep(e6,e7,x_2);
  let a7 = smoothstep(e7,e8,x_2);
  return max(mix(v0,v1,a0)*step(e0,x_2)*step(x_2,e1),
    max(mix(v1,v2,a1)*step(e1,x_2)*step(x_2,e2),
    max(mix(v2,v3,a2)*step(e2,x_2)*step(x_2,e3),
    max(mix(v3,v4,a3)*step(e3,x_2)*step(x_2,e4),
    max(mix(v4,v5,a4)*step(e4,x_2)*step(x_2,e5),
    max(mix(v5,v6,a5)*step(e5,x_2)*step(x_2,e6),
    max(mix(v6,v7,a6)*step(e6,x_2)*step(x_2,e7),mix(v7,v8,a7)*step(e7,x_2)*step(x_2,e8)
  )))))));
}
