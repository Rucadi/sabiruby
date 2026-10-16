//! The Hash table: mruby 4.1.0-rc2's `src/hash.c` written again in Rust.
//!
//! A Hash answers `[]`, `[]=`, `delete` and the scans in the same order as the reference does,
//! because the order is visible: a key that is not a String, Symbol, Integer or Float answers
//! `eql?` itself, and a Ruby `eql?` can count its calls, see which entries it is compared with,
//! and change the Hash under the lookup. The reference keeps its entries in an "entry array"
//! (EA) in insertion order, and
//!
//! - up to 16 entries (`AR_MAX_SIZE`) it has no index ("AR"): a lookup walks every entry and
//!   compares the key with each, `eql?` included, without asking the key's `hash` at all;
//! - past that ("HT") it keeps "index buckets" (IB), an open-addressing table of positions in
//!   the EA, probed from the key's hash code by triangular steps; every live entry met on the
//!   way is compared, whatever its own code is (the table does not keep codes).
//!
//! When the table is rebuilt (growing, compacting, AR to HT, `rehash`) every key is hashed
//! again, so a key's `hash` is asked then too.
//!
//! What is the reference's and what is SabiRuby's:
//!
//! - The EA is kept here without its holes: `entries` holds the live entries in order, so the
//!   rest of the VM reads a slice as before. What a hole meant to the reference is kept as
//!   numbers: `slots` is each entry's position in the reference's EA, `n_used` its used length
//!   and `ea_capa` its capacity, and they decide when the table is compacted and rebuilt
//!   exactly where the reference decides it. The buckets hold positions in `entries`; a removal
//!   moves the later ones down one, so it rewrites the buckets past it (O(capacity), as the
//!   `Vec::remove` beside it is O(n)).
//! - The modification check (`H_CHECK_MODIFIED`) is `HCheck`: the mode, the bucket width, the
//!   EA capacity and the live count, plus the key in the EA's first slot in AR mode, which is
//!   what the reference's check reads there on a 64-bit word-boxing build (it reads
//!   `h->hsh.ht->ea` through the AR's entry pointer); a change of the EA's address with no
//!   change of capacity, which the reference may or may not see depending on the allocator, is
//!   seen here as `epoch` (`replace`, `clear`).
//! - Hash codes are `mrb_obj_hash_code`'s, with one exception: a Symbol hashes by its number
//!   here. rc2 hashes it by `mrb_fixnum()` of the boxed word, which leaves one bit of the
//!   symbol (master changed that in 28d59b911), and SabiRuby's symbol numbers are not the
//!   reference's anyway, so which entries a lookup meets beside a Symbol key cannot be the same.
//!
//! The constants below are the reference's (src/hash.c): they are what makes the order the
//! reference's, so they are not settings.

use alloc::vec::Vec;

use crate::value::{Slot, Value};

/// Entries up to which a Hash has no index (`AR_MAX_SIZE`).
pub const AR_MAX_SIZE: u32 = 16;
/// The smallest EA capacity (`AR_DEFAULT_CAPA`).
const AR_DEFAULT_CAPA: u32 = 4;
/// The bucket width a Hash starts with when it gets an index (`IB_INIT_BIT`, which the
/// reference derives from `AR_MAX_SIZE`: 2^5 buckets are the fewest whose 3/4 exceeds 16).
const IB_INIT_BIT: u32 = 5;
/// Bucket values the reference reserves for "empty" and "deleted" (`EA_N_RESERVED_INDICES`).
const EA_N_RESERVED_INDICES: u32 = 2;
/// The most the EA grows by at once (`EA_MAX_INCREASE`).
const EA_MAX_INCREASE: u32 = u16::MAX as u32;
/// `EA_MAX_CAPA` on a 64-bit build: `IB_MAX_CAPA - EA_N_RESERVED_INDICES`, IB_MAX_CAPA = 2^31.
pub const EA_MAX_CAPA: u32 = (1u32 << 31) - EA_N_RESERVED_INDICES;
/// An empty bucket.
pub(crate) const EMPTY: u32 = u32::MAX;
/// A bucket whose entry was deleted.
pub(crate) const DELETED: u32 = u32::MAX - 1;

/// A Hash: its entries in insertion order, the reference's bookkeeping of them, and the value
/// `Hash#default` answers with. See the module documentation.
#[derive(Clone, Default)]
pub struct HashData {
    /// The live entries in insertion order.
    entries: Vec<(Slot, Slot)>,
    /// Each entry's position in the reference's entry array (ascending).
    slots: Vec<u32>,
    /// The reference's `ea_n_used`: slots used, live or deleted.
    n_used: u32,
    /// The reference's `ea_capa`.
    ea_capa: u32,
    /// `log2` of the bucket count, 0 while the Hash has no index (AR).
    ib_bit: u32,
    /// The buckets: a position in `entries`, [`EMPTY`] or [`DELETED`].
    ib: Vec<u32>,
    /// Bumped where the reference allocates the table afresh without changing what the
    /// modification check otherwise reads (`h_replace`, `h_clear`).
    epoch: u32,
    pub default: Slot,
}

/// What `H_CHECK_MODIFIED` compares before and after a call into Ruby.
#[derive(Clone, Copy)]
pub(crate) struct HCheck {
    bit: u32,
    ea_capa: u32,
    size: u32,
    epoch: u32,
    /// AR only: the key in EA slot 0, `None` for a hole or no slot.
    first: Option<Value>,
}

impl HCheck {
    pub(crate) fn same(&self, o: &HCheck) -> bool {
        self.bit == o.bit && self.ea_capa == o.ea_capa && self.size == o.size && self.epoch == o.epoch
            && match (self.first, o.first) {
                (None, None) => true,
                (Some(a), Some(b)) => same_word(a, b),
                _ => false,
            }
    }
}

/// `mrb_obj_eq`: the same value, a Float by its bits (a NaN is itself).
pub(crate) fn same_word(a: Value, b: Value) -> bool {
    match (a, b) {
        (Value::Float(x), Value::Float(y)) => x.to_bits() == y.to_bits(),
        _ => a == b,
    }
}

/// `ea_next_capa_for`.
fn ea_next_capa_for(size: u32, max_capa: u32) -> u32 {
    if size < AR_DEFAULT_CAPA { return AR_DEFAULT_CAPA; }
    let size = size as u64;
    let mut capa = size * 6 / 5 + 6;
    if capa - size > EA_MAX_INCREASE as u64 { capa = size + EA_MAX_INCREASE as u64; }
    if capa <= max_capa as u64 { capa as u32 } else { max_capa }
}

/// `ib_upper_bound_for`: 3/4 of a bucket count.
fn ib_upper_bound_for(capa: u32) -> u32 { (capa >> 2) | (capa >> 1) }

