# x86_64 by default; on an ARM host or when cross-compiling: make build TARGET=aarch64-unknown-linux-musl
TARGET ?= x86_64-unknown-linux-musl

.PHONY: setup dev test lint build

setup:
	cargo fetch

dev:
	cargo run -- help

test:
	cargo test

lint:
	cargo fmt --check
	cargo clippy -- -D warnings

build:
	cargo build --locked --release --target $(TARGET)
