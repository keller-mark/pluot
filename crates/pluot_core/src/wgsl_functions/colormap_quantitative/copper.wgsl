// Reference: https://github.com/vitessce/vitessce/blob/main/packages/gl/src/glsl/index.js
fn copper(x_6: f32) -> vec4<f32> {
  let e0 = 0.0;
  let v0 = vec4<f32>(0.0,0.0,0.0,1.0);
  let e1 = 0.804;
  let v1 = vec4<f32>(1.0,0.6274509803921569,0.4,1.0);
  let e2 = 1.0;
  let v2 = vec4<f32>(1.0,0.7803921568627451,0.4980392156862745,1.0);
  let a0 = smoothstep(e0,e1,x_6);
  let a1 = smoothstep(e1,e2,x_6);
  return max(mix(v0,v1,a0)*step(e0,x_6)*step(x_6,e1),mix(v1,v2,a1)*step(e1,x_6)*step(x_6,e2)
  );
}