/// `ib_bit_for`: the bucket width for `size` entries.
pub(crate) fn ib_bit_for(size: u32) -> u32 {
    // `next_power2`: the power of two above `size`, even when `size` is one already
    let mut capa = 1u32 << (32 - size.leading_zeros()).min(31);
    if capa != 1u32 << 31 && ib_upper_bound_for(capa) < size { capa *= 2; }
    capa.trailing_zeros()
}

/// The `n`th bucket of the probe from `code` (`ib_it_next`): the triangular numbers.
#[inline]
pub(crate) fn probe_at(code: u32, n: u32, mask: u32) -> usize {
    (code.wrapping_add(n.wrapping_mul(n).wrapping_add(n) / 2) & mask) as usize
}

/// The final mix of `mrb_obj_hash_code`.
#[inline]
pub(crate) fn mix(mut h: u32) -> u32 {
    h ^= h >> 16;
    h = h.wrapping_mul(0x45d9f3b);
    h ^= h >> 16;
    h
}

/// `mrb_byte_hash`: FNV-1a, 32 bits.
pub(crate) fn byte_hash(b: &[u8]) -> u32 {
    let mut h: u32 = 0x811c_9dc5;
    for x in b { h ^= *x as u32; h = h.wrapping_mul(0x0100_0193); }
    h
}

impl HashData {
    /// An empty Hash with room for `capa` entries (`mrb_hash_new_capa`, which `OP_HASH` uses):
    /// past [`AR_MAX_SIZE`] it starts with an index sized for them.
    pub fn with_capa(capa: usize) -> HashData {
        let mut h = HashData::default();
        if capa == 0 { return h; }
        let size = capa.min(EA_MAX_CAPA as usize) as u32;
        h.ea_capa = size;
        if size > AR_MAX_SIZE { h.ht_reset(ib_bit_for(size)); }
        h
    }
    /// The entries in insertion order.
    #[inline]
    pub fn entries(&self) -> &[(Slot, Slot)] { &self.entries }
    #[inline]
    pub fn len(&self) -> usize { self.entries.len() }
    #[inline]
    pub fn is_empty(&self) -> bool { self.entries.is_empty() }
    #[inline]
    pub(crate) fn is_ht(&self) -> bool { self.ib_bit != 0 }
    #[inline]
    pub(crate) fn mask(&self) -> u32 { (1u32 << self.ib_bit) - 1 }
    #[inline]
    pub(crate) fn bucket(&self, b: usize) -> u32 { self.ib.get(b).copied().unwrap_or(EMPTY) }
    #[inline]
    pub(crate) fn key_at(&self, p: usize) -> Option<Value> { self.entries.get(p).map(|e| e.0.get()) }
    #[inline]
    pub(crate) fn value_at(&self, p: usize) -> Option<Value> { self.entries.get(p).map(|e| e.1.get()) }
    #[inline]
    pub(crate) fn slot_at(&self, p: usize) -> u32 { self.slots[p] }
    /// Where the entry in EA slot `slot` is now, if it is still live.
    pub(crate) fn pos_of_slot(&self, slot: u32) -> Option<usize> { self.slots.binary_search(&slot).ok() }
    /// `entry_check_vacated` in the reference's terms: whether EA slot `slot` still holds `key`.
    pub(crate) fn slot_holds(&self, slot: u32, key: Value) -> Option<usize> {
        let p = self.pos_of_slot(slot)?;
        if same_word(self.entries[p].0.get(), key) { Some(p) } else { None }
    }
    pub(crate) fn check(&self) -> HCheck {
        let first = if self.is_ht() { None } else { match self.slots.first() { Some(0) => Some(self.entries[0].0.get()), _ => None } };
        HCheck { bit: self.ib_bit, ea_capa: self.ea_capa, size: self.entries.len() as u32, epoch: self.epoch, first }
    }
    pub(crate) fn n_used(&self) -> u32 { self.n_used }
    pub(crate) fn set_n_used(&mut self, n: u32) { self.n_used = n; }
    pub(crate) fn ea_capa(&self) -> u32 { self.ea_capa }
    pub(crate) fn ib_bit(&self) -> u32 { self.ib_bit }

