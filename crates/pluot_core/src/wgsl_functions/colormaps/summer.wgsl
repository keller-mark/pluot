// Reference: https://github.com/vitessce/vitessce/blob/main/packages/gl/src/glsl/index.js

fn summer(x_9: f32) -> vec4<f32> {
  let e0 = 0.0;
  let v0 = vec4<f32>(0.0,0.5019607843137255,0.4,1.0);
  let e1 = 1.0;
  let v1 = vec4<f32>(1.0,1.0,0.4,1.0);
  let a0 = smoothstep(e0,e1,x_9);
  return mix(v0,v1,a0)*step(e0,x_9)*step(x_9,e1);
}
