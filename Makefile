EXTRACTORS=$(shell cargo run -q --release --all-features -- --extractor=print)

PROGRAM=target/release/extraction-gym

SRC=$(shell find . -name '.rs') Cargo.toml Cargo.lock
DATA=$(shell find data -name '*.csv')

TARGETS=

define run-extraction
TARGETS += $(1:data/%=output/%)-$(2).json
$(1:data/%=output/%)-$(2).json: $(1)
	mkdir -p $$(dir $$@)
	$(PROGRAM) $$< --extractor=$(2) --out=$$@
endef

$(foreach ext,$(EXTRACTORS),\
	$(foreach data,$(DATA),\
        $(eval $(call run-extraction,$(data),$(ext)))\
    )\
)

.PHONY: new
new: $(TARGETS)

.PHONY: all
all: test nits bench

$(PROGRAM): $(SRC)
	cargo build --release --all-features

.PHONY: bench
bench: $(SRC) $(DATA)
	$(PROGRAM)                -- --out=out.csv $(DATA)

# .PHONY: bench-all
# bench-all: $(SRC) $(DATA)
# 	$(PROGRAM) --all-features -- --out=out.csv $(DATA)

.PHONY: test
test:
	cargo test --release

.PHONY: nits
nits:
	rustup component add rustfmt clippy
	cargo fmt -- --check
	cargo clean --doc

	cargo clippy --tests