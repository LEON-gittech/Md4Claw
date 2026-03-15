# AppFlowy macOS Build Guide

This guide covers how to compile and run AppFlowy from source on macOS.

## Prerequisites

### 1. System Tools

```bash
# Xcode command line tools
xcode-select --install

# Homebrew (if not installed)
/bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"

# Required packages
brew install cmake protobuf
```

### 2. Rust Toolchain (>= 1.85)

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source "$HOME/.cargo/env"

# Add Apple Silicon or Intel target
rustup target add aarch64-apple-darwin   # Apple Silicon (M1/M2/M3)
rustup target add x86_64-apple-darwin    # Intel Mac

# Install cargo-make (>= 0.37.18)
cargo install cargo-make
```

### 3. Flutter (3.27.4)

```bash
# Clone Flutter at the required version
git clone https://github.com/flutter/flutter.git -b 3.27.4 ~/flutter
export PATH="$HOME/flutter/bin:$PATH"

# Verify
flutter --version
flutter doctor
```

Add to your shell profile (`~/.zshrc` or `~/.bash_profile`):
```bash
export PATH="$HOME/flutter/bin:$PATH"
```

## Build Steps

### Step 1: Build the Rust Backend

```bash
cd frontend/rust-lib

# Apple Silicon (M1/M2/M3)
cargo make --profile development-mac-arm64 appflowy-core-dev

# Intel Mac
cargo make --profile development-mac-x86_64 appflowy-core-dev
```

This compiles the Rust core library (`libdart_ffi.dylib`) that the Flutter app loads via FFI.

### Step 2: Build and Run the Flutter App

```bash
cd frontend/appflowy_flutter

# Install Dart/Flutter dependencies
flutter pub get

# Generate code (protobuf, freezed, etc.)
flutter packages pub run build_runner build --delete-conflicting-outputs

# Run the app
flutter run -d macos
```

## Build Profiles

| Profile | Target | Description |
|---------|--------|-------------|
| `development-mac-arm64` | `aarch64-apple-darwin` | Debug build for Apple Silicon |
| `development-mac-x86_64` | `x86_64-apple-darwin` | Debug build for Intel Mac |

## Troubleshooting

### Protobuf errors during Rust build
The Rust build generates protobuf code. If you see protobuf-related errors, ensure `protoc` is on your PATH:
```bash
protoc --version   # should show libprotoc 3.x or higher
```

### Flutter doctor issues
Run `flutter doctor -v` and resolve any reported issues before building.

### dylib not found at runtime
Make sure you built the Rust backend with the correct profile for your architecture before running the Flutter app.
