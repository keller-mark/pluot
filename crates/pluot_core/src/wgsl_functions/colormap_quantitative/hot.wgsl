// Reference: https://github.com/vitessce/vitessce/blob/main/packages/gl/src/glsl/index.js

fn hot(x_0: f32) -> vec4<f32> {
  let e0 = 0.0;
  let v0 = vec4<f32>(0.0,0.0,0.0,1.0);
  let e1 = 0.3;
  let v1 = vec4<f32>(0.9019607843137255,0.0,0.0,1.0);
  let e2 = 0.6;
  let v2 = vec4<f32>(1.0,0.8235294117647058,0.0,1.0);
  let e3 = 1.0;
  let v3 = vec4<f32>(1.0,1.0,1.0,1.0);
  let a0 = smoothstep(e0,e1,x_0);
  let a1 = smoothstep(e1,e2,x_0);
  let a2 = smoothstep(e2,e3,x_0);
  return max(mix(v0,v1,a0)*step(e0,x_0)*step(x_0,e1),
    max(mix(v1,v2,a1)*step(e1,x_0)*step(x_0,e2),mix(v2,v3,a2)*step(e2,x_0)*step(x_0,e3)
  ));
}
