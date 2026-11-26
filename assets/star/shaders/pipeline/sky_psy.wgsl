fn pipeline_fragment(input: PipelineInput) -> vec4<f32> {
    let dir = normalize(input.position);
    let t = frame.time;
    let swirl = atan2(dir.z, dir.x) + dir.y * 2.5 + t * 0.55;
    let melt = sin(dir.x * 5.0 + t * 1.1) * cos(dir.y * 4.0 - t * 0.8) * sin(dir.z * 3.5 + t * 0.6);
    let hue = fract(swirl * 0.16 + melt * 0.12 + t * 0.05);
    let s = 0.75 + 0.2 * sin(t * 0.9 + melt * 3.0);
    let v = 0.55 + 0.4 * (dir.y * 0.5 + 0.5);
    // HSV to RGB
    let k = vec4<f32>(1.0, 2.0 / 3.0, 1.0 / 3.0, 3.0);
    let p = abs(fract(vec3<f32>(hue) + k.xyz) * 6.0 - k.www);
    let rgb = v * mix(vec3<f32>(1.0), clamp(p - vec3<f32>(1.0), vec3<f32>(0.0), vec3<f32>(1.0)), s);
    let tint = mix(rgb, input.color.rgb, 0.25);
    let a = input.color.a * (0.55 + 0.35 * abs(melt));
    return vec4<f32>(tint, a);
}
