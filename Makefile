#
# Makefile
#
# Launched by 'build.rs' but also usable stand-alone.
#
# Handles:
#	- compiling the ULD C API driver code
#	- creating Rust interfaces for it
#	- turns 'pins.toml' (pin definitions) to Rust snippet, for examples
#
#	In short, this is mostly about the C/Rust binding.
#
# Requires:
#	- bindgen
#	- patch
#	- clang
#
# Env.vars:
#	- VL53L5CX_ULD_API={path}	Folder where the vendor C sources are placed
#
# Note: 'bindgen' docs show how that tool builds a static library, when used as a Rust library. We take a very similar
#		approach, but use 'bindgen' CLI instead of the library.
#
# Note: 'clang' only recognizes the 'riscv32-unknown-elf' target, not the chip specific ones seen e.g. in
#		'../.cargo/config.toml'. This seems normal - and works, so..
#
# Note: Unconditional (manual) make with 'make -B' MAY BE CURRENTLY BROKEN. Just run 'make _clean', instead. #tbd.
#
VL53L5CX_ULD_API?=./VL53L5CX_ULD_API

_TMP:=./tmp/
_C_SRC:=tmp/c_src

_OBJ_PATH:=$(_TMP)/$(TARGET)

_OTHER_INCS:=platform.h fake/*.h

_T:=$(shell cat .cargo/config.toml | grep -e '^target\s*=\s"' | cut -d '"' -f2)
	# riscv32imac-unknown-none-elf
	# riscv32imc-unknown-none-elf

ifneq (,$(findstring riscv32, $(_T)))
  _TARGET_ARCH:=riscv32-unknown-elf
  _CLANG_FLAGS:=--target=$(_TARGET_ARCH)
else
  $(error Unexpected TARGET: ${_T})
endif

all:
	@false

# The targets that 'build.rs' normally builds (for debugging, in case make fails)
manual: tmp/libvendor_uld.a tmp/uld_raw.rs
	@echo "\nManual build succeeded."

#---
# Build the static library
#
tmp/libvendor_uld.a: tmp/vl53l5cx_api.o
	ar rcs $@ $<

tmp/%.o: $(_C_SRC)/%.c wrap.h $(_C_SRC)/vl53l5cx_api.h $(_C_SRC)/vl53l5cx_buffers.h $(_OTHER_INCS) tmp/config.h Makefile \
	| clang
	@echo $<
	clang -nostdinc --target=$(_TARGET_ARCH) -Ifake -I. -I$(_C_SRC) -c -o $@ $<

#---
# Patch ST.com sources from './VL53LCX_ULD_API/**' to 'tmp/c_src/'
#
# 	- places e.g. headers and C files in the same folder, for convenience
#	- removes a nasty! reference to a field in a structure that's supposed to be customer-made and opaque
#
$(_C_SRC)/%.h: $(VL53L5CX_ULD_API)/inc/%.h
	@cp $< $@

$(_C_SRC)/vl53l5cx_api.c: $(VL53L5CX_ULD_API)/src/vl53l5cx_api.c ./patch
	patch $< ./patch -o $@

# Error at root level, if vendor sources aren't there
ifeq ("$(wildcard $(VL53L5CX_ULD_API)/src/*)", "")
$(error Vendor''s ULD driver not found; please download and place in the folder $(VL53L5CX_ULD_API))
endif

#---
# Generate Rust
#
# Generate ’tmp/uld_raw.rs’. MANUAL CHANGES TO IT WILL BE LOST; modify this instead.
#
# About 'bindgen' (v.0.69.4):
#	- prints errors to stdout, which is .... a bit unkind.
#	- does not process lines like:
#		<<
#			define VL53L5CX_RESOLUTION_4X4			((uint8_t) 16U)
#		<<
#		As a countermeasure, we declare 'consts' in 'wrap.h'.
#
# WEIRD:
#	The author didn't get '--blocklist-file' to work. It didn't deny the functions in 'platform.h' - but that's just
#	8 names.
#
#	Work-around:
#		- just use '--allowlist-*'
#
#	Similarly, '--allowlist-file {file}.h' _DID NOT WORK_ when applied to a file included from 'wrap.h'.
#
#	Work-around:
#		- keep all wrapping (also enums) in one file (good anyhow)
#
# Enums:
#	The 'sed' changes:
#		<<
#			#[repr(u32)]
#			#[derive(Debug, Copy, Clone, Hash, PartialEq, Eq)]
#			pub enum ... {
#		<<
#	into:
#		<<
#			#[repr(u32)]
#			#[derive(FromRepr, Debug, Copy, ...)]
#			pub enum ... {
#		<<
#
#	This allows 'strum' to be used, for 'MyEnum::from_repr(<int>)', which is handy.
#
#	Note: 'sed' *can* do pattern-matching that spans multiple lines (we'd ideally take that 'pub enum' into
#		the pattern and just change two parts), but that's WAY COMPLICATED. In our case, dum, separate replacements
#		do the trick.
#
#	NOTE: Pretty vital to raise logging level (for non-internals) to 'WARN'. Can be done!!
#
#	NOTE: 'build.rs' will rebuild the file on each run. This is important since the feature set may change (and we have
#		no way of knowing it in here; think a user running 'cargo build --example' separately, with different features.
#
#		Thus, the dependencies (that need to exist) are placed as "order-only" prerequisites. [1]
#			[1]: https://www.gnu.org/software/make/manual/html_node/Prerequisite-Types.html
#
tmp/uld_raw.rs: wrap.h tmp/config.h $(_OTHER_INCS) Makefile \
	| $(_C_SRC)/vl53l5cx_api.h $(_C_SRC)/vl53l5cx_buffers.h bindgen
	  RUST_LOG='warn,bindgen::ir=error' bindgen $< \
	    --allowlist-file wrap.h \
	    --allowlist-type 'VL53L5CX_.+' \
	    --allowlist-function 'vl53l5cx_check_data_ready' \
	    --allowlist-function 'vl53l5cx_get_(?:(power_mode)|(ranging_data))' \
	    --allowlist-function 'vl53l5cx_init' \
	    --allowlist-function 'vl53l5cx_set_(?:(power_mode))' \
	    --allowlist-function 'vl53l5cx_set_(?:(resolution)|(ranging_frequency_hz)|(integration_time_ms)|(sharpener_percent)|(target_order)|(ranging_mode))' \
	    --allowlist-function 'vl53l5cx_st(?:(art)|(op))_ranging' \
	    --allowlist-item 'API_REVISION' \
	    \
	    --use-core \
	    --default-enum-style rust \
	    --raw-line '#![allow(non_camel_case_types)]' \
	    --raw-line '#![allow(non_snake_case)]' \
		--raw-line 'use strum::FromRepr;' \
		--no-copy 'VL53L5CX_(?:(Configuration)|(Platform))' \
	    -- -I. -I$(_C_SRC) \
	  | sed 's/#[[]repr(u32)[]]/#[repr(u8)]/' \
	  | sed 's/#[[]derive(Debug, Copy, Clone, Hash, PartialEq, Eq)/#[derive(FromRepr, Copy, Clone, Hash, PartialEq, Eq)/' \
	  | sed 's/these field, except for the sensor address."/these fields."/' \
	  | sed -E 's/^(pub enum PowerMode)/#[allow(dead_code)]\n\1/' \
	  | sed -E 's/(vl53l5cx_set_power_mode)/_\1/' \
	  > $@

	# Note: 'sed' removes 'Debug' from the derived behaviours. This is intentional; 'defmt' uses 'Format'.

# Test for the above (for manual development, only); CAN BE LET GO
EXP_x: tmp/uld_raw.rs
	grep -q API_REVISION $<
	grep -q vl53l5cx_init $<
	grep -q vl53l5cx_get_ranging_data $<
	! grep -q 'static VL53L5CX_FIRMWARE' $<
	! grep -q vl53l5cx_is_alive $<
	! grep -q vl53l5cx_get_resolution $<
	! grep -q VL53L5CX_RdByte $<
	! grep -q 'these field, except ' $<
	@#
	@echo ""
	@echo "Yay!"

	#grep -q 'pub enum PowerMode' $<

# 'build.rs' writes 'tmp/config.h.next' on _every_ build.
# If the contents differ, update 'tmp/config.h'.
#
# Note: Keep the 'tmp/config.h.next' around - if we were to remove it, running 'make manual' repeatedly (for debugging?)
#		would fail.
#
tmp/config.h: tmp/config.h.next
	@cmp -s tmp/config.h tmp/config.h.next || \
	  cp $< $@

tmp/config.h.next: Cargo.toml
	$(error '$@' seems out-of-date (or not to exist). Please build once using 'cargo build' to have features propagate to it)

# Check that required tools are installed
bindgen:
	@which bindgen >/dev/null || ( \
	  echo >&2 "ERROR: 'bindgen' CLI not detected. Please install via 'cargo install bindgen-cli'."; false \
	)

clang:
	@which clang >/dev/null || ( \
	  echo >&2 "ERROR: 'clang' CLI not detected. Please install via 'sudo apt install llvm-dev libclang-dev clang'."; false \
	)

#---
_clean:
	-rm tmp/uld_raw.rs tmp/c_src/* tmp/vl53l5cx_api.o tmp/libvendor_uld.a \
		tmp/config.h tmp/config.h.*

_klean: _clean
	cargo clean

echo:
	@echo $(VL53L5CX_ULD_API)

#--
# Remove any targets created if a build fails.
.DELETE_ON_ERROR:

.PHONY: all bindgen clang _clean _klean echo

# #hack For some reason, giving '-B' from 'build.rs' didn't work. By declaring the file '.PHONY' we make sure it's
#	always recreated
#.PHONY: tmp/uld_raw.rs
