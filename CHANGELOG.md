# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project does not currently adhere to a particular versioning scheme.

## [Unreleased]

### Changed

- Improve error messages for redefinition of types and objects. ([#485][gh-485])
- Don't allow tabs to be used for indentation or spacing. ([#463][gh-463])
- Rename builtin function `sleep` to `IO/nanosleep`. ([#581][gh-581])
- Equational number pattern compilation to use the predecessor variable when possible. ([#470][gh-470])

### Fixed

- Fix readback of numeric operations. ([#467][gh-467])
- Propagate the "builtin" attribute of definitions when extracting functions from `bend` and `fold` syntax.
- Panic while using unscoped variables on definition patterns. ([#468][gh-468])

### Added

- Create new type of top-level definition for writing native HVM definitions. ([#586][gh-586])
- Add `log` and `atan2` builtin functions. ([#583][gh-583])
- Add `to_f24`, `to_u24` and `to_i24` number casting builtin functions. ([#582][gh-582])
- Add `IO/sleep` builtin function to sleep for a given amount of seconds as a float. ([#581][gh-581])
- Add primitive file IO functions `IO/FS/{read, write, seek, open, close}`. ([#573][gh-573])
- Add encoding/decoding builtin functions `Bytes/{decode_utf8, decode_ascii} String/{encode_ascii, decode_ascii} Utf8/{decode_character, REPLACEMENT_CHARACTER}`. ([#580][gh-580])
- Add `IO/print` function to print to stdout. ([#579][gh-579])
- Add `IO/input` function to read a line of input from stdin. ([#579][gh-579])
- Add file IO utilities `IO/FS/{read_file, write_file, read_line, read_to_end}`. ([#578][gh-578])
- Add list utilities `List/{length, reverse, flatten, concat}`.
- Add `elif` chains to functional syntax. ([#596][gh-596])
- Add local definitions to imperative syntax. ([#562][gh-562])
- Add local definitions to functional syntax.

## [0.2.35] - 2024-06-06

### Changed

- Make lambda `Term` with bind patterns display as `let` terms. ([#466][gh-466])

## [0.2.34] - 2024-06-05

### Added

- Add syntax for "less than or equal" `<=` and "greater than or equal" `>=` numeric operators. ([#451][gh-451])

## [0.2.33] - 2024-06-05

### Added

- Implement `expand_main`, a compilation pass that expands references in the entry point function. ([#424][gh-424])

### Changed

- Make the `float_combinators` pass no longer extract in the entry point function. ([#424][gh-424])

## [0.2.32] - 2024-06-05

### Added

- Implement the built-in `Tree` datatype. ([#528][gh-528])
- Implement `![]` and `!` syntax for `Tree` literals. ([#528][gh-528])
- Create a changelog.

### Changed

- Move the builtins documentation to `/docs`.

## [0.2.30] - 2024-06-04

### Changed

- No longer expand generated recursive definitions. ([#502][gh-502])

## [0.2.29] - 2024-06-04

### Added

- Support custom HVM binaries. ([#479][gh-479])

### Changed

- Make monadic blocks lazy by deferring execution of continuations with free vars. ([#526][gh-526])

## [0.2.28] - 2024-05-30

### Added

- Support mapper statements. ([#465][gh-465])

## [0.2.27] - 2024-05-29

### Changed

- Make `with` clauses take a bind and an argument. ([#516][gh-516])

## [0.2.26] - 2024-05-28

### Changed

- `do` keyword to `with`. ([#494][gh-494])

### Added

- `wrap` alias inside `with` blocks. ([#494][gh-494])

## [0.2.25] - 2024-05-28

### Added

- Generated constructor tags. ([#512][gh-512])

## [0.2.24] - 2024-05-27

### Added

- `elif` chains. ([#427][gh-427])

## [0.2.23] - 2024-05-27

### Fixed

- `gen-cu` and `gen-c` commands after move to HVM syntax tree.

## [0.2.22] - 2024-05-26

### Changed

- Rust channel from `nightly` to `stable`. ([#486][gh-486])

## [0.2.21] - 2024-05-25

### Changed

- HVM syntax tree for representing inets inside the compiler. ([#475][gh-475])

## [0.2.20] - 2024-05-24

### Fixed

- Map getters generation inside map setters. ([#489][gh-489])

## [0.2.19] - 2024-05-24

### Changed

- Variable names to not allow `__`. ([#478][gh-478])

## [0.2.18] - 2024-05-24

### Fixed

- Nested map getters generation. ([#483][gh-483])

## [0.2.17] - 2024-05-23

### Changed

- Top-level names to not start with `//`. ([#443][gh-443])

## [0.2.16] - 2024-05-23

### Added

- New `IO` builtins.

### Fixed

- Definition pruning transformation.

## [0.2.15] - 2024-05-22

### Fixed

- Exponentiation miscompilation. ([#444][gh-444])

## [0.2.14] - 2024-05-22

### Changed

- Inet level eta-reduction pass to not reduce number nodes.

## [0.2.13] - 2024-05-22

### Fixed

- Scope of `fork`.

## [0.2.12] - 2024-05-22

### Changed

- Functional syntax `data` keyword to `type`.

## [0.2.11] - 2024-05-22

### Added

- List comprehension.
- Bit shift left and Bit shift right.

## [0.2.10] - 2024-05-21

### Changed

- Numbers to new HVM number operation format.
- Rules definition to be in a single block.
- Disabled `net-size` check by default.

## [0.2.9] - 2024-05-19

### Changed

- Readback error messages.

## [0.2.8] - 2024-05-19

### Changed

- Increase max net size.
- `check-net-size` to be optional.

## [0.2.7] - 2024-05-17

### Added

- Apache-2.0 License.

## [0.2.6] - 2024-05-17

### Added

- Simple readback of tuples.

### Changed

- Imperative syntax to require `,` in list-like builtins.

### Fixed

- Empty map parsing.

## [0.2.5] - 2024-05-16

### Added

- Exponentiation `**` operator.

### Changed

- `go` to `fork` inside `bend` statement.

## [0.2.4] - 2024-05-15

### Changed

- New version for hvm-core compatibility.

## [0.2.3] - 2024-05-15

### Changed

- Rename to `bend-lang`.
- Use crates.io HVM.

### Added

- List readback.

## [0.2.2] - 2024-05-15

### Changed

- Comments from `//` to `#`.
- Lambda syntax and built-in constructor names.
- Fold to require explicit state passing.

### Added

- Record types and destructuring.
- String readback.

## [0.2.1] - 2024-05-15

### Fixed

- Number parsing in imperative syntax.

### Changed

- Require `\n` after return.

## [0.2.0] - 2024-05-14

- Initial public release of Bend.

[0.2.0]: https://github.com/HigherOrderCO/Bend/releases/tag/0.2.0
[0.2.1]: https://github.com/HigherOrderCO/Bend/releases/tag/0.2.1
[0.2.2]: https://github.com/HigherOrderCO/Bend/releases/tag/0.2.2
[0.2.3]: https://github.com/HigherOrderCO/Bend/releases/tag/0.2.3
[0.2.4]: https://github.com/HigherOrderCO/Bend/releases/tag/0.2.4
[0.2.5]: https://github.com/HigherOrderCO/Bend/releases/tag/0.2.5
[0.2.6]: https://github.com/HigherOrderCO/Bend/releases/tag/0.2.6
[0.2.7]: https://github.com/HigherOrderCO/Bend/releases/tag/0.2.7
[0.2.8]: https://github.com/HigherOrderCO/Bend/releases/tag/0.2.8
[0.2.9]: https://github.com/HigherOrderCO/Bend/releases/tag/0.2.9
[0.2.10]: https://github.com/HigherOrderCO/Bend/releases/tag/0.2.10
[0.2.11]: https://github.com/HigherOrderCO/Bend/releases/tag/0.2.11
[0.2.12]: https://github.com/HigherOrderCO/Bend/releases/tag/0.2.12
[0.2.13]: https://github.com/HigherOrderCO/Bend/releases/tag/0.2.13
[0.2.14]: https://github.com/HigherOrderCO/Bend/releases/tag/0.2.14
[0.2.15]: https://github.com/HigherOrderCO/Bend/releases/tag/0.2.15
[0.2.16]: https://github.com/HigherOrderCO/Bend/releases/tag/0.2.16
[0.2.17]: https://github.com/HigherOrderCO/Bend/releases/tag/0.2.17
[0.2.18]: https://github.com/HigherOrderCO/Bend/releases/tag/0.2.18
[0.2.19]: https://github.com/HigherOrderCO/Bend/releases/tag/0.2.19
[0.2.20]: https://github.com/HigherOrderCO/Bend/releases/tag/0.2.20
[0.2.21]: https://github.com/HigherOrderCO/Bend/releases/tag/0.2.21
[0.2.22]: https://github.com/HigherOrderCO/Bend/releases/tag/0.2.22
[0.2.23]: https://github.com/HigherOrderCO/Bend/releases/tag/0.2.23
[0.2.24]: https://github.com/HigherOrderCO/Bend/releases/tag/0.2.24
[0.2.25]: https://github.com/HigherOrderCO/Bend/releases/tag/0.2.25
[0.2.26]: https://github.com/HigherOrderCO/Bend/releases/tag/0.2.26
[0.2.27]: https://github.com/HigherOrderCO/Bend/releases/tag/0.2.27
[0.2.28]: https://github.com/HigherOrderCO/Bend/releases/tag/0.2.28
[0.2.29]: https://github.com/HigherOrderCO/Bend/releases/tag/0.2.29
[0.2.30]: https://github.com/HigherOrderCO/Bend/releases/tag/0.2.30
[0.2.32]: https://github.com/HigherOrderCO/Bend/releases/tag/0.2.32
[0.2.33]: https://github.com/HigherOrderCO/Bend/releases/tag/0.2.33
[0.2.34]: https://github.com/HigherOrderCO/Bend/releases/tag/0.2.34
[0.2.35]: https://github.com/HigherOrderCO/Bend/releases/tag/0.2.35
[gh-424]: https://github.com/HigherOrderCO/Bend/issues/424
[gh-427]: https://github.com/HigherOrderCO/Bend/issues/427
[gh-443]: https://github.com/HigherOrderCO/Bend/issues/443
[gh-444]: https://github.com/HigherOrderCO/Bend/issues/444
[gh-451]: https://github.com/HigherOrderCO/Bend/issues/451
[gh-463]: https://github.com/HigherOrderCO/Bend/issues/463
[gh-465]: https://github.com/HigherOrderCO/Bend/issues/465
[gh-466]: https://github.com/HigherOrderCO/Bend/issues/466
[gh-467]: https://github.com/HigherOrderCO/Bend/issues/467
[gh-468]: https://github.com/HigherOrderCO/Bend/issues/468
[gh-470]: https://github.com/HigherOrderCO/Bend/issues/470
[gh-475]: https://github.com/HigherOrderCO/Bend/issues/475
[gh-478]: https://github.com/HigherOrderCO/Bend/issues/478
[gh-479]: https://github.com/HigherOrderCO/Bend/issues/479
[gh-483]: https://github.com/HigherOrderCO/Bend/issues/483
[gh-485]: https://github.com/HigherOrderCO/Bend/issues/485
[gh-486]: https://github.com/HigherOrderCO/Bend/issues/486
[gh-489]: https://github.com/HigherOrderCO/Bend/issues/489
[gh-494]: https://github.com/HigherOrderCO/Bend/issues/494
[gh-502]: https://github.com/HigherOrderCO/Bend/issues/502
[gh-512]: https://github.com/HigherOrderCO/Bend/issues/512
[gh-516]: https://github.com/HigherOrderCO/Bend/issues/516
[gh-526]: https://github.com/HigherOrderCO/Bend/issues/526
[gh-528]: https://github.com/HigherOrderCO/Bend/issues/528
[gh-562]: https://github.com/HigherOrderCO/Bend/issues/562
[gh-581]: https://github.com/HigherOrderCO/Bend/issues/581
[gh-573]: https://github.com/HigherOrderCO/Bend/issues/573
[gh-578]: https://github.com/HigherOrderCO/Bend/issues/578
[gh-579]: https://github.com/HigherOrderCO/Bend/issues/579
[gh-580]: https://github.com/HigherOrderCO/Bend/issues/580
[gh-582]: https://github.com/HigherOrderCO/Bend/issues/582
[gh-583]: https://github.com/HigherOrderCO/Bend/issues/583
[gh-586]: https://github.com/HigherOrderCO/Bend/issues/586
[gh-596]: https://github.com/HigherOrderCO/Bend/issues/596
[Unreleased]: https://github.com/HigherOrderCO/Bend/compare/0.2.35...HEAD