    /// Overwrites the value of entry `p`.
    pub fn set_value_at(&mut self, p: usize, v: Slot) {
        if let Some(e) = self.entries.get_mut(p) { e.1 = v; }
    }
    /// `ea_compress`: the holes are given back and every entry's slot is its position.
    pub(crate) fn compress(&mut self) {
        for (i, s) in self.slots.iter_mut().enumerate() { *s = i as u32; }
        self.n_used = self.entries.len() as u32;
    }
    /// `ar_adjust_ea`/`ht_adjust_ea`: the capacity for `size` entries, at most `max`.
    pub(crate) fn adjust_ea(&mut self, size: u32, max: u32) { self.ea_capa = ea_next_capa_for(size, max); }
    /// `ht_init` without hashing anything: the Hash has an index of `bit` with every bucket
    /// empty. The caller places the entries ([`HashData::place`]) as `ib_init` does.
    pub(crate) fn ht_reset(&mut self, bit: u32) {
        self.ib_bit = bit;
        self.ib.clear();
        self.ib.resize(1usize << bit, EMPTY);
    }
    /// `ib_init`'s step for one entry: the first empty bucket of the probe from `code`.
    pub(crate) fn place(&mut self, code: u32, p: usize) {
        let mask = self.mask();
        for n in 0..=mask {
            let b = probe_at(code, n, mask);
            if self.ib[b] == EMPTY { self.ib[b] = p as u32; return; }
        }
    }
    /// `ht_to_ar`: drops the index and gives the EA the capacity of an AR.
    pub(crate) fn to_ar(&mut self) {
        self.compress();
        let size = self.entries.len() as u32;
        self.ea_capa = ea_next_capa_for(size, AR_MAX_SIZE);
        self.ib_bit = 0;
        self.ib = Vec::new();
    }
    /// The part of `ar_set` after the search found nothing, up to where the entry is written:
    /// `true` when the Hash has to get an index instead (it holds 16 and no hole).
    pub(crate) fn ar_make_room(&mut self) -> bool {
        let size = self.entries.len() as u32;
        if self.ea_capa == self.n_used {
            if size == self.n_used {
                if size == AR_MAX_SIZE { return true; }
                self.adjust_ea(size, AR_MAX_SIZE);
            } else {
                self.compress();
                self.adjust_ea(size, self.ea_capa.min(AR_MAX_SIZE));
            }
        }
        false
    }
    /// Appends an entry at the end of the EA (`ea_set` at `ea_n_used`, AR).
    pub(crate) fn ar_push(&mut self, k: Slot, v: Slot) {
        self.entries.push((k, v));
        self.slots.push(self.n_used);
        self.n_used += 1;
    }
    /// The part of `ht_set` before the probe. `Some(bit)` asks the caller to rebuild the index
    /// at that width (after the compress done here); `None` with `to_ar` set asks it to go back
    /// to AR and store there.
    pub(crate) fn ht_make_room(&mut self) -> HtRoom {
        let size = self.entries.len() as u32;
        let bit = self.ib_bit;
        let capa = 1u32 << bit;
        if ib_upper_bound_for(capa) <= size {
            if size != self.n_used { self.compress(); }
            return HtRoom::Rebuild(bit + 1);
        }
        if size != self.n_used {
            let compress = if capa - EA_N_RESERVED_INDICES <= self.n_used {
                true
            } else if self.ea_capa == self.n_used {
                if size <= AR_MAX_SIZE { return HtRoom::ToAr; }
                ea_next_capa_for(size, EA_MAX_CAPA) <= self.ea_capa
            } else { false };
            if compress {
                self.compress();
                let max = self.ea_capa;
                self.adjust_ea(size, max);
                return HtRoom::Rebuild(bit);
            }
        }
        HtRoom::Fits
    }
    /// Writes a new entry into bucket `b` (the end of `ht_set`'s probe).
    pub(crate) fn ht_push(&mut self, b: usize, k: Slot, v: Slot) {
        if self.n_used == self.ea_capa { self.adjust_ea(self.n_used, EA_MAX_CAPA); }
        let p = self.entries.len() as u32;
        self.entries.push((k, v));
        self.slots.push(self.n_used);
        self.n_used += 1;
        if let Some(x) = self.ib.get_mut(b) { *x = p; }
    }
    /// Removes entry `p` (`entry_delete`, and `ib_it_delete` in HT): its slot becomes a hole,
    /// its bucket a deleted one.
    pub fn remove_entry(&mut self, p: usize) -> (Slot, Slot) {
        let e = self.entries.remove(p);
        self.slots.remove(p);
        if self.is_ht() {
            let p = p as u32;
            for b in self.ib.iter_mut() {
                if *b == p { *b = DELETED; } else if *b != EMPTY && *b != DELETED && *b > p { *b -= 1; }
            }
        }
        e
    }
    /// Removes every entry whose value is nil (`mrb_hash_compact`); how many went.
    pub(crate) fn remove_nil_values(&mut self) -> usize {
        let mut n = 0;
        let mut p = 0;
        while p < self.entries.len() {
            if self.entries[p].1.is_nil() { self.remove_entry(p); n += 1; } else { p += 1; }
        }
        n
    }
    /// `h_clear`: an empty AR with no EA.
    pub fn clear(&mut self) {
        self.entries.clear();
        self.slots.clear();
        self.n_used = 0;
        self.ea_capa = 0;
        self.ib_bit = 0;
        self.ib = Vec::new();
        self.epoch = self.epoch.wrapping_add(1);
    }
    /// `h_replace`: this Hash takes a copy of `other`'s table, its holes and buckets included.
    /// The default is not touched (the caller copies it where the reference does).
    pub fn replace_table(&mut self, other: &HashData) {
        let epoch = self.epoch.wrapping_add(1);
        let default = self.default;
        *self = other.clone();
        self.epoch = epoch;
        self.default = default;
    }
    /// The position of the first entry whose slot is past `slot`.
    pub(crate) fn slots_after(&self, slot: u32) -> usize { self.slots.partition_point(|s| *s <= slot) }
    /// Puts back at `p` an entry [`HashData::remove_entry`] took out (a rehash moving one down).
    pub(crate) fn reinsert(&mut self, p: usize, e: (Slot, Slot)) {
        self.entries.insert(p, e);
        self.slots.insert(p, 0);
        if self.is_ht() {
            let p = p as u32;
            for b in self.ib.iter_mut() { if *b != EMPTY && *b != DELETED && *b >= p { *b += 1; } }
        }
    }
    /// The pieces of a rehash ([`crate::vm::Vm`]'s `h_rehash`), which moves entries down as it goes.
    pub(crate) fn set_slot(&mut self, p: usize, slot: u32) { self.slots[p] = slot; }
    pub(crate) fn set_bucket(&mut self, b: usize, p: usize) { if let Some(x) = self.ib.get_mut(b) { *x = p as u32; } }
}

/// What [`HashData::ht_make_room`] leaves to the caller.
pub(crate) enum HtRoom {
    /// There is room: probe and store.
    Fits,
    /// Rebuild the index at this width, hashing every key, then probe and store.
    Rebuild(u32),
    /// Go back to AR and store there (`ht_set_as_ar`).
    ToAr,
}

// ------------------------------------------------------------------------------------------
// The algorithm, which calls into Ruby (`hash`, `eql?`) between its steps.

use crate::error::VmResult;
use crate::object::ObjKind;
use crate::value::ObjId;
use crate::vm::Vm;

/// What a keyed search found.
pub(crate) enum Found {
    /// Entry `p` matched.
    At(usize),
    /// `eql?` said yes and then took the entry it said yes to out of the Hash (deleted it, or
    /// deleted and stored again). mruby 4.1.0-rc2 answers from the slot as the comparison left
    /// it, holding the pair it compared (its `entry_check_vacated` came after the stable
    /// branch point, a54353ecf): a read answers that value, a store writes where no entry is
    /// any more, a delete answers it and takes nothing. Kept as that answer, without reading
    /// anything that is not there.
    Vacated(Value),
    Missing,
}

/// The reference's `mrb_vtype` number of a value whose `hash` is sent (`U32(tt)` in
/// `mrb_obj_hash_code`), by what SabiRuby holds it as.
fn vtype(vm: &Vm, o: ObjId) -> u32 {
    match &vm.heap.get(o).kind {
        ObjKind::Object => 8,
        ObjKind::Class(cd) => if cd.is_singleton { 11 } else if cd.is_module { 10 } else { 9 },
        ObjKind::Hash(_) => 12,
        ObjKind::Exception => 14,
        ObjKind::Proc(_) => 16,
        ObjKind::Array(_) => 17,
        ObjKind::String(_) => 18,
        ObjKind::Range { .. } => 19,
        ObjKind::Env(_) => 20,
        ObjKind::Fiber(_) => 21,
        ObjKind::Break { .. } => 24,
        ObjKind::BigInt(_) => 27,
        // C data in the reference: a task, a Regexp, a MatchData, a host value
        _ => 13,
    }
}

