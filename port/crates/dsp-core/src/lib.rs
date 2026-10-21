// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! Processor graph core: processors, routers, inputs/outputs, values,
//! operators, lookup tables, delay memory and shared utilities.
//!
//! Ports upstream's `vital/src/synthesis/framework/` (except the voice and
//! note handlers, which are M7), `lookups/lookup_table.h`,
//! `lookups/memory.h`, `utilities/*` and `filters/one_pole_filter.h`. The
//! `poly_float` layer (`poly_values.h`, `poly_utils.h`, `futils.h`,
//! `matrix.h`, `common.h`, scalar `utils.h`) is [`sas_simd`], re-exported
//! here.
//!
//! # The graph
//!
//! A [`Processor`] is a node with inputs ([`Input`]) and outputs
//! ([`Output`]); graph code holds nodes as [`ProcessorRef`] (`Rc<Processor>`),
//! and node identity is `Rc` identity, as `Processor*` identity is upstream.
//! Its behaviour is a [`ProcessorImpl`] kernel. Upstream's class hierarchy
//! maps like this:
//!
//! | Upstream | Here |
//! |---|---|
//! | `class Foo : public Processor` with members and virtual overrides | `struct Foo` implementing [`ProcessorImpl`]; members are fields, overrides are trait methods taking the node as `this` |
//! | `new Foo(...)` (calls `Processor(num_inputs, num_outputs, control_rate, max_oversample)`) | `Foo::create(...)` returning `Processor::new(Foo { .. }, num_inputs, num_outputs, control_rate, max_oversample)` |
//! | `Processor::method(...)` from an override (base call) | `this.processor_method(...)`, or `this.router_method(...)` for `ProcessorRouter::` |
//! | `input(i)->source->buffer`, `input(i)->at(0)`, `input(i)->source->trigger_mask` | `this.input_buffer(i)`, `this.input(i).at(0)`, `this.input(i).trigger_mask()` |
//! | `output(i)->buffer[s] = v` | `this.output_buffer(i)[s].set(v)` (buffers are `Rc<[Cell<PolyFloat>]>`) |
//! | `input(i)->source == &Processor::null_source_` | `this.input(i).is_null_source()` |
//! | `foo->plug(bar, k)` / `foo->plugNext(bar)` (`bar` a `Processor*` or `Output*`) | `foo.plug(&bar, k)` / `foo.plug_next(&bar)` |
//! | `clone()` | [`Processor::clone_processor`]: shares state, inputs and outputs, copies the kernel (as upstream's copy constructor does) |
//! | `dynamic_cast<Foo*>(p)->bar()` | `p.with::<Foo, _>(\|foo, this\| foo.bar(this))` |
//! | `ProcessorRouter` / `SynthModule` | [`Processor::new_router`] / [`SynthModule::create_with`]; routing tables and module data live on the node |
//! | `Value* v; v->set(x)` | `v.set_value(x)` (dispatches to `Value`, `SmoothValue`, `ValueSwitch`, ...) |
//!
//! Inside a kernel method, call your own virtuals directly
//! (`self.process(this, 1)`), never through the node (`this.process(1)`):
//! the kernel is borrowed while it runs. Graph edits on your own node
//! (`this.add_processor`, `this.plug`, ...) are fine, and the
//! `numInputsChanged` notification they trigger is deferred until the
//! kernel returns.
//!
//! # Polyphony
//!
//! Routers keep a global order shared by all their copies and a local order
//! per copy. The original runs the processors that were added; each copy
//! (one per voice pair, made with `clone_processor`) runs its own clones of
//! the stateful ones and shares the stateless ones ([`ProcessorImpl::has_state`]
//! false: most operators). Output buffers are shared scratch between clones,
//! as upstream: voices are processed one after another and the voice handler
//! accumulates their outputs. See [`router`] for the update protocol and
//! [`RouterHooks`] for the `VoiceHandler` overrides.
//!
//! # Denormals (Voice Engine / plugin)
//!
//! Upstream processes every block inside JUCE's `ScopedNoDenormals`
//! (flush-to-zero; FTZ+DAZ on x86, FPCR.FZ on arm64), and the reference
//! renders do too. The engine's block entry point must hold a
//! [`ScopedNoDenormals`] guard around processing, at the same place, or
//! anything that decays into the denormal range will differ.
//!
//! # Threads
//!
//! The graph uses `Rc`/`Cell` and is single-threaded. It may be moved to
//! another thread only as a whole (for example inside the plugin's engine
//! wrapper with an `unsafe impl Send`), never shared. Unconnected inputs read
//! a per-thread zero buffer that is borrowed only for the duration of a call.
//!
//! # Numerics
//!
//! All arithmetic is the straightforward translation of upstream (no FMA,
//! no estimates, upstream's operation order). The per-ISA sites of
//! `migration/decisions/reference-arm.md` (`toInt`, float→int casts, the
//! poly lane operations) are only in [`sas_simd`]; code here goes through
//! them ([`sas_simd::poly_utils::to_int`], [`sas_simd::conversions`]).
//!
//! # Not yet here
//!
//! `utils::RandomGenerator` (on `sas-common`'s mt19937 replica) is added once
//! `sas-common` merges.

pub use sas_simd;

pub mod circular_queue;
pub mod constants;
pub mod denormals;
pub mod feedback;
pub mod lookup_table;
pub mod memory;
pub mod one_pole_filter;
pub mod operators;
pub mod output;
pub mod processor;
pub mod router;
pub mod synth_module;
pub mod testing;
pub mod utilities;
pub mod value;

pub use circular_queue::CircularQueue;
pub use denormals::ScopedNoDenormals;
pub use feedback::Feedback;
pub use lookup_table::OneDimLookup;
pub use memory::{Memory, StereoMemory};
pub use one_pole_filter::OnePoleFilter;
pub use output::{Buffer, Input, Output};
pub use processor::{PlugSource, Processor, ProcessorImpl, ProcessorRef, ProcessorState};
pub use router::{ProcessorRouter, RouterHooks};
pub use synth_module::{ParameterDetails, StatusOutput, SynthModule, ValueScale};
pub use value::{Value, ValueKernel};

/// Milestone in `migration/PLAN.md` that ports this crate.
pub const MILESTONE: &str = "M1";

/// Upstream files (relative to the repository root) this crate ports.
pub const UPSTREAM_SOURCES: &[&str] = &[
    "vital/src/synthesis/framework/processor.h",
    "vital/src/synthesis/framework/processor.cpp",
    "vital/src/synthesis/framework/processor_router.h",
    "vital/src/synthesis/framework/processor_router.cpp",
    "vital/src/synthesis/framework/feedback.h",
    "vital/src/synthesis/framework/feedback.cpp",
    "vital/src/synthesis/framework/value.h",
    "vital/src/synthesis/framework/value.cpp",
    "vital/src/synthesis/framework/operators.h",
    "vital/src/synthesis/framework/operators.cpp",
    "vital/src/synthesis/framework/synth_module.h",
    "vital/src/synthesis/framework/synth_module.cpp",
    "vital/src/synthesis/framework/circular_queue.h",
    "vital/src/synthesis/lookups/lookup_table.h",
    "vital/src/synthesis/lookups/memory.h",
    "vital/src/synthesis/utilities/",
    "vital/src/synthesis/filters/one_pole_filter.h",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upstream_sources_exist() {
        let repo = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
        for source in UPSTREAM_SOURCES {
            assert!(repo.join(source).exists(), "missing upstream path {source}");
        }
        assert!(MILESTONE.starts_with('M'));
    }
}
