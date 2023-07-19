pub fn get_toml() -> String {
    r#"
[package]
name = "contract"
version = "0.1.0"
edition = "2021"

[dependencies]
serde = { version = "1.0.152", features = ["derive"] }
serde_json = "1.0.92"
json-patch = "~0.2"
thiserror = "~1.0"
taple-sc-rust = { git = "https://github.com/opencanarias/taple-sc-rust.git", branch = "main"}

[profile.release]
strip = "debuginfo"
lto = true

[lib]
crate-type = ["cdylib"]
  "#
    .into()
}
