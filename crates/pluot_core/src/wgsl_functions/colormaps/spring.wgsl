// Reference: https://github.com/vitessce/vitessce/blob/main/packages/gl/src/glsl/index.js

fn spring(x_14: f32) -> vec4<f32> {
  let e0 = 0.0;
  let v0 = vec4<f32>(1.0,0.0,1.0,1.0);
  let e1 = 1.0;
  let v1 = vec4<f32>(1.0,1.0,0.0,1.0);
  let a0 = smoothstep(e0,e1,x_14);
  return mix(v0,v1,a0)*step(e0,x_14)*step(x_14,e1);
}