impl Vm {
    #[inline]
    pub(crate) fn hd(&self, o: ObjId) -> Option<&HashData> {
        match &self.heap.get(o).kind { ObjKind::Hash(h) => Some(h), _ => None }
    }
    #[inline]
    pub(crate) fn hd_mut(&mut self, o: ObjId) -> Option<&mut HashData> {
        match &mut self.heap.get_mut(o).kind { ObjKind::Hash(h) => Some(h), _ => None }
    }
    fn h_check(&self, o: ObjId) -> Option<HCheck> { self.hd(o).map(|h| h.check()) }
    /// The end of `H_CHECK_MODIFIED`: raises when the table is not what `before` saw.
    pub(crate) fn h_verify(&mut self, o: ObjId, before: Option<HCheck>) -> VmResult<()> {
        let now = self.h_check(o);
        match (before, now) {
            (Some(b), Some(n)) if b.same(&n) => Ok(()),
            (None, None) => Ok(()),
            _ => Err(self.raise(self.core.runtime_error, "hash modified")),
        }
    }
    /// `obj_hash_code` (`mrb_obj_hash_code` under `H_CHECK_MODIFIED`): the code a Hash places
    /// and looks `key` up by. A key that is not a String, nil, true, false, Symbol, Integer or
    /// Float is sent `hash`.
    pub(crate) fn h_code(&mut self, o: ObjId, key: Value) -> VmResult<u32> {
        let code = match key {
            Value::Nil => 0,
            Value::False => 2,
            Value::True => 6,
            Value::Int(i) => i as u32,
            Value::Sym(s) => s.0,
            Value::Float(f) => { let f = if f == 0.0 { 0.0 } else { f }; byte_hash(&f.to_bits().to_le_bytes()) }
            Value::Obj(k) => {
                if let Some(b) = self.heap.string(k) { byte_hash(b) } else {
                    let tt = vtype(self, k);
                    let before = self.h_check(o);
                    let hs = self.s.hash;
                    // the caller may hold the key and the Hash in Rust locals only
                    self.native_active += 1;
                    let r = self.funcall(key, hs, &[], Value::Nil);
                    self.native_active -= 1;
                    let r = r?;
                    self.h_verify(o, before)?;
                    let n = match r { Value::Int(i) => i as u32, Value::Float(f) => f as i64 as u32, _ => 0 };
                    tt ^ n
                }
            }
        };
        Ok(mix(code))
    }
    /// `mrb_eql`: the same value, Integer, Float and String compared here, and a class whose
    /// `eql?` is the plain identity answers false without a send.
    pub(crate) fn mrb_eql(&mut self, a: Value, b: Value) -> VmResult<bool> {
        if same_word(a, b) { return Ok(true); }
        match (a, b) {
            (Value::Int(_), _) | (Value::Float(_), _) => return Ok(a == b),
            (Value::Obj(x), _) if self.heap.string(x).is_some() => {
                return Ok(matches!(b, Value::Obj(y) if self.heap.string(y).is_some() && self.heap.string(x) == self.heap.string(y)));
            }
            _ => {}
        }
        let eql = self.s.eql;
        let c = self.class_of(a);
        if let (Some(basic), Some((crate::object::MethodRef::Native(f), _))) = (self.eql_basic, self.find_method_cached(c, eql)) {
            // `Object#eql?` is the identity, which `same_word` already answered; a value
            // `Vm::data_new` made answers by what it names
            if core::ptr::fn_addr_eq(f, basic) { return Ok(self.same_value(a, b)); }
        }
        self.native_active += 1;
        let r = self.funcall(a, eql, &[b], Value::Nil);
        self.native_active -= 1;
        Ok(r?.truthy())
    }
    /// `obj_eql`: `a` is the key asked about, `b` the one stored. A String, Symbol, Integer or
    /// Float key compares here; anything else answers `mrb_eql` under the modification check.
    pub(crate) fn h_obj_eql(&mut self, o: ObjId, a: Value, b: Value) -> VmResult<bool> {
        match a {
            Value::Sym(_) | Value::Int(_) => Ok(a == b),
            Value::Float(x) => Ok(match b { Value::Float(y) => x == y || (x != x && x.to_bits() == y.to_bits()), _ => false }),
            Value::Obj(x) if self.heap.string(x).is_some() => {
                Ok(matches!(b, Value::Obj(y) if self.heap.string(y).is_some() && self.heap.string(x) == self.heap.string(y)))
            }
            _ => {
                let before = self.h_check(o);
                let r = self.mrb_eql(a, b)?;
                self.h_verify(o, before)?;
                Ok(r)
            }
        }
    }
    /// The comparison of a keyed search against entry `p`: [`Found::At`] where it matched,
    /// what the match left where `eql?` took the entry away.
    fn h_compare(&mut self, o: ObjId, key: Value, p: usize) -> VmResult<Option<Found>> {
        let (stored, slot, val) = match self.hd(o) { Some(h) if p < h.len() => (h.key_at(p).unwrap(), h.slot_at(p), h.value_at(p).unwrap()), _ => return Ok(None) };
        if !self.h_obj_eql(o, key, stored)? { return Ok(None); }
        Ok(Some(match self.hd(o).and_then(|h| h.slot_holds(slot, stored)) { Some(p) => Found::At(p), None => Found::Vacated(val) }))
    }
    /// `h_get`'s search (`ar_get` / `ht_get`).
    pub(crate) fn h_find(&mut self, o: ObjId, key: Value) -> VmResult<Found> {
        if let Some(f) = self.h_find_native(o, key) { return Ok(f); }
        let ht = match self.hd(o) { Some(h) => h.is_ht(), None => return Ok(Found::Missing) };
        if !ht {
            // `EA_EACH`: the live count is read once, and the walk goes by slot
            let mut left = self.hd(o).map(|h| h.len()).unwrap_or(0);
            let mut p = 0;
            while left > 0 {
                let slot = match self.hd(o) { Some(h) if p < h.len() => h.slot_at(p), _ => break };
                if let Some(f) = self.h_compare(o, key, p)? { return Ok(f); }
                left -= 1;
                p = match self.hd(o) { Some(h) => h.slots_after(slot), None => break };
            }
            return Ok(Found::Missing);
        }
        let code = self.h_code(o, key)?;
        let mask = match self.hd(o) { Some(h) if h.is_ht() => h.mask(), _ => return Ok(Found::Missing) };
        for n in 0..=mask {
            let b = probe_at(code, n, mask);
            let v = match self.hd(o) { Some(h) => h.bucket(b), None => EMPTY };
            if v == EMPTY { break; }
            if v == DELETED { continue; }
            if let Some(f) = self.h_compare(o, key, v as usize)? { return Ok(f); }
        }
        Ok(Found::Missing)
    }
    /// Rebuilds the index at width `bit` (`ht_init`'s `ib_init`): every key is hashed again,
    /// in order, and placed in the first empty bucket of its probe.
    fn h_rebuild(&mut self, o: ObjId, bit: u32) -> VmResult<()> {
        if let Some(h) = self.hd_mut(o) { h.ht_reset(bit); let n = h.len() as u32; h.set_n_used(n); }
        if self.h_rebuild_native(o) { return Ok(()); }
        let mut p = 0;
        loop {
            let key = match self.hd(o).and_then(|h| h.key_at(p)) { Some(k) => k, None => break };
            let code = self.h_code(o, key)?;
            if let Some(h) = self.hd_mut(o) { if p < h.len() { h.place(code, p); } }
            p += 1;
        }
        Ok(())
    }
    /// `h_key_for`: an unfrozen String key is stored as a frozen copy.
    fn h_key_for(&mut self, k: Value) -> Value {
        match k {
            Value::Obj(ko) if self.heap.string(ko).is_some() && !self.heap.get(ko).frozen => {
                let b = self.heap.string(ko).unwrap().to_vec();
                let n = self.heap.alloc(self.core.string, ObjKind::String(b));
                self.heap.get_mut(n).frozen = true;
                Value::Obj(n)
            }
            _ => k,
        }
    }
    /// `h_set` (`ar_set` / `ht_set`).
    pub(crate) fn h_set(&mut self, o: ObjId, key: Value, val: Value) -> VmResult<()> {
        let ht = match self.hd(o) { Some(h) => h.is_ht(), None => return Ok(()) };
        if !ht {
            match self.h_find(o, key)? {
                Found::At(p) => { if let Some(h) = self.hd_mut(o) { h.set_value_at(p, Slot::from(val)); } return Ok(()); }
                Found::Vacated(_) => return Ok(()),
                Found::Missing => {}
            }
            let k = self.h_key_for(key);
            let to_ht = match self.hd_mut(o) { Some(h) => h.ar_make_room(), None => return Ok(()) };
            if to_ht {
                self.h_rebuild(o, IB_INIT_BIT)?;
                return self.ht_set(o, k, val);
            }
            if let Some(h) = self.hd_mut(o) { h.ar_push(Slot::from(k), Slot::from(val)); }
            return Ok(());
        }
        self.ht_set(o, key, val)
    }
    fn ht_set(&mut self, o: ObjId, key: Value, val: Value) -> VmResult<()> {
        let room = match self.hd_mut(o) { Some(h) => h.ht_make_room(), None => return Ok(()) };
        match room {
            HtRoom::Fits => {}
            HtRoom::Rebuild(bit) => self.h_rebuild(o, bit)?,
            HtRoom::ToAr => {
                if let Some(h) = self.hd_mut(o) { h.to_ar(); }
                return self.h_set(o, key, val);
            }
        }
        if let Some(r) = self.hd(o).and_then(|h| h.probe_native(&self.heap, key)) {
            match r {
                Ok(p) => { if let Some(h) = self.hd_mut(o) { h.set_value_at(p, Slot::from(val)); } }
                Err(b) => {
                    if self.hd(o).map(|h| h.n_used()) == Some(EA_MAX_CAPA) { return Err(self.raise_arg("hash too big")); }
                    let k = self.h_key_for(key);
                    if let Some(h) = self.hd_mut(o) { h.ht_push(b, Slot::from(k), Slot::from(val)); }
                }
            }
            return Ok(());
        }
        let code = self.h_code(o, key)?;
        let mask = match self.hd(o) { Some(h) if h.is_ht() => h.mask(), _ => return Ok(()) };
        for n in 0..=mask {
            let b = probe_at(code, n, mask);
            let v = match self.hd(o) { Some(h) => h.bucket(b), None => return Ok(()) };
            if v == DELETED { continue; }
            if v == EMPTY {
                if self.hd(o).map(|h| h.n_used()) == Some(EA_MAX_CAPA) { return Err(self.raise_arg("hash too big")); }
                let k = self.h_key_for(key);
                if let Some(h) = self.hd_mut(o) { h.ht_push(b, Slot::from(k), Slot::from(val)); }
                return Ok(());
            }
            match self.h_compare(o, key, v as usize)? {
                Some(Found::At(p)) => { if let Some(h) = self.hd_mut(o) { h.set_value_at(p, Slot::from(val)); } return Ok(()); }
                Some(_) => return Ok(()),
                None => {}
            }
        }
        Ok(())
    }
    /// `h_delete`: the value the key had.
    pub(crate) fn h_delete(&mut self, o: ObjId, key: Value) -> VmResult<Option<Value>> {
        match self.h_find(o, key)? {
            Found::At(p) => Ok(self.hd_mut(o).map(|h| h.remove_entry(p).1.get())),
            Found::Vacated(v) => Ok(Some(v)),
            Found::Missing => Ok(None),
        }
    }
    /// `h_shift`: takes the first entry. With an index the reference finds its bucket by the
    /// key's hash code, so the key is asked for it.
    pub(crate) fn h_shift(&mut self, o: ObjId) -> VmResult<Option<(Value, Value)>> {
        let (ht, key) = match self.hd(o) { Some(h) if !h.is_empty() => (h.is_ht(), h.key_at(0).unwrap()), _ => return Ok(None) };
        if ht { let _ = self.h_code(o, key)?; }
        Ok(self.hd_mut(o).and_then(|h| if h.is_empty() { None } else { let (k, v) = h.remove_entry(0); Some((k.get(), v.get())) }))
    }
    /// `h_rehash` (`ar_rehash` / `ht_rehash`): every entry is stored again in order, a later
    /// duplicate giving its value to the earlier one.
    pub(crate) fn h_rehash(&mut self, o: ObjId) -> VmResult<()> {
        let (len, ht) = match self.hd(o) { Some(h) => (h.len() as u32, h.is_ht()), None => return Ok(()) };
        if len == 0 { if let Some(h) = self.hd_mut(o) { h.clear(); } return Ok(()); }
        if ht && len <= AR_MAX_SIZE { if let Some(h) = self.hd_mut(o) { h.to_ar(); } }
        let ht = self.hd(o).map(|h| h.is_ht()).unwrap_or(false);
        let ea_capa = self.hd(o).map(|h| h.ea_capa()).unwrap_or(0);
        if ht {
            // `ht_init(h, 0, ...)`: an empty index for the size, nothing used yet
            let bit = ib_bit_for(len);
            if let Some(h) = self.hd_mut(o) { h.ht_reset(bit); h.set_n_used(0); }
        }
        let mut w = 0usize;
        let mut left = len;
        while left > 0 {
            left -= 1;
            // entry `w` is the next one to move: the ones before it are written
            let (key, slot) = match self.hd(o) { Some(h) if w < h.len() => (h.key_at(w).unwrap(), h.slot_at(w)), _ => break };
            let dup = if ht {
                let code = self.h_code(o, key)?;
                let mask = self.hd(o).map(|h| h.mask()).unwrap_or(0);
                let mut res = Err(0usize);
                for n in 0..=mask {
                    let b = probe_at(code, n, mask);
                    let v = self.hd(o).map(|h| h.bucket(b)).unwrap_or(EMPTY);
                    if v == EMPTY || v == DELETED { res = Err(b); break; }
                    match self.h_compare(o, key, v as usize)? {
                        Some(Found::At(p)) => { res = Ok(Some(p)); break; }
                        Some(_) => { res = Ok(None); break; }
                        None => {}
                    }
                }
                res
            } else {
                let mut res = Err(0usize);
                for j in 0..w {
                    match self.h_compare(o, key, j)? {
                        Some(Found::At(p)) => { res = Ok(Some(p)); break; }
                        Some(_) => { res = Ok(None); break; }
                        None => {}
                    }
                }
                res
            };
            // the entry being moved, where the comparisons left it
            let Some(r) = self.hd(o).and_then(|h| h.pos_of_slot(slot)) else { continue };
            match dup {
                Ok(Some(p)) => {
                    if let Some(h) = self.hd_mut(o) { let v = h.entries()[r].1; h.set_value_at(p, v); h.remove_entry(r); }
                }
                Ok(None) => {}
                Err(b) => {
                    if let Some(h) = self.hd_mut(o) {
                        if r != w { let e = h.remove_entry(r); h.reinsert(w, e); }
                        h.set_slot(w, w as u32);
                        if ht { h.set_bucket(b, w); }
                    }
                    w += 1;
                }
            }
        }
        if let Some(h) = self.hd_mut(o) {
            let size = h.len() as u32;
            h.set_n_used(size);
            if ht && size <= AR_MAX_SIZE { h.to_ar(); } else { h.adjust_ea(size, ea_capa); }
        }
        Ok(())
    }
    /// `H_EACH`'s step: the entry after EA slot `slot`, or the first one.
    pub(crate) fn h_next(&self, o: ObjId, after: Option<u32>) -> Option<usize> {
        let h = self.hd(o)?;
        let p = match after { Some(s) => h.slots_after(s), None => 0 };
        if p < h.len() { Some(p) } else { None }
    }
}

