# osunayo

[![Rust](https://github.com/Kiy0w0/osunayo/actions/workflows/rust.yml/badge.svg)](https://github.com/Kiy0w0/osunayo/actions/workflows/rust.yml)
[![license](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![osu!lazer](https://img.shields.io/badge/osu%21lazer-2026.1005.0-ff66aa.svg)](output/info.json)
[![rustc](https://img.shields.io/badge/rustc-1.85%2B-dea584.svg?logo=rust)](https://doc.rust-lang.org/edition-guide/rust-2024/)

Static field-offset dumper for [osu!lazer](https://github.com/ppy/osu), laid out like
[a2x/cs2-dumper](https://github.com/a2x/cs2-dumper).

osunayo reads the ECMA-335 metadata of the assemblies in an osu!lazer install and computes the
CoreCLR x64 instance layout of every reference type. It never attaches to the running game, never
uses a debugger, and never reads process memory. Results land in `output/`.

## How it works

Field order is taken from the runtime, not guessed at:

- Every assembly is parsed with [`dotnetdll`](https://crates.io/crates/dotnetdll): type
  definitions, field signatures, enum underlying types, `Layout` attributes, `extends` chains.
- Offset assignment reimplements `MethodTableBuilder::HandleAutoLayout` and
  `MethodTableBuilder::PlaceInstanceFields` from
  [dotnet/runtime](https://github.com/dotnet/runtime/blob/main/src/coreclr/vm/methodtablebuilder.cpp)
  for x64 (`DATA_ALIGNMENT` 8, `MAX_LOG2_PRIMITIVE_FIELD_SIZE` 3, no offset bias):
  - gc reference fields take the front of the pointer-sized bucket, in declaration order;
  - the remaining primitive fields fill descending size buckets (8, 4, 2, 1), in declaration
    order within each bucket;
  - enum fields are normalised to their underlying primitive by the runtime, so they are placed
    as plain primitives rather than as structs;
  - real by-value struct fields are appended last, each aligned to `min(size, pointer size)`;
  - offsets are relative to the object header and receive `+8` when rendered.
- Struct sizes come from the same walk, including `Nullable<T>` and `KeyValuePair<,>`. Structs
  marked `Sequential` or `Explicit` keep declaration order and natural alignment.

## Usage

```bash
cargo build --release

# regenerate output/ (default command)
osunayo

# print one type's computed layout, plus the declared field sizes it was derived from
osunayo dump osu.Game.Online.Multiplayer.MultiplayerRoomUser

# score the computed layout against a reference offset dump
osunayo verify path/to/lazer.md
```

The install directory defaults to `%LOCALAPPDATA%\osulazer\current`; set `OSU_LAZER` to override
it. Building needs Rust 1.85+ (edition 2024).

## Output

One module per assembly, each rendered as JSON plus Rust, C++, C# and Zig headers.

| File | Contents |
|---|---|
| `info.json` | `game_version`, `build_number`, `source`, `timestamp`, `assemblies` |
| `<module>.json` | `{ "<module>": { classes, enums } }`; each class carries `fields`, `metadata`, `parent` |
| `<module>.rs` | `pub mod <module> { pub mod t_<Type> { pub const f_<field>: usize = 0x…; } }` |
| `<module>.hpp` | `namespace <module> { namespace t_<Type> { constexpr std::ptrdiff_t f_<field> = 0x…; } }` |
| `<module>.cs` | `public static class <module> { public static class t_<Type> { public const nint f_<field> = 0x…; } }` |
| `<module>.zig` | `pub const <module> = struct { pub const t_<Type> = struct { pub const f_<field>: usize = 0x…; } };` |

`<module>` is the assembly file name with every non-alphanumeric character replaced by `_`, so
`osu.Game.dll` becomes `osu_Game_dll`.

Fields are listed in layout order, flattened over the base chain, so any offset can be read
directly without walking `parent`.

JSON keeps the raw metadata names. The code headers cannot, so they prefix every type with `t_`
and every field with `f_` and de-duplicate collisions: .NET metadata routinely contains names that
are not identifiers in those languages (`<Host>k__BackingField`, fields literally named `type` or
`mod`, and — in `osu.Game.Auth.dll`, which ships name-obfuscated — names made of control
characters).

## What is not dumped

- **Static fields.** Their addresses only exist once the runtime resolves them; metadata alone
  cannot know them. Instance fields only.
- **Native or live addresses.** No module base, no RVA, no signature scan. Offsets are relative to
  the object header and are valid for any object of that type.
- **Value types** are not listed as classes; they appear only as field types.
- **Interfaces**, which carry no instance fields.

## Accuracy

`verify` against a 185-entry `layouts[2026.1005.0]` reference dump resolves 184 entries and
reports 108 exact matches (58.7%). The disagreements are structured, not random:

- all 184 resolved offsets fit inside the computed type size — zero misaligned, zero overlapping,
  zero beyond it;
- the ordering rule reproduces the reference exactly for
  `osu.Game.Online.Multiplayer.MultiplayerRoomUser` (8/8 of its entries, including the
  non-obvious struct-last placement) and for `osu.Framework.Timing.FramedClock.<CurrentTime>`;
- it also scores highest of the eleven candidate ordering rules implemented for comparison:
  108/184 (58.7%) overall against a 75/184 (40.8%) runner-up, and 47/50 (94.0%) over the 17 types
  whose first-field delta is already zero, against a 31/50 (62.0%) runner-up.

The residual error is confined to deep `osu.Framework` base chains and is usually a multiple of 8:
the first-field delta against the reference is −8 for types deriving directly from `Drawable` and
−16 for types deriving from `CompositeDrawable` or `Game`, so the computed base block accumulates
extra bytes somewhere in those classes. That is a struct size in my model running slightly large,
not the ordering rule; a handful of types drift further (−48, −340) or the other way (+240).
Offsets for types near `System.Object` are exact; offsets for types deep inside `osu.Framework`
should be confirmed against the running process.

## License

MIT
