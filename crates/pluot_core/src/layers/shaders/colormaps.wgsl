// Reference: https://github.com/vitessce/vitessce/blob/main/packages/gl/src/glsl/index.js
//
//


fn plasma(x_4: f32) -> vec4<f32> {
  let e0 = 0.0;
  let v0 = vec4<f32>(0.050980392156862744,0.03137254901960784,0.5294117647058824,1.0);
  let e1 = 0.13;
  let v1 = vec4<f32>(0.29411764705882354,0.011764705882352941,0.6313725490196078,1.0);
  let e2 = 0.25;
  let v2 = vec4<f32>(0.49019607843137253,0.011764705882352941,0.6588235294117647,1.0);
  let e3 = 0.38;
  let v3 = vec4<f32>(0.6588235294117647,0.13333333333333333,0.5882352941176471,1.0);
  let e4 = 0.5;
  let v4 = vec4<f32>(0.796078431372549,0.27450980392156865,0.4745098039215686,1.0);
  let e5 = 0.63;
  let v5 = vec4<f32>(0.8980392156862745,0.4196078431372549,0.36470588235294116,1.0);
  let e6 = 0.75;
  let v6 = vec4<f32>(0.9725490196078431,0.5803921568627451,0.2549019607843137,1.0);
  let e7 = 0.88;
  let v7 = vec4<f32>(0.9921568627450981,0.7647058823529411,0.1568627450980392,1.0);
  let e8 = 1.0;
  let v8 = vec4<f32>(0.9411764705882353,0.9764705882352941,0.12941176470588237,1.0);
  let a0 = smoothstep(e0,e1,x_4);
  let a1 = smoothstep(e1,e2,x_4);
  let a2 = smoothstep(e2,e3,x_4);
  let a3 = smoothstep(e3,e4,x_4);
  let a4 = smoothstep(e4,e5,x_4);
  let a5 = smoothstep(e5,e6,x_4);
  let a6 = smoothstep(e6,e7,x_4);
  let a7 = smoothstep(e7,e8,x_4);
  return max(mix(v0,v1,a0)*step(e0,x_4)*step(x_4,e1),
    max(mix(v1,v2,a1)*step(e1,x_4)*step(x_4,e2),
    max(mix(v2,v3,a2)*step(e2,x_4)*step(x_4,e3),
    max(mix(v3,v4,a3)*step(e3,x_4)*step(x_4,e4),
    max(mix(v4,v5,a4)*step(e4,x_4)*step(x_4,e5),
    max(mix(v5,v6,a5)*step(e5,x_4)*step(x_4,e6),
    max(mix(v6,v7,a6)*step(e6,x_4)*step(x_4,e7),mix(v7,v8,a7)*step(e7,x_4)*step(x_4,e8)
  )))))));
}

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

fn greys(x_10: f32) -> vec4<f32> {
  let e0 = 0.0;
  let v0 = vec4<f32>(0.0,0.0,0.0,1.0);
  let e1 = 1.0;
  let v1 = vec4<f32>(1.0,1.0,1.0,1.0);
  let a0 = smoothstep(e0,e1,x_10);
  return mix(v0,v1,a0)*step(e0,x_10)*step(x_10,e1);
}

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

fn inferno(x_3: f32) -> vec4<f32> {
  let e0 = 0.0;
  let v0 = vec4<f32>(0.0,0.0,0.01568627450980392,1.0);
  let e1 = 0.13;
  let v1 = vec4<f32>(0.12156862745098039,0.047058823529411764,0.2823529411764706,1.0);
  let e2 = 0.25;
  let v2 = vec4<f32>(0.3333333333333333,0.058823529411764705,0.42745098039215684,1.0);
  let e3 = 0.38;
  let v3 = vec4<f32>(0.5333333333333333,0.13333333333333333,0.41568627450980394,1.0);
  let e4 = 0.5;
  let v4 = vec4<f32>(0.7294117647058823,0.21176470588235294,0.3333333333333333,1.0);
  let e5 = 0.63;
  let v5 = vec4<f32>(0.8901960784313725,0.34901960784313724,0.2,1.0);
  let e6 = 0.75;
  let v6 = vec4<f32>(0.9764705882352941,0.5490196078431373,0.0392156862745098,1.0);
  let e7 = 0.88;
  let v7 = vec4<f32>(0.9764705882352941,0.788235294117647,0.19607843137254902,1.0);
  let e8 = 1.0;
  let v8 = vec4<f32>(0.9882352941176471,1.0,0.6431372549019608,1.0);
  let a0 = smoothstep(e0,e1,x_3);
  let a1 = smoothstep(e1,e2,x_3);
  let a2 = smoothstep(e2,e3,x_3);
  let a3 = smoothstep(e3,e4,x_3);
  let a4 = smoothstep(e4,e5,x_3);
  let a5 = smoothstep(e5,e6,x_3);
  let a6 = smoothstep(e6,e7,x_3);
  let a7 = smoothstep(e7,e8,x_3);
  return max(mix(v0,v1,a0)*step(e0,x_3)*step(x_3,e1),
    max(mix(v1,v2,a1)*step(e1,x_3)*step(x_3,e2),
    max(mix(v2,v3,a2)*step(e2,x_3)*step(x_3,e3),
    max(mix(v3,v4,a3)*step(e3,x_3)*step(x_3,e4),
    max(mix(v4,v5,a4)*step(e4,x_3)*step(x_3,e5),
    max(mix(v5,v6,a5)*step(e5,x_3)*step(x_3,e6),
    max(mix(v6,v7,a6)*step(e6,x_3)*step(x_3,e7),mix(v7,v8,a7)*step(e7,x_3)*step(x_3,e8)
  )))))));
}

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

fn spring(x_14: f32) -> vec4<f32> {
  let e0 = 0.0;
  let v0 = vec4<f32>(1.0,0.0,1.0,1.0);
  let e1 = 1.0;
  let v1 = vec4<f32>(1.0,1.0,0.0,1.0);
  let a0 = smoothstep(e0,e1,x_14);
  return mix(v0,v1,a0)*step(e0,x_14)*step(x_14,e1);
}

fn summer(x_9: f32) -> vec4<f32> {
  let e0 = 0.0;
  let v0 = vec4<f32>(0.0,0.5019607843137255,0.4,1.0);
  let e1 = 1.0;
  let v1 = vec4<f32>(1.0,1.0,0.4,1.0);
  let a0 = smoothstep(e0,e1,x_9);
  return mix(v0,v1,a0)*step(e0,x_9)*step(x_9,e1);
}

fn autumn(x_13: f32) -> vec4<f32> {
  let e0 = 0.0;
  let v0 = vec4<f32>(1.0,0.0,0.0,1.0);
  let e1 = 1.0;
  let v1 = vec4<f32>(1.0,1.0,0.0,1.0);
  let a0 = smoothstep(e0,e1,x_13);
  return mix(v0,v1,a0)*step(e0,x_13)*step(x_13,e1);
}

fn winter(x_12: f32) -> vec4<f32> {
  let e0 = 0.0;
  let v0 = vec4<f32>(0.0,0.0,1.0,1.0);
  let e1 = 1.0;
  let v1 = vec4<f32>(0.0,1.0,0.5019607843137255,1.0);
  let a0 = smoothstep(e0,e1,x_12);
  return mix(v0,v1,a0)*step(e0,x_12)*step(x_12,e1);
}
