.PHONY: all bench test nits

SRC=$(shell find . -name '.rs') Cargo.toml Cargo.lock
DATA=$(shell find data -name '*.csv')

all: test nits bench

bench: $(SRC) $(DATA)
	cargo run --release                -- --out=out.csv $(DATA)

bench-all: $(SRC) $(DATA)
	cargo run --release --all-features -- --out=out.csv $(DATA)

test:
	cargo test --release

nits:
	rustup component add rustfmt clippy
	cargo fmt -- --check
	cargo clean --doc

	cargo clippy --tests