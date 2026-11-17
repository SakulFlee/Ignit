<div align="center">

<img src=".github/logo/Orbital_Logo.svg" alt="Ignit Engine Logo" width="200"/>

# 🛰️ Ignit Engine & Framework


![](https://img.shields.io/badge/Forgejo-Origin-orange?style=for-the-badge&logo=forgejo&logoSize=auto&link=https%3A%2F%2Fforgejo.sakul-flee.de%2FSakulFlee%2FIgnit%2F)
![](https://img.shields.io/badge/GitHub-Mirror-blue?style=for-the-badge&logo=github&logoSize=auto&link=https%3A%2F%2Fgithub.com%2FSakulFlee%2FIgnit)

_Ignit_ is a cross-platform real-time graphical rendering engine & framework written in Rust.
The goal of this project is to bridge well established industry-standard techniques with upcoming (next-gen) and experimental techniques to form a robust new foundation for next-gen visual computing.

</div>

## Showcase

| glTF demo: Damaged Helmet| |
| - | - |
| ![A](Images/DamagedHelmet.png) | |
| PBR Grid: Spheres | PBR Grid: Cubes |
| ![A](Images/PBRGridSpheres.png) | ![A](Images/PBRGridCubes.png) |

## Core Features 

<div align="center">

| Feature | Description |
|--------|-------------|
| **PBR Rendering** | Physically Based Rendering with realistic material properties and lighting |
| **Image-Based Lighting** | Advanced lighting using HDR environment maps for realistic reflections |
| **GLTF 2.0 Support** | Full import capabilities for complex 3D scenes, materials, and animations |
| **Cross-Platform** | Unified rendering across desktop platforms via WGPU abstraction |
| **Modular Architecture** | Flexible element system with message-based communication |
| **Modern Shaders** | WGSL-based shaders for cross-platform compatibility |

</div>

## Experiments

<div align="center">

| Experimental Concept | Description |
| - | - |
| HJAA (Halton-Jitter Anti-Aliasing) | Advanced spatial smoothing utilizing the [Halton Sequence](https://en.wikipedia.org/wiki/Halton_sequence) |
| IVXGI (Incremental Voxel-based Global Illumination) | A evolution of [Nvidia's VXGI](https://developer.download.nvidia.com/assets/events/GDC15/GEFORCE/VXGI_Dynamic_Global_Illumination_GDC15.pdf) for high performance dynamic lighting. |

</div>

> [!IMPORTANT]
> Documentation for experimental features will be released once they have been fully validated in practical, non-theoretical environments.

## Platform Compatibility

- ✅ **Windows**: Full support
- ✅ **Linux**: Full support
- ✅ **macOS**: Full support
- ⚠️ **Web**: Theoretically possible but not currently supported
- ⚠️ **Android**: Works but requires frequent updates to keep compatible
- ⚠️ **iOS**: Compiles but untested — boilerplate provided, community testing welcome

## Getting Started

The official project templates live in the `Examples/` directory. They are
workspace members and mirror the templates used by `ignit init`:

- **minimal-3d**: A small procedural 3D scene with a camera and a spotlight
- **all-in-one**: Procedural scene, glTF import, lighting, shadows, UI, and overlays
- **minimal-2d**: A minimal 2D shape rendering example

The examples are self-contained and can be run from their own directories:

```shell
(cd Examples/minimal-3d && cargo run)
(cd Examples/all-in-one && cargo run)
(cd Examples/minimal-2d && cargo run)
```

They can also be run from the repository root:

```shell
cargo run -p template_minimal_3d
cargo run -p template_all_in_one
cargo run -p template_minimal_2d
```

`minimal-3d` and `minimal-2d` need no asset files. `all-in-one` ships the scene and helmet
model it uses in `Examples/all-in-one/Assets/`.

To create a standalone project from one of these templates, use the Ignit CLI:

```shell
ignit init my-game --template all-in-one
```

To use the engine in your own project, add the following to your `Cargo.toml`:

```toml
ignit = { git = "https://forgejo.sakul-flee.de/SakulFlee/Ignit.git", branch = "main" }
```

## Nix

The flake packages the `ignit` CLI. It provides no dev shell — install the
binary and let Cargo build your project:

```shell
nix shell github:SakulFlee/Ignit#ignit-cli
# or
nix profile install github:SakulFlee/Ignit#ignit-cli
```

On NixOS, build from a shell that provides the engine's system libraries, and
enable `hardware.graphics` for the Vulkan drivers:

```shell
nix-shell -p cargo pkg-config systemd wayland libxkbcommon vulkan-loader mesa \
  --run 'ignit build'
```

Android builds need a container or `programs.nix-ld`, because the CLI-managed
SDK and JDK are generic Linux builds.

## Performance Reporting

The debug UI can write FPS, frame-stage timings and cache counters to
`Logs/performance.csv`. See
[PERFORMANCE_REPORTING.md](PERFORMANCE_REPORTING.md) for enabling capture and
retrieving the file on desktop, Android and iOS, plus the bundled analysis
notebook.

## Engine Concepts

### Elements

The primary way to add interactive objects to your world is through **Elements**. Elements are the core game objects that exist in the world and handle their own behavior. Each element can register itself with the world to define what resources it needs (models, cameras, etc.), and can respond to various events during the application lifecycle such as updates and messages from other elements.

### Messaging System

Elements communicate with each other and the world through a message-passing system rather than shared memory. This ensures loose coupling between elements and enables flexible, modular game design. Rather than sharing memory or directly accessing other elements, elements send messages to communicate. This approach is easily scalable - tags can be used multiple times, so messages will be automatically sent to all elements with a matching tag.

### World Architecture

The world manages resource stores (models, cameras, environments, lights), handles the asset import system, and creates the global bind group used by shaders. Resources follow a specific lifecycle involving creation, realization, caching, and cleanup.

## History

Ignit has a rich history spanning multiple iterations and rewrites. The project originally started as "Akimo" and was written in Java as a pure CPU renderer for 2D graphics. Over the years, it evolved through multiple iterations:

- **Akimo (Java)**: Started as a CPU-based 2D renderer, then upgraded to use LWJGL for OpenGL and Vulkan support
- **Akimo (C++)**: A complete rewrite in C++ with Vulkan bindings, still under the "Akimo" name
- **Orbital (Rust/Vulkan)**: A rewrite in Rust but still using Vulkan bindings
- **Orbital (Rust/WGPU/WGSL)**: The previous Rust/WGPU iteration, renamed to Ignit
- **Ignit (Rust/WGPU/WGSL)**: The current iteration, renamed from Orbital for v1.6.0

Each version learned from the previous ones. The current Ignit implementation is built in Rust and uses the WGPU graphics API abstraction layer for cross-platform support.

## Contributing

[Forgejo](https://forgejo.sakul-flee.de/SakulFlee/Ignit) is the canonical repository; GitHub is a mirror.
Please open pull requests on Forgejo.
If you can only use GitHub, treat a GitHub pull request as a proposal that will be ported to Forgejo before merge.

AI tools are welcome, but the contributor owns the result:

- Inspect and understand every line you submit.
- Verify that it builds and that relevant tests, lints, and examples pass.
- Do not add AI/model attribution to code, commits, documentation, or pull-request metadata. AI tools are not authors, co-authors, owners, or contributors.
- The person opening the pull request is responsible for correctness, licensing, security, and follow-up maintenance.

See [AGENTS.md](AGENTS.md) for the full contribution contract, architecture map, tooling, and pre-PR checklist.

## License

This project is dual licensed in Rust's fashion:

- [MIT License](https://spdx.org/licenses/MIT.html)
- [Apache License 2.0](https://spdx.org/licenses/Apache-2.0.html)

For your own project you can chose whichever fits you better.
For templates/examples we recommend to also dual-licensing.

> We highly encourage everyone to share their sub-crates with the community so that others can benefit from it too!

[Ignit]: https://github.com/SakulFlee/Ignit
[WGPU]: https://wgpu.rs/