// ------------------------------------------------------------------------------------------
// Set: mruby-set's khash (include/mruby/khash.h of 4.1.0-rc2) on the same storage.
//
// A Set holds its elements as a Hash-shaped `HashData` (element => true). Its lookups follow
// khash: up to `KHASH_SMALL_LIMIT` elements there is no table and every element is compared
// with the one asked about, the stored element answering `eql?`; the fifth element builds a
// table of `KHASH_MIN_SIZE` buckets, which is probed from `mrb_obj_hash_code` by steps of one
// more each time and grows when the live count reaches 7/8 of it. A rebuild moves the table
// (`h->data`), and a lookup whose `eql?` caused one raises (`KHASH_CHECK_MODIFIED`). The
// buckets are this file's `ib`, holding positions in `entries`; the elements are iterated in
// the order they were added, where the reference iterates its buckets (a difference this port
// keeps from before).

/// `KHASH_SMALL_LIMIT`.
const KHASH_SMALL_LIMIT: usize = 4;
/// `KHASH_MIN_SIZE`, as a power of two.
const KHASH_MIN_BIT: u32 = 3;

impl HashData {
    /// `KH_UPPER_BOUND`: 7/8 of the buckets.
    fn kh_upper_bound(&self) -> usize { let n = self.ib.len(); n - (n >> 3) }
    /// A rebuild moved the table (`h->data` changes).
    pub(crate) fn bump_gen(&mut self) { self.epoch = self.epoch.wrapping_add(1); }
    pub(crate) fn kh_mark(&self) -> (u32, u32) { (self.epoch, self.ib_bit) }
}

