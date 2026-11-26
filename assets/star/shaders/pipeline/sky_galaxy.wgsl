fn pipeline_fragment(input: PipelineInput) -> vec4<f32> {
    let dir = normalize(input.position);
    let t = frame.time * 0.08;
    let spin = atan2(dir.z, dir.x) + t;
    let arms = 0.5 + 0.5 * sin(spin * 3.0 + dir.y * 6.0);
    let core = exp(-length(dir.xz) * 1.8) * (0.6 + 0.4 * sin(t * 4.0));
    let stars = step(0.992, fract(sin(dot(dir.xy, vec2<f32>(12.9898, 78.233)) + t) * 43758.5453));
    let accent = input.color.rgb;
    let deep = vec3<f32>(0.02, 0.02, 0.08);
    let rgb = mix(deep, accent, arms * 0.55 + core) + stars * vec3<f32>(1.0);
    let a = input.color.a * (0.4 + arms * 0.4 + core * 0.3);
    return vec4<f32>(rgb, a);
}
