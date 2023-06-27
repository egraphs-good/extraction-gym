.PHONY: all bench test nits

SRC=$(shell find . -name '.rs') Cargo.toml Cargo.lock
DATA=$(shell find data -name '*.csv')

all: test nits bench

bench: $(SRC) $(DATA)
	cargo run --release -- $(DATA) | tee out.csv

test:
	cargo test --release
	cargo test --release --all-features

nits:
	rustup component add rustfmt clippy
	cargo fmt -- --check
	cargo clean --doc

	cargo clippy --tests
	cargo clippy --tests --all-features