impl Vm {
    /// `kset_hash_value`: `mrb_obj_hash_code`, with a `hash` that raises counting as 0 and
    /// the element still added or looked for (the reference keeps the exception in `mrb->exc`).
    fn kset_hash(&mut self, o: ObjId, k: Value) -> u32 {
        // the Set's own modification check is the khash one, so the Hash's is not applied
        let saved = self.hd(o).map(|h| h.check());
        let r = self.h_code_unchecked(k);
        let _ = saved;
        r.unwrap_or(0)
    }
    /// `mrb_obj_hash_code` without `H_CHECK_MODIFIED`.
    fn h_code_unchecked(&mut self, key: Value) -> VmResult<u32> {
        if let Value::Obj(k) = key {
            if self.heap.string(k).is_none() {
                let tt = vtype(self, k);
                let hs = self.s.hash;
                self.native_active += 1;
                let r = self.funcall(key, hs, &[], Value::Nil);
                self.native_active -= 1;
                let n = match r? { Value::Int(i) => i as u32, Value::Float(f) => f as i64 as u32, _ => 0 };
                return Ok(mix(tt ^ n));
            }
        }
        // the immediates and Strings never run Ruby, so any Hash will do for the check
        let code = match key {
            Value::Nil => 0,
            Value::False => 2,
            Value::True => 6,
            Value::Int(i) => i as u32,
            Value::Sym(s) => s.0,
            Value::Float(f) => { let f = if f == 0.0 { 0.0 } else { f }; byte_hash(&f.to_bits().to_le_bytes()) }
            Value::Obj(k) => byte_hash(self.heap.string(k).unwrap_or(&[])),
        };
        Ok(mix(code))
    }
    /// `kset_equal_value(stored, key)`: the stored element answers. What `eql?` raises reaches
    /// the caller (`mrb->exc` raised once the method returns).
    fn kset_equal(&mut self, a: Value, b: Value) -> VmResult<bool> {
        match a {
            Value::Sym(_) | Value::Int(_) => Ok(a == b),
            Value::Float(x) => Ok(matches!(b, Value::Float(y) if x == y)),
            Value::Obj(x) if self.heap.string(x).is_some() => {
                Ok(matches!(b, Value::Obj(y) if self.heap.string(y).is_some() && self.heap.string(x) == self.heap.string(y)))
            }
            _ => self.mrb_eql(a, b),
        }
    }
    fn kh_verify(&mut self, o: ObjId, mark: Option<(u32, u32)>) -> VmResult<()> {
        if self.hd(o).map(|h| h.kh_mark()) != mark {
            return Err(self.raise(self.core.runtime_error, "hash table modified during callback"));
        }
        Ok(())
    }
    /// `kh_get`: the position of `k`, if it is an element.
    fn kh_get(&mut self, o: ObjId, k: Value) -> VmResult<Option<usize>> {
        let table = match self.hd(o) { Some(h) => h.is_ht(), None => return Ok(None) };
        if !table {
            let mut i = 0;
            loop {
                let Some(stored) = self.hd(o).and_then(|h| h.key_at(i)) else { return Ok(None) };
                let mark = self.hd(o).map(|h| h.kh_mark());
                let eq = self.kset_equal(stored, k)?;
                self.kh_verify(o, mark)?;
                if eq { return Ok(Some(i)); }
                i += 1;
            }
        }
        let mark = self.hd(o).map(|h| h.kh_mark());
        let code = self.kset_hash(o, k);
        self.kh_verify(o, mark)?;
        let n = self.hd(o).map(|h| h.ib.len()).unwrap_or(0);
        let mask = (n as u32).wrapping_sub(1);
        let mut b = code & mask;
        let mut step = 0u32;
        for _ in 0..n {
            let v = self.hd(o).map(|h| h.bucket(b as usize)).unwrap_or(EMPTY);
            if v == EMPTY { break; }
            if v != DELETED {
                let stored = self.hd(o).and_then(|h| h.key_at(v as usize)).unwrap_or(Value::Nil);
                let eq = self.kset_equal(stored, k)?;
                self.kh_verify(o, mark)?;
                if eq { return Ok(Some(v as usize)); }
            }
            step += 1;
            b = b.wrapping_add(step) & mask;
        }
        Ok(None)
    }
    /// `kh__rebuild`: a table of `bit` holding every element again, each hashed in turn.
    fn kh_rebuild(&mut self, o: ObjId, bit: u32) {
        if let Some(h) = self.hd_mut(o) { h.ht_reset(bit); h.bump_gen(); }
        let mut p = 0;
        while let Some(k) = self.hd(o).and_then(|h| h.key_at(p)) {
            let code = self.kset_hash(o, k);
            if let Some(h) = self.hd_mut(o) {
                if p < h.len() {
                    let mask = h.mask();
                    let (mut b, mut step) = (code & mask, 0u32);
                    for _ in 0..h.ib.len() {
                        if h.ib[b as usize] == EMPTY { h.ib[b as usize] = p as u32; break; }
                        step += 1;
                        b = b.wrapping_add(step) & mask;
                    }
                }
            }
            p += 1;
        }
    }
    /// `kh_put` for an element known not to be there: where it goes.
    fn kh_put_new(&mut self, o: ObjId, k: Value) -> VmResult<()> {
        let (table, len) = match self.hd(o) { Some(h) => (h.is_ht(), h.len()), None => return Ok(()) };
        if !table {
            if len >= KHASH_SMALL_LIMIT { self.kh_rebuild(o, KHASH_MIN_BIT); } else {
                if let Some(h) = self.hd_mut(o) { h.ar_push(Slot::from(k), Slot::from(Value::True)); }
                return Ok(());
            }
        }
        if let Some(h) = self.hd(o) { if h.len() >= h.kh_upper_bound() { let bit = h.ib_bit() + 1; self.kh_rebuild(o, bit); } }
        loop {
            let code = self.kset_hash(o, k);
            let Some(h) = self.hd(o) else { return Ok(()) };
            let n = h.ib.len();
            let mask = (n as u32).wrapping_sub(1);
            let (mut b, mut step, mut del) = (code & mask, 0u32, None);
            let mut empty = None;
            for _ in 0..n {
                let v = h.bucket(b as usize);
                if v == EMPTY { empty = Some(b); break; }
                if v == DELETED && del.is_none() { del = Some(b); }
                step += 1;
                b = b.wrapping_add(step) & mask;
            }
            let at = match (del, empty) {
                (Some(d), _) => d,
                (None, Some(e)) => e,
                // a whole round without an empty bucket or a deleted one: rebuilt at the size
                (None, None) => { let bit = h.ib_bit(); self.kh_rebuild(o, bit); continue; }
            };
            if let Some(h) = self.hd_mut(o) { let p = h.len(); h.ar_push(Slot::from(k), Slot::from(Value::True)); h.set_bucket(at as usize, p); }
            return Ok(());
        }
    }
    /// `Set#include?`.
    pub(crate) fn set_contains(&mut self, s: Value, k: Value) -> VmResult<bool> {
        match s.obj() { Some(o) if self.hd(o).is_some() => Ok(self.kh_get(o, k)?.is_some()), _ => Ok(false) }
    }
    /// `Set#add`: adds `k` unless an element is `eql?` to it. An unfrozen String is stored as a
    /// frozen copy, as a Hash key is.
    pub(crate) fn set_insert(&mut self, s: Value, k: Value) -> VmResult<()> {
        let o = match s.obj() { Some(o) if self.hd(o).is_some() => o, _ => return Err(self.raise_type("not a set")) };
        if self.heap.get(o).frozen { return Err(self.frozen_error(s)); }
        if self.kh_get(o, k)?.is_some() { return Ok(()); }
        let k = self.h_key_for(k);
        self.kh_put_new(o, k)
    }
    /// `Set#delete`: removes `k`; whether it was there.
    pub(crate) fn set_remove(&mut self, s: Value, k: Value) -> VmResult<bool> {
        let o = match s.obj() { Some(o) if self.hd(o).is_some() => o, _ => return Ok(false) };
        let Some(p) = self.kh_get(o, k)? else { return Ok(false) };
        Ok(self.hd_mut(o).map(|h| { if p < h.len() { h.remove_entry(p); true } else { false } }).unwrap_or(false))
    }
}

