//! Opt-in profiling with a host-supplied monotonic nanosecond clock.
//!
//! Native durations are inclusive of Ruby callbacks. `exclusive_native_ns`
//! subtracts nested native calls, but still includes callback bytecode. Ruby
//! stacks are sampled every N instructions, not at fixed wall-time intervals;
//! sample counts must not be presented as CPU-time percentages.

use alloc::{format, string::String, vec::Vec};
use hashbrown::HashMap;

use crate::object::IrepId;
use crate::symbol::Sym;
use crate::{Value, Vm};

#[derive(Clone, Debug, Default)]
pub struct NativeStats {
    pub name: String,
    pub calls: u64,
    pub inclusive_ns: u64,
    pub exclusive_native_ns: u64,
    pub max_ns: u64,
    pub callers: HashMap<(IrepId, usize), CallerStats>,
}

#[derive(Clone, Debug, Default)]
pub struct CallerStats {
    pub calls: u64,
    pub inclusive_ns: u64,
}

#[derive(Clone, Debug)]
pub struct NativeSpan {
    pub native: usize,
    pub start_ns: u64,
    pub duration_ns: u64,
    pub caller: Option<(IrepId, usize)>,
}

#[derive(Clone, Debug)]
pub struct Sample {
    pub timestamp_ns: u64,
    /// Outermost to innermost, indexing `Profile::frames`.
    pub stack: Vec<usize>,
}

struct ActiveNative {
    native: usize,
    start_ns: u64,
    children_ns: u64,
    caller: Option<(IrepId, usize)>,
}

/// Aggregates remain active after the bounded trace buffer fills.
pub struct Profile {
    pub natives: Vec<NativeStats>,
    pub spans: Vec<NativeSpan>,
    pub samples: Vec<Sample>,
    pub frames: Vec<String>,
    pub dropped_events: u64,
    pub next_sample: u64,
    clock: fn() -> u64,
    interval: u64,
    limit: usize,
    min_span_ns: u64,
    native_ids: HashMap<(usize, u32), usize>,
    frame_ids: HashMap<(IrepId, Option<Sym>, u32), usize>,
    active: Vec<ActiveNative>,
}

impl Profile {
    pub fn new(clock: fn() -> u64, sample_instructions: u64, event_limit: usize) -> Self {
        Self {
            natives: Vec::new(),
            spans: Vec::new(),
            samples: Vec::new(),
            frames: Vec::new(),
            dropped_events: 0,
            next_sample: 0,
            clock,
            interval: sample_instructions.max(1),
            limit: event_limit,
            min_span_ns: 100_000,
            native_ids: HashMap::new(),
            frame_ids: HashMap::new(),
            active: Vec::new(),
        }
    }

    fn has_room(&self) -> bool {
        self.spans.len() + self.samples.len() < self.limit
    }
}

impl Vm {
    pub(crate) fn profile_native_begin(&mut self, address: usize, receiver: Value) {
        let Some(profile) = self.profile.as_ref() else {
            return;
        };
        let class = if receiver.obj().is_some_and(|id| self.heap.is_class(id)) {
            receiver.obj().unwrap()
        } else {
            self.real_class_of(receiver)
        };
        let key = (address, class.0);
        let existing = profile.native_ids.get(&key).copied();
        let id = if let Some(id) = existing {
            id
        } else {
            // Resolve names once per callable/receiver class, never on every
            // call. Direct SEND natives do not otherwise carry a method name.
            let mid = self.profile_native_method();
            let name = format!(
                "{}#{}",
                self.class_name(class),
                mid.map(|m| self.sym_name(m))
                    .unwrap_or_else(|| "<native>".into())
            );
            let profile = self.profile.as_mut().unwrap();
            let id = profile.natives.len();
            profile.natives.push(NativeStats {
                name,
                ..NativeStats::default()
            });
            profile.native_ids.insert(key, id);
            id
        };
        let profile = self.profile.as_mut().unwrap();
        profile.active.push(ActiveNative {
            native: id,
            start_ns: (profile.clock)(),
            children_ns: 0,
            caller: self.ci.last().map(|ci| (ci.irep, ci.pc)),
        });
    }

    pub(crate) fn profile_native_end(&mut self) {
        let Some(profile) = self.profile.as_mut() else {
            return;
        };
        let Some(active) = profile.active.pop() else {
            return;
        };
        let duration = (profile.clock)().saturating_sub(active.start_ns);
        if let Some(parent) = profile.active.last_mut() {
            parent.children_ns = parent.children_ns.saturating_add(duration);
        }
        let stats = &mut profile.natives[active.native];
        stats.calls += 1;
        stats.inclusive_ns = stats.inclusive_ns.saturating_add(duration);
        stats.exclusive_native_ns = stats
            .exclusive_native_ns
            .saturating_add(duration.saturating_sub(active.children_ns));
        stats.max_ns = stats.max_ns.max(duration);
        if let Some(caller) = active.caller {
            let caller = stats.callers.entry(caller).or_default();
            caller.calls += 1;
            caller.inclusive_ns = caller.inclusive_ns.saturating_add(duration);
        }
        if duration >= profile.min_span_ns {
            if profile.has_room() {
                profile.spans.push(NativeSpan {
                    native: active.native,
                    start_ns: active.start_ns,
                    duration_ns: duration,
                    caller: active.caller,
                });
            } else {
                profile.dropped_events += 1;
            }
        }
    }

    pub(crate) fn profile_sample(&mut self) {
        let Some(profile) = self.profile.as_mut() else {
            return;
        };
        profile.next_sample = self.instructions.saturating_add(profile.interval);
        if !profile.has_room() {
            profile.dropped_events += 1;
            return;
        }
        let timestamp_ns = (profile.clock)();
        let mut stack = Vec::with_capacity(self.ci.len());
        for ci in &self.ci {
            let irep = &self.ireps[ci.irep];
            let line = irep.line_of(ci.pc).unwrap_or(0);
            let key = (ci.irep, ci.mid, line);
            let id = if let Some(id) = profile.frame_ids.get(&key) {
                *id
            } else {
                let name = format!(
                    "{}:{}:{}",
                    irep.filename.as_deref().unwrap_or("<bytecode>"),
                    line,
                    ci.mid
                        .map(|m| self.syms.name_str(m))
                        .unwrap_or_else(|| "<top>".into())
                );
                let id = profile.frames.len();
                profile.frames.push(name);
                profile.frame_ids.insert(key, id);
                id
            };
            stack.push(id);
        }
        profile.samples.push(Sample {
            timestamp_ns,
            stack,
        });
    }
}
