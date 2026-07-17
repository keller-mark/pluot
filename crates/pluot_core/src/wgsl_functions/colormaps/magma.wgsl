// Reference: https://github.com/vitessce/vitessce/blob/main/packages/gl/src/glsl/index.js
fn magma(x_7: f32) -> vec4<f32> {
  let e0 = 0.0;
  let v0 = vec4<f32>(0.0,0.0,0.01568627450980392,1.0);
  let e1 = 0.13;
  let v1 = vec4<f32>(0.10980392156862745,0.06274509803921569,0.26666666666666666,1.0);
  let e2 = 0.25;
  let v2 = vec4<f32>(0.30980392156862746,0.07058823529411765,0.4823529411764706,1.0);
  let e3 = 0.38;
  let v3 = vec4<f32>(0.5058823529411764,0.1450980392156863,0.5058823529411764,1.0);
  let e4 = 0.5;
  let v4 = vec4<f32>(0.7098039215686275,0.21176470588235294,0.47843137254901963,1.0);
  let e5 = 0.63;
  let v5 = vec4<f32>(0.8980392156862745,0.3137254901960784,0.39215686274509803,1.0);
  let e6 = 0.75;
  let v6 = vec4<f32>(0.984313725490196,0.5294117647058824,0.3803921568627451,1.0);
  let e7 = 0.88;
  let v7 = vec4<f32>(0.996078431372549,0.7607843137254902,0.5294117647058824,1.0);
  let e8 = 1.0;
  let v8 = vec4<f32>(0.9882352941176471,0.9921568627450981,0.7490196078431373,1.0);
  let a0 = smoothstep(e0,e1,x_7);
  let a1 = smoothstep(e1,e2,x_7);
  let a2 = smoothstep(e2,e3,x_7);
  let a3 = smoothstep(e3,e4,x_7);
  let a4 = smoothstep(e4,e5,x_7);
  let a5 = smoothstep(e5,e6,x_7);
  let a6 = smoothstep(e6,e7,x_7);
  let a7 = smoothstep(e7,e8,x_7);
  return max(mix(v0,v1,a0)*step(e0,x_7)*step(x_7,e1),
    max(mix(v1,v2,a1)*step(e1,x_7)*step(x_7,e2),
    max(mix(v2,v3,a2)*step(e2,x_7)*step(x_7,e3),
    max(mix(v3,v4,a3)*step(e3,x_7)*step(x_7,e4),
    max(mix(v4,v5,a4)*step(e4,x_7)*step(x_7,e5),
    max(mix(v5,v6,a5)*step(e5,x_7)*step(x_7,e6),
    max(mix(v6,v7,a6)*step(e6,x_7)*step(x_7,e7),mix(v7,v8,a7)*step(e7,x_7)*step(x_7,e8)
  )))))));
}