// ------------------------------------------------------------------------------------------
// The scans of src/hash.c that call into Ruby while they walk (`H_EACH` with
// `H_CHECK_MODIFIED` around the call), as 4.1.0-rc2 has them.

impl Vm {
    /// One step of `H_EACH`: entry after EA slot `after`, as `(key, value, slot)`.
    fn h_each_at(&self, o: ObjId, after: Option<u32>) -> Option<(Value, Value, u32)> {
        let p = self.h_next(o, after)?;
        let h = self.hd(o)?;
        Some((h.key_at(p)?, h.value_at(p)?, h.slot_at(p)))
    }
    /// The value of the entry in EA slot `slot` now, or the one read before where it is gone
    /// (rc2 reads the slot as it is).
    fn h_value_now(&self, o: ObjId, slot: u32, before: Value) -> Value {
        self.hd(o).and_then(|h| h.pos_of_slot(slot).and_then(|p| h.value_at(p))).unwrap_or(before)
    }
    /// `mrb_hash_merge`: every pair of `h2` stored into `h1`, in order.
    pub(crate) fn h_merge(&mut self, h1: Value, h2: Value) -> VmResult<()> {
        let (Some(_), Some(o2)) = (h1.obj(), h2.obj()) else { return Ok(()) };
        if h1 == h2 { return Ok(()); }
        let mut left = self.hd(o2).map(|h| h.len()).unwrap_or(0);
        let mut after = None;
        while left > 0 {
            let Some((k, v, slot)) = self.h_each_at(o2, after) else { break };
            after = Some(slot);
            left -= 1;
            let before = self.h_check(o2);
            self.hash_set(h1, k, v)?;
            self.h_verify(o2, before)?;
        }
        Ok(())
    }
    /// `h_replace`: `dst` takes a copy of `src`'s table.
    pub(crate) fn h_replace(&mut self, dst: ObjId, src: ObjId) {
        let Some(t) = self.hd(src).cloned() else { return };
        if let Some(h) = self.hd_mut(dst) { h.replace_table(&t); }
    }
    /// `mrb_hash_assoc` (`by_value` false) and `mrb_hash_rassoc` (true): the first pair whose
    /// key (value) is `eql?` to `x`, the stored one answering.
    pub(crate) fn h_assoc(&mut self, s: Value, x: Value, by_value: bool) -> VmResult<Option<(Value, Value)>> {
        let Some(o) = s.obj() else { return Ok(None) };
        let mut left = self.hd(o).map(|h| h.len()).unwrap_or(0);
        let mut after = None;
        while left > 0 {
            let Some((k, v, slot)) = self.h_each_at(o, after) else { break };
            after = Some(slot);
            left -= 1;
            if self.h_obj_eql(o, if by_value { v } else { k }, x)? {
                let v = self.h_value_now(o, slot, v);
                return Ok(Some((k, v)));
            }
        }
        Ok(None)
    }
    /// `mrb_hash_equal` (`eql` false) and `mrb_hash_eql` (true), past the size and recursion
    /// checks: each key of `h1` looked up in `h2`, the values compared.
    pub(crate) fn h_equal(&mut self, h1: Value, h2: Value, eql: bool) -> VmResult<bool> {
        let (Some(o1), Some(o2)) = (h1.obj(), h2.obj()) else { return Ok(false) };
        let mut left = self.hd(o1).map(|h| h.len()).unwrap_or(0);
        let mut after = None;
        while left > 0 {
            let Some((k, v, slot)) = self.h_each_at(o1, after) else { break };
            after = Some(slot);
            left -= 1;
            let before = self.h_check(o1);
            let found = match self.h_find(o2, k)? {
                Found::At(p) => self.hd(o2).and_then(|h| h.value_at(p)),
                Found::Vacated(w) => Some(w),
                Found::Missing => None,
            };
            self.h_verify(o1, before)?;
            let Some(w) = found else { return Ok(false) };
            let v = self.h_value_now(o1, slot, v);
            let before = self.h_check(o1);
            let same = if eql { self.mrb_eql(v, w)? } else { self.equal(v, w)? };
            self.h_verify(o1, before)?;
            if !same { return Ok(false); }
        }
        Ok(true)
    }
    /// `mrb_hash_except_keys` (`__except`, pattern matching's `**rest`).
    pub(crate) fn h_except(&mut self, s: Value, keys: Value) -> VmResult<Value> {
        let result = self.hash_new();
        let Some(o) = s.obj() else { return Ok(result) };
        let mut left = self.hd(o).map(|h| h.len()).unwrap_or(0);
        let mut after = None;
        while left > 0 {
            let Some((k, v, slot)) = self.h_each_at(o, after) else { break };
            after = Some(slot);
            left -= 1;
            let mut found = false;
            let mut i = 0;
            // `keys` is read afresh each time: Ruby in `==` can change it
            while let Some(x) = self.ary_vals(keys).and_then(|l| l.get(i).copied()) {
                let before = self.h_check(o);
                let eq = self.equal(k, x)?;
                self.h_verify(o, before)?;
                if eq { found = true; break; }
                i += 1;
            }
            if !found {
                let v = self.h_value_now(o, slot, v);
                let before = self.h_check(o);
                self.hash_set(result, k, v)?;
                self.h_verify(o, before)?;
            }
        }
        Ok(result)
    }
}

