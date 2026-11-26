fn pipeline_fragment(input: PipelineInput) -> vec4<f32> {
    let dir = normalize(input.position);
    let elev = dir.y * 0.5 + 0.5;
    let t = frame.time;
    let top = input.color.rgb * (0.75 + 0.25 * sin(t * 0.15));
    let bot = input.color.rgb * 0.25 + vec3<f32>(0.05, 0.08, 0.16);
    let rgb = mix(bot, top, elev);
    return vec4<f32>(rgb, input.color.a * (0.55 + elev * 0.35));
}
