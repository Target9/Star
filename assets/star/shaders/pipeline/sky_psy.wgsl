fn pipeline_fragment(input: PipelineInput) -> vec4<f32> {
    let dir = normalize(input.position);
    let t = frame.time;

    // Multi-frequency swirl + melt so the dome keeps reshaping.
    let ang = atan2(dir.z, dir.x);
    let swirl = ang * 1.35 + dir.y * 3.4 + t * 0.85;
    let melt1 = sin(dir.x * 6.5 + t * 1.35) * cos(dir.y * 5.2 - t * 1.05);
    let melt2 = sin(dir.z * 4.8 - t * 0.95) * cos(dir.x * 3.2 + t * 0.7);
    let melt3 = sin((dir.x + dir.z) * 7.0 + t * 1.6) * 0.55;
    let melt = melt1 * melt2 + melt3;

    // Nested fractal banding — classic “breathing geometry” look.
    let radial = length(dir.xz);
    let bands = sin(radial * 14.0 - t * 2.2 + melt * 4.0)
        * cos(ang * 3.0 + t * 0.9 + dir.y * 5.0);
    let cells = sin(dir.x * 9.0 + melt * 6.0 + t)
        * sin(dir.y * 8.0 - t * 1.3)
        * sin(dir.z * 7.5 + t * 0.8);

    let hue = fract(swirl * 0.22 + melt * 0.28 + bands * 0.08 + t * 0.11);
    let s = clamp(0.78 + 0.22 * sin(t * 1.4 + melt * 5.0) + 0.08 * cells, 0.55, 1.0);
    let v = clamp(0.42 + 0.45 * (dir.y * 0.5 + 0.5) + 0.18 * abs(melt) + 0.08 * abs(bands), 0.25, 1.0);

    // HSV → RGB
    let k = vec4<f32>(1.0, 2.0 / 3.0, 1.0 / 3.0, 3.0);
    let p = abs(fract(vec3<f32>(hue) + k.xyz) * 6.0 - k.www);
    let rgb = v * mix(vec3<f32>(1.0), clamp(p - vec3<f32>(1.0), vec3<f32>(0.0), vec3<f32>(1.0)), s);

    // Second hue layer for oil-slick iridescence.
    let hue2 = fract(hue + 0.33 + melt * 0.05);
    let p2 = abs(fract(vec3<f32>(hue2) + k.xyz) * 6.0 - k.www);
    let rgb2 = v * mix(vec3<f32>(1.0), clamp(p2 - vec3<f32>(1.0), vec3<f32>(0.0), vec3<f32>(1.0)), s * 0.9);
    let mixed = mix(rgb, rgb2, 0.35 + 0.25 * sin(t * 0.7 + bands));

    let tint = mix(mixed, input.color.rgb, 0.12);
    let a = input.color.a * clamp(0.50 + 0.40 * abs(melt) + 0.15 * abs(bands), 0.35, 0.95);
    return vec4<f32>(tint, a);
}
