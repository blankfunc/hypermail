$env:RUSTFLAGS='--cfg getrandom_backend="wasm_js"'
cargo build -r --target wasm32-unknown-unknown