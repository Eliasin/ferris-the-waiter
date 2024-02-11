# Ferris the Waiter

## Installing Additional Tools

By default, `cargo-leptos` uses `nightly` Rust, `cargo-generate`, and `sass`. If you run into any trouble, you may need to install one or more of these tools.

1. `rustup toolchain install nightly --allow-downgrade` - make sure you have Rust nightly
2. `rustup target add wasm32-unknown-unknown` - add the ability to compile Rust to WebAssembly
3. `rustup component add rust-analyzer` - add the ability to compile Rust to WebAssembly
4. `cargo install cargo-generate` - install `cargo-generate` binary (should be installed automatically in future)
5. `npm install -g sass` - install `dart-sass` (should be optional in future

## Running the dev server

```bash
FERRIS_WAITER_CONFIG=config.toml cargo leptos watch
```

~TAILWIND INSTRUCTIONS PLACEHOLDER :)

## Getting a bcrypt hash for the `password` field in the config

A simple and easy way to grab a hash is to just slap something like this in `main.rs` and run the dev server.

```rust
    println!("{}", bcrypt::hash("MY_PASSWORD", 12));
```

## Compiling for Release

```bash
cargo leptos build --release
```

Will generate your server binary in target/server/release and your site package in target/site. There is also `package.sh` which will compress the result into `ferris-the-waiter.tar.gz`.

## Testing Your Project

```bash
cargo leptos end-to-end
```

```bash
cargo leptos end-to-end --release
```

Cargo-leptos uses Playwright as the end-to-end test tool.  
Tests are located in end2end/tests directory.

## Executing a Server on a Remote Machine Without the Toolchain

After running a `cargo leptos build --release` the minimum files needed are:

1. The server binary located in `target/server/release`
2. The `site` directory and all files within located in `target/site`

Copy these files to your remote server. The directory structure should be:

```text
ferris-the-waiter
site/
```

Set the following environment variables (updating for your project as needed):

```text
LEPTOS_OUTPUT_NAME="ferris-the-waiter"
LEPTOS_SITE_ROOT="site"
LEPTOS_SITE_PKG_DIR="pkg"
LEPTOS_SITE_ADDR="127.0.0.1:3000"
LEPTOS_RELOAD_PORT="3001"
```

Finally, run the server binary.
