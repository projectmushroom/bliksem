#include <metal_stdlib>
#include <SwiftUI/SwiftUI.h>
using namespace metal;

// Phosphor bloom, scanlines and vignette. `pos` is in points; one font pixel is one point.
[[ stitchable ]] half4 phosphor(float2 pos, SwiftUI::Layer layer, float2 size) {
    half4 c = layer.sample(pos);
    half3 glow = half3(0.0h);
    const float r = 2.0;
    const float2 near[8] = { float2(r, 0), float2(-r, 0), float2(0, r), float2(0, -r),
                             float2(r, r), float2(-r, r), float2(r, -r), float2(-r, -r) };
    for (int i = 0; i < 8; i++) glow += layer.sample(pos + near[i]).rgb;
    const float2 far[4] = { float2(2 * r, 0), float2(-2 * r, 0), float2(0, 2 * r), float2(0, -2 * r) };
    for (int i = 0; i < 4; i++) glow += layer.sample(pos + far[i]).rgb * 0.5h;
    half3 col = c.rgb + glow * 0.07h;
    float scan = 0.74 + 0.26 * cos(pos.y * 6.2831853);
    col *= half(scan);
    float2 uv = pos / size * 2.0 - 1.0;
    float d = dot(uv, uv);
    col *= half(1.0 - 0.22 * d * d);
    return half4(col, c.a);
}

// Barrel curvature: for each output pixel, where to sample the flat screen.
[[ stitchable ]] float2 curve(float2 pos, float2 size, float amount) {
    float2 uv = pos / size * 2.0 - 1.0;
    uv *= 1.0 + amount * dot(uv, uv);
    return (uv + 1.0) * 0.5 * size;
}

