# WARP.md

This file provides guidance to WARP (warp.dev) when working with code in this repository.

## Project Overview

**stardust-xr-asteroids** is a declarative UI library for Stardust XR built in Rust. It provides a React-like paradigm for creating 3D spatial user interfaces without requiring macros in usage, making it IDE-friendly. The library enables developers to build interactive XR applications that can connect to the Stardust XR server.

## Development Commands

### Basic Rust Commands
```bash
# Build the project
cargo build

# Build in release mode
cargo build --release

# Run tests
cargo test

# Run tests with full output
cargo test -- --nocapture

# Format code (uses hard tabs as configured in rustfmt.toml)
cargo fmt

# Check for compilation errors without building
cargo check

# Run clippy for linting
cargo clippy
```

### Running Examples
```bash
# Run the basic layout example
cargo run --example basic_layout

# Run the elements add/remove example
cargo run --example elements_add_remove

# Run the recursion example
cargo run --example recursion
```

### Development Mode
```bash
# Enable development mode for faster iteration
export ASTEROIDS_DEV=1
```

### Tracy Profiling (Optional)
```bash
# Build with Tracy profiling support
cargo build --features tracy
```

## Architecture Overview

### Core Architectural Patterns

**Declarative UI Model**: The library follows a React-inspired declarative model where UI is described as a function of state. The `Reify` trait defines how application state transforms into UI elements.

**Element System**: Built around two main traits:
- `Element<State>`: High-level declarative element interface
- `CustomElement<State>`: Lower-level imperative implementation for specific UI components

**State Management**: Uses the `ValidState` trait to ensure state types are `Send + Sync + 'static`. Application state implements `ClientState` which handles serialization, migration, and app lifecycle.

### Key Components

#### Client System (`src/client.rs`)
- **ClientState**: Main application state trait with app ID, initialization, and frame callbacks
- **State Persistence**: Automatically saves/loads state to RON files, with dev mode support
- **Event Loop**: Handles Stardust XR server events including frames, ping/pong, and state saving
- **Accent Color Integration**: Automatically syncs with system accent color via D-Bus

#### Element Architecture
- **ElementWrapper**: Provides the builder pattern for adding children and properties
- **ElementDiffer**: Core diffing system that enables efficient updates by comparing old and new element trees
- **Transformable**: Mixin trait for elements that support spatial transformations (position, rotation, scale)

#### Spatial Hierarchy
All elements exist within a spatial hierarchy rooted at `Spatial` elements. Each element provides a `spatial_aspect()` method that returns the `SpatialRef` children should be parented to.

#### Built-in Elements (`src/elements/`)
The library includes a comprehensive set of XR-specific UI elements:
- **Spatial**: Basic 3D spatial container
- **Button**: Interactive button with hover states
- **Text**: 3D text rendering with alignment options
- **Lines**: 3D line/wireframe rendering
- **Model**: 3D model loading and display
- **Turntable**: Interactive rotation control
- **MouseHandler**: Mouse input handling
- **Keyboard**: Virtual keyboard input
- **And many more** (see `src/elements/mod.rs` for full list)

### State Flow

1. **Initialization**: App loads previous state or creates default
2. **Reification**: State is transformed into element tree via `reify()`
3. **Diffing**: New element tree is compared with previous tree
4. **Updates**: Only changed elements are updated in the XR scene
5. **Frame Loop**: Process input events, update state, re-reify, diff, repeat

### Resource Management

- **Resource Registry**: Shared resources across elements (textures, models, etc.)
- **Automatic Cleanup**: RAII pattern ensures proper cleanup of XR resources
- **Path-based Organization**: Resources organized by element path for debugging

### Development Patterns

**Element Creation Pattern**:
1. Define struct with `#[derive(Setters)]` for builder pattern
2. Implement `CustomElement<State>` with create/diff/frame methods
3. Implement `Transformable` if element supports spatial transforms
4. Add to `elements/mod.rs` with `mod_expose!` macro

**State Update Pattern**:
- Keep state changes in event handlers
- Use `FnWrapper` for callbacks that modify state
- Implement `Migrate` trait for state versioning

**Testing**:
- Each element includes `#[tokio::test]` integration tests
- Tests create minimal `ClientState` implementations
- Use `client::run()` to test full integration with Stardust XR

## Project Structure Notes

- `src/lib.rs`: Main library exports and core traits
- `src/client.rs`: Client connection and event loop management
- `src/element.rs`: Core element system and diffing logic
- `src/elements/`: All built-in UI elements
- `src/custom.rs`: `CustomElement` trait and utilities
- `src/mapped.rs`: State mapping utilities for sub-components
- `src/util/`: Utility modules (delta tracking, migrations, etc.)
- `examples/`: Example applications showing library usage

## Important Notes

- **Hard Tabs**: Project uses hard tabs for indentation (see `rustfmt.toml`)
- **No Macros in Usage**: Library designed to be macro-free for better IDE support
- **XR-First**: All coordinates and transforms are in 3D space
- **Async Runtime**: Uses Tokio for async operations
- **D-Bus Integration**: Connects to system services for accent colors and other OS integration

This library is part of the larger Stardust XR ecosystem and requires a running Stardust XR server to function.
