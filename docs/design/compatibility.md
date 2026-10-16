# Compatibility policy: when SabiRuby does not follow the reference

Decided by the author on 2026-10-16.

## The target

SabiRuby implements the behaviour of mruby 4.1: the `stable` branch, today 4.1.0-rc2
(Docker `kishima/mruby:4.1.0-rc2`). Behaviour that changed only on mruby's `master` after the
stable branch point (`4e2bc5c2b`) is not followed. This holds until 4.1 is released and the
book *Deep dive into mruby* is published; after that SabiRuby follows the latest mruby at
regular intervals (`../backlog.md`, "追従する本家のバージョン").

## The rule

Follow the reference. Every behaviour is checked against the reference binary, not against
memory or against CRuby.

The one exception: **when the reference's behaviour is clearly a defect**, SabiRuby takes the
behaviour that is evidently right (usually CRuby's), and records the difference. "Clearly a
defect" means one of:

- **It relies on undefined behaviour**: it reads freed memory, crashes, or its answer depends on
  what happens to be in memory. Example: in rc2, an `eql?` that deletes the entry being matched
  makes the Hash lookup read the vacated slot.
- **It is inconsistent within mruby itself**: the same kind of method answers differently, or an
  argument is converted from the wrong position. Example: rc2's `aget_index` converts the
  method's first argument, so `a[0, nil] = 1` succeeds as `[1, 1]` instead of raising.
- **mruby itself fixed it later as a bug** (a master commit that says so).

It is **not** an exception when mruby differs from CRuby on purpose, even if CRuby's behaviour
looks better: string handling, features left out of the build, the wording of messages, what
the ISO standard specifies. SabiRuby exists to check what the book says about mruby; a
difference there would make SabiRuby and the book disagree.

When it is unclear whether a behaviour is a defect or a design choice, the author decides.
Implementers stop and report such a case instead of choosing.

## How a deviation is recorded

Each deviation taken under the exception is recorded in three places:

1. `README.md`, "Known deviations": what differs, the reference code (file and line or
   function), what CRuby does, and why this side was taken.
2. A test in `tests/custom/` whose `.expected` is the behaviour taken. Its header names where
   the expectation comes from (CRuby, or the mruby master commit), and the reference's answer is
   left as a comment.
3. If the case is worth a mention in the book, one line in `book_mruby3/docs/notes/`.

The intended deviations that predate this policy (a NaN has no identity because Floats are
immediates, the hash pattern whose keys move the subject, the five string methods of
`design/utf8.md`) are listed in the same README section.
