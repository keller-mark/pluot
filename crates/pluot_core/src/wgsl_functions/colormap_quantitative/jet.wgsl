// Reference: https://github.com/vitessce/vitessce/blob/main/packages/gl/src/glsl/index.js

fn jet(x_8: f32) -> vec4<f32> {
  let e0 = 0.0;
  let v0 = vec4<f32>(0.0,0.0,0.5137254901960784,1.0);
  let e1 = 0.125;
  let v1 = vec4<f32>(0.0,0.23529411764705882,0.6666666666666666,1.0);
  let e2 = 0.375;
  let v2 = vec4<f32>(0.0196078431372549,1.0,1.0,1.0);
  let e3 = 0.625;
  let v3 = vec4<f32>(1.0,1.0,0.0,1.0);
  let e4 = 0.875;
  let v4 = vec4<f32>(0.9803921568627451,0.0,0.0,1.0);
  let e5 = 1.0;
  let v5 = vec4<f32>(0.5019607843137255,0.0,0.0,1.0);
  let a0 = smoothstep(e0,e1,x_8);
  let a1 = smoothstep(e1,e2,x_8);
  let a2 = smoothstep(e2,e3,x_8);
  let a3 = smoothstep(e3,e4,x_8);
  let a4 = smoothstep(e4,e5,x_8);
  return max(mix(v0,v1,a0)*step(e0,x_8)*step(x_8,e1),
    max(mix(v1,v2,a1)*step(e1,x_8)*step(x_8,e2),
    max(mix(v2,v3,a2)*step(e2,x_8)*step(x_8,e3),
    max(mix(v3,v4,a3)*step(e3,x_8)*step(x_8,e4),mix(v4,v5,a4)*step(e4,x_8)*step(x_8,e5)
  ))));
}
