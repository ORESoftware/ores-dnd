/// `ores_dnd` — the pure-Dart implementation of the `ores.dnd/v1`
/// cross-runtime drag-and-drop protocol: codec, drop policy, session state
/// machine, integration ports and the declaration decoder used for tjsv
/// runtime evidence. No Flutter dependency; `ores_dnd_flutter` adds widgets.
///
/// See docs/DESIGN.md in the repository for the state machine and policy order
/// every runtime (Rust, TypeScript, Dart) implements identically.
library;

import 'dart:convert';

part 'src/wire.dart';
part 'src/codec.dart';
part 'src/policy.dart';
part 'src/session.dart';
part 'src/ports.dart';
part 'src/corpus.dart';
part 'src/fuzz.dart';
