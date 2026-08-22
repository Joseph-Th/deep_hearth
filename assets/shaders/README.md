# Shader Contract

This document owns the adapter-facing contract for Deep Hearth's renderer-neutral WGSL content. See
[`../../TECHNICAL_DESIGN.md`](../../TECHNICAL_DESIGN.md) for the presentation boundary and
[`../../TESTING.md`](../../TESTING.md) for repository validation policy.

## Where to work

| Concern | Location |
| --- | --- |
| WGSL libraries and executable sources | `assets/shaders/*.wgsl` |
| Built-in shader IDs, dependencies, pipeline profiles, and work budgets | `src/content/shaders.rs` |
| Shader registry and deterministic assembly | `src/shader/` |
| Indexed texture definitions and upload contract | `src/content/textures.rs`, `src/texture/` |
| Standalone WGSL validator | `src/bin/validate_shaders.rs` |

Run `python ci.py gate --shaders` after changing WGSL, shader assembly, built-in shader definitions, or
their adapter contract. Naga is available only through the `test-shader-validation` feature; the default
crate has no graphics dependency.

At adapter startup, call `registries.shaders().bake_shader_set()` once, compile the assembled executable
programs, and retain the backend pipelines. Assembly expands each shared library once in stable shader-ID
order.

## Frame order

1. Render opaque casters with `SHADER_SHADOW` and cutout casters with `SHADER_SHADOW_CUTOUT` into a
   directional-light depth texture. Both pipelines have no color target.
2. Render `SHADER_SKY` into the linear HDR target without depth writes.
3. Dispatch `SHADER_LIGHT_CULL` once per 16x16 screen tile.
4. Render opaque and cutout geometry with `SHADER_SURFACE` into linear HDR plus depth.
5. Make the opaque HDR color and depth readable, then render `SHADER_WATER` back-to-front.
6. Render `SHADER_SMOKE` back-to-front, preferably into the same HDR target. Dense particle systems
   may use a half-resolution target and composite once to reduce overdraw.
7. Dispatch `SHADER_BLOOM` into a half-resolution `rgba16float` storage texture.
8. Render `SHADER_POST_PROCESS` to the display target. The display target performs the final linear
   to sRGB conversion; do not pre-encode the HDR inputs.

Shadowing and bloom are optional. If their programs remain compiled while disabled, bind valid neutral
resources: depth 1.0 for shadows and black for bloom.

WGSL/WebGPU depth is 0.0 to 1.0. The supplied projection and inverse projection matrices must use
that convention. Water and smoke output premultiplied alpha and require ONE / ONE_MINUS_SRC_ALPHA
color blending with depth testing enabled and depth writes disabled.

Each render definition declares a depth mode (`Disabled`, `ReadOnly`, `ReadWrite`) and color target
(`None`, `LinearHdr`, `Display`). Read modes use less-equal depth comparison. Scene-color, bloom, and
post-process linear samplers clamp to edge; the shadow comparison sampler also clamps to edge and uses
less-equal comparison.

## Surface resources

The surface and cutout-shadow programs consume the texture baker without transcoding:

| Binding resource | GPU representation |
| --- | --- |
| indexed texture mips | `R8Uint` 2D array, loaded as `texture_2d_array<u32>` |
| palette rows | `R16Uint` 2D texture, width 16 |
| palette colors | `Rgba8Unorm` 2D texture, width 16 |
| mesh texture key | low 16 bits: array layer; high 16 bits: palette row |

Fetch palette indices with `textureLoad`; never linearly filter them. Surface derivatives select one of
the authored 32/16/8/4/2/1 discrete mips. The Rust texture contract injects the base side and maximum mip
into shared WGSL so surface and cutout-shadow code do not duplicate those constants.

Route baked `Opaque` descriptors through `SHADER_SHADOW` and `Cutout` descriptors through
`SHADER_SHADOW_CUTOUT`. Blend-mode geometry needs an explicit adapter policy before casting shadows.

Surface vertex locations are:

| Location | Value |
| --- | --- |
| 0 | world position `vec3<f32>` |
| 1 | texture UV plus sky/block light `vec4<f32>` |
| 2 | world normal plus ambient occlusion `vec4<f32>` |
| 3 | packed texture key `u32` |
| 4 | linear tint `vec4<f32>` |

The opaque shadow pass reads only world position and has no fragment stage. The cutout shadow pass reuses
surface locations 0, 1, and 3 and samples palette alpha, so it needs no separate shadow-mesh layout.

## Tiled lighting

`SHADER_LIGHT_CULL` dispatches one `[64, 1, 1]` workgroup per 16x16 tile. In `LightCullFrame`,
`viewport_tiles.xy` is viewport size in pixels and `.zw` is tile count. `light_count.x` is the valid
point-light count. The output count buffer has one `u32` per tile; the index buffer has 32 `u32`
entries per tile.

Order the point-light buffer deterministically by visual priority plus stable light ID. The culler
considers at most 512 lights and retains the first 32 overlaps in that order. `SurfaceFrame.light_grid.x`
is tile columns and `.z` is the valid point-light count.

## Effect inputs

- Water receives opaque scene color plus matching depth. `viewport_size_inverse.xy` is viewport size and
  `.zw` is inverse size.
- Smoke instances use corners in location 0, center plus normalized age in location 1, size plus
  rotation/seed in location 2, and linear color in location 3. Soft particles use opaque depth.
  `near_far_viewport` contains near, far, viewport width, and viewport height.
- Sky uses a fullscreen triangle generated by `vertex_index` and no textures.
- Bloom writes half-resolution HDR. `source_inverse_size.xy` is inverse source size, `output_size.xy` is
  output extent, and `threshold_knee.xy` is threshold plus soft-knee width.
- Post process performs tone mapping and display effects. `exposure_bloom.xy` is exposure plus bloom
  intensity; `.zw` is inverse bloom size.

## Maximum shader work

| Program | Texture reads | Noise layers | Local lights | Largest loop |
| --- | ---: | ---: | ---: | ---: |
| surface | 7 | 0 | 32 | 32 |
| light cull | 0 | 0 | 0 | 8 |
| water | 2 | 1 | 0 | 0 |
| smoke | 1 | 3 | 0 | 0 |
| sky | 0 | 3 | 0 | 0 |
| bloom | 4 | 0 | 0 | 0 |
| post process | 5 | 0 | 0 | 0 |
| opaque shadow | 0 | 0 | 0 | 0 |
| cutout shadow | 3 | 0 | 0 | 0 |

These are authored worst-case invocation budgets, not measured timings. Update the definition, tests,
and this table together when a budget changes; profile the real adapter before increasing a limit.