// ------------------------------------------------------------------------------------------
// The searches no Ruby can take part in. A key that is a String, Symbol, Integer or Float is
// compared without a send (`obj_eql`) and hashed without one (`mrb_obj_hash_code`), so its
// whole search runs under one borrow of the heap. It meets the same entries in the same order
// as the general search above; only the work of getting to them is less.

use crate::object::Heap;

/// The code of a key that needs no `hash` sent, `None` for one that does.
#[inline]
fn native_code(heap: &Heap, key: Value) -> Option<u32> {
    Some(mix(match key {
        Value::Int(i) => i as u32,
        Value::Sym(s) => s.0,
        Value::Float(f) => { let f = if f == 0.0 { 0.0 } else { f }; byte_hash(&f.to_bits().to_le_bytes()) }
        Value::Obj(k) => byte_hash(heap.string(k)?),
        _ => return None,
    }))
}

/// `obj_eql` for a key that answers it without Ruby, as `Vm::h_obj_eql` does.
#[inline]
fn native_eql(heap: &Heap, key: Value, kb: Option<&[u8]>, stored: Value) -> bool {
    match key {
        Value::Float(x) => match stored { Value::Float(y) => x == y || (x != x && x.to_bits() == y.to_bits()), _ => false },
        Value::Obj(_) => match (kb, stored) { (Some(a), Value::Obj(y)) => heap.string(y) == Some(a), _ => false },
        _ => key == stored,
    }
}

impl HashData {
    /// The search of `h_find` for a key no Ruby answers for; `None` when the key is not one.
    pub(crate) fn find_native(&self, heap: &Heap, key: Value) -> Option<Option<usize>> {
        let kb = match key {
            Value::Int(_) | Value::Sym(_) | Value::Float(_) => None,
            Value::Obj(k) => Some(heap.string(k)?),
            _ => return None,
        };
        if !self.is_ht() {
            return Some(self.entries.iter().position(|e| native_eql(heap, key, kb, e.0.get())));
        }
        let code = native_code(heap, key)?;
        let mask = self.mask();
        for n in 0..=mask {
            let v = self.ib[probe_at(code, n, mask)];
            if v == EMPTY { return Some(None); }
            if v == DELETED { continue; }
            if native_eql(heap, key, kb, self.entries[v as usize].0.get()) { return Some(Some(v as usize)); }
        }
        Some(None)
    }
    /// The probe of `ht_set` for a key no Ruby answers for: the entry it matches, or the empty
    /// bucket a new entry goes in.
    pub(crate) fn probe_native(&self, heap: &Heap, key: Value) -> Option<Result<usize, usize>> {
        let kb = match key { Value::Obj(k) => Some(heap.string(k)?), Value::Int(_) | Value::Sym(_) | Value::Float(_) => None, _ => return None };
        let code = native_code(heap, key)?;
        let mask = self.mask();
        for n in 0..=mask {
            let b = probe_at(code, n, mask);
            let v = self.ib[b];
            if v == EMPTY { return Some(Err(b)); }
            if v == DELETED { continue; }
            if native_eql(heap, key, kb, self.entries[v as usize].0.get()) { return Some(Ok(v as usize)); }
        }
        None
    }
}

impl Vm {
    #[inline]
    pub(crate) fn h_find_native(&self, o: ObjId, key: Value) -> Option<Found> {
        let h = self.hd(o)?;
        Some(match h.find_native(&self.heap, key)? { Some(p) => Found::At(p), None => Found::Missing })
    }
    /// `ib_init` for a table whose keys need no `hash` sent; false (nothing done) otherwise.
    fn h_rebuild_native(&mut self, o: ObjId) -> bool {
        let codes: Option<Vec<u32>> = match self.hd(o) { Some(h) => h.entries().iter().map(|e| native_code(&self.heap, e.0.get())).collect(), None => return true };
        let Some(codes) = codes else { return false };
        if let Some(h) = self.hd_mut(o) { for (p, c) in codes.into_iter().enumerate() { h.place(c, p); } }
        true
    }
}
