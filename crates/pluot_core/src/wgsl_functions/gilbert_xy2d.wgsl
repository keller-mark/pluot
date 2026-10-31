// Mirrors `gilbert_xy2d` in `gilbert.rs`. WGSL has no recursion, and each
// recursive step of the reference implementation descends into exactly one
// sub-block, so the recursion is unrolled into a loop over (x, y, a, b) blocks.
fn gilbert_contains(p: vec2<i32>, start: vec2<i32>, extent: vec2<i32>) -> bool {
  let lo = select(start, start + extent + vec2<i32>(1), extent < vec2<i32>(0));
  let hi = select(start + extent - vec2<i32>(1), start, extent < vec2<i32>(0));
  return all(p >= lo) && all(p <= hi);
}

fn gilbert_xy2d(cell: vec2<u32>, width: u32, height: u32) -> u32 {
  let p = vec2<i32>(cell);
  var start = vec2<i32>(0);
  var a = select(vec2<i32>(0, i32(height)), vec2<i32>(i32(width), 0), width >= height);
  var b = select(vec2<i32>(i32(width), 0), vec2<i32>(0, i32(height)), width >= height);
  var position = 0u;

  for (var depth = 0u; depth < 128u; depth = depth + 1u) {
    let w = abs(a.x + a.y);
    let h = abs(b.x + b.y);
    let da = sign(a);
    let db = sign(b);
    if (h == 1) {
      return position + u32(dot(da, p - start));
    }
    if (w == 1) {
      return position + u32(dot(db, p - start));
    }

    // `>> 1u` rounds towards negative infinity, as the reference's `// 2` does.
    var a2 = a >> vec2<u32>(1u);
    var b2 = b >> vec2<u32>(1u);
    let w2 = abs(a2.x + a2.y);
    let h2 = abs(b2.x + b2.y);

    if (2 * w > 3 * h) {
      if ((w2 % 2) == 1 && w > 2) {
        a2 = a2 + da;
      }
      if (gilbert_contains(p, start, a2 + b)) {
        a = a2;
        continue;
      }
      position = position + u32(abs((a2.x + a2.y) * (b.x + b.y)));
      start = start + a2;
      a = a - a2;
      continue;
    }

    if ((h2 % 2) == 1 && h > 2) {
      b2 = b2 + db;
    }
    if (gilbert_contains(p, start, b2 + a2)) {
      a = b2;
      b = a2;
      continue;
    }
    position = position + u32(abs((b2.x + b2.y) * (a2.x + a2.y)));
    if (gilbert_contains(p, start + b2, a + b - b2)) {
      start = start + b2;
      b = b - b2;
      continue;
    }
    position = position + u32(abs((a.x + a.y) * ((b.x - b2.x) + (b.y - b2.y))));
    start = start + (a - da) + (b2 - db);
    let next_a = -b2;
    b = -(a - a2);
    a = next_a;
  }
  return position;
}

// floor(numerator * multiplier / divisor) without overflowing 32 bits, where
// numerator < divisor < 2^31: binary long division over the bits of `multiplier`.
fn mul_div_floor(numerator: u32, multiplier: u32, divisor: u32) -> u32 {
  var quotient = 0u;
  var remainder = 0u;
  for (var bit = 31i; bit >= 0i; bit = bit - 1i) {
    quotient = quotient << 1u;
    remainder = remainder << 1u;
    if (remainder >= divisor) {
      remainder = remainder - divisor;
      quotient = quotient + 1u;
    }
    if (((multiplier >> u32(bit)) & 1u) == 1u) {
      remainder = remainder + numerator;
      if (remainder >= divisor) {
        remainder = remainder - divisor;
        quotient = quotient + 1u;
      }
    }
  }
  return quotient;
}
