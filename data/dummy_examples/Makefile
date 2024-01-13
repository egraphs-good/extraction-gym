# make the .json data from the .egg files in this directory
# run make from this directory
# FIXME this doesn't quite work yet, egglog
# doesn't export roots properly

egg_files = $(wildcard *.egg)
json_files = $(egg_files:.egg=.json)

all: $(json_files)

egglog_manifest = ../../../egglog/Cargo.toml

%.json: %.egg
	cargo run --manifest-path $(egglog_manifest) -- $< --to-json
	