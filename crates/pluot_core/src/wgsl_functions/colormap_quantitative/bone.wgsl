// Reference: https://github.com/vitessce/vitessce/blob/main/packages/gl/src/glsl/index.js
fn bone(x_11: f32) -> vec4<f32> {
  let e0 = 0.0;
  let v0 = vec4<f32>(0.0,0.0,0.0,1.0);
  let e1 = 0.376;
  let v1 = vec4<f32>(0.32941176470588235,0.32941176470588235,0.4549019607843137,1.0);
  let e2 = 0.753;
  let v2 = vec4<f32>(0.6627450980392157,0.7843137254901961,0.7843137254901961,1.0);
  let e3 = 1.0;
  let v3 = vec4<f32>(1.0,1.0,1.0,1.0);
  let a0 = smoothstep(e0,e1,x_11);
  let a1 = smoothstep(e1,e2,x_11);
  let a2 = smoothstep(e2,e3,x_11);
  return max(mix(v0,v1,a0)*step(e0,x_11)*step(x_11,e1),
    max(mix(v1,v2,a1)*step(e1,x_11)*step(x_11,e2),mix(v2,v3,a2)*step(e2,x_11)*step(x_11,e3)
  ));
}
