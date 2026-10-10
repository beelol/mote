# Matrix toolchain design

## Decision

The owner's instruction on 2026-10-10 activates Matrix implementation and confirms the four
defaults in `matrix/decisions.md` without changes. Matrix remains the source language. The existing
v0 JSON tree remains the first consumer contract. Unsupported Matrix semantics reject with a
structured diagnostic until a versioned consumer contract carries them losslessly.

## Goal

Build a host-side Matrix toolchain that parses the frozen language, formats it canonically, resolves
host-owned bindings and actions, and lowers the documented nine-element subset to the unchanged v0
tree. It must fail closed with stable diagnostics and must never replace a previous valid surface
with an incomplete or rejected candidate.

## Architecture

Create a dependency-light `matrix-lang` workspace crate. A hand-written lexer records byte spans and
feeds a recursive-descent parser. The parser produces a semantic AST for declarations, references,
modifiers, blocks and actions. A canonical formatter writes only authorized syntax.

Resolution consumes a versioned host manifest. The manifest owns Value, Field and Action types. The
resolver checks scope, field ownership, collection projections, selection, action arguments and
literal constraints. It returns a typed tree or structured diagnostics. The v0 lowerer accepts only
the subset documented in `matrix/runtime.md` and returns `mote_ui::Node`. Other valid Matrix elements
return `E_LOWER_UNSUPPORTED` rather than approximate behavior.

Streaming uses a candidate accumulator that distinguishes incomplete input from rejected input.
Only a fully parsed, resolved and lowered candidate can become the committed surface. The host keeps
the last committed surface on either outcome.

## Public interfaces

- `parse_surface(&str) -> Result<Surface, Vec<Diagnostic>>`
- `parse_fragment(&str) -> Result<Vec<Declaration>, Vec<Diagnostic>>`
- `format_surface(&Surface) -> String`
- `format_fragment(&[Declaration]) -> String`
- `resolve(&Surface, &Manifest, &BindingSnapshot) -> Result<ResolvedSurface, Vec<Diagnostic>>`
- `lower_v0(&ResolvedSurface) -> Result<mote_ui::Node, Vec<Diagnostic>>`
- `CandidateStream::push(&mut self, &[u8]) -> CandidateStatus`

`Diagnostic` contains a stable code, severity, byte span, declaration path, message and optional
hint. Syntax and semantic failures may report more than one diagnostic only when later diagnostics
do not depend on guessed recovery.

## Bounds and safety

Source is valid UTF-8 and at most 256 KiB. Parsed and expanded trees are at most 12 declarations deep
and 2,000 nodes. Numeric literals are finite JSON numbers. Unknown syntax, bindings, actions, icons,
modifiers and trailing input reject the whole candidate. Matrix contains no scripts, expressions,
network access or model calls.

## Delivery sequence

1. Lexer, source spans and diagnostic types.
2. Closed AST and parser for the full frozen grammar.
3. Canonical formatter and streaming completion classification.
4. Host manifest, scope resolution and type checking.
5. Exact v0 subset lowering and conformance snapshots.
6. Host candidate commitment and client delivery.
7. Versioned full-catalog consumer contract and runtime.

Each slice is test-driven, independently reviewed, committed to `main` and pushed after the full
workspace test and clippy contract passes.

## Deferred evidence, not deferred code

Native screenshot checks require an unlocked Mac. Pi and iPad measurements require their hardware.
Those facts can leave acceptance evidence open, but they do not stop parser, compiler, host, runtime
or headless rendering work.
