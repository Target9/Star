fn pipeline_fragment(input: PipelineInput) -> vec4<f32> {
    let dir = normalize(input.position);
    let t = frame.time;
    let band = sin(dir.x * 3.0 + t * 0.7) * cos(dir.z * 2.2 - t * 0.45);
    let ribbon = smoothstep(0.15, 0.85, dir.y + band * 0.25);
    let pulse = 0.55 + 0.45 * sin(t * 1.3 + dir.x * 4.0);
    let accent = input.color.rgb;
    let teal = vec3<f32>(0.15, 0.85, 0.55);
    let rgb = mix(accent, teal, 0.35 + 0.35 * sin(t * 0.4 + dir.z * 2.0)) * pulse;
    return vec4<f32>(rgb * ribbon, input.color.a * ribbon * 0.85);
}
