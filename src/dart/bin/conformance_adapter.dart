// tjsv runtime-evidence adapter:
//   dart run bin/conformance_adapter.dart <cases.json> <out.json>
// Reads the trusted case list produced by scripts/conformance/corpus.mjs and
// writes this runtime's adapter block (verdicts only — never expectations).
import 'dart:convert';
import 'dart:io';

import 'package:ores_dnd/ores_dnd.dart';

Future<void> main(List<String> args) async {
  if (args.length != 2) {
    stderr.writeln('usage: conformance_adapter.dart <cases.json> <out.json>');
    exit(64);
  }
  final casesFile = File(args[0]);
  final root = casesFile.parent.path;
  final cases =
      (jsonDecode(await casesFile.readAsString())
              as Map<String, Object?>)['cases']
          as List;
  final results = <Map<String, Object?>>[];
  for (final raw in cases) {
    final c = (raw as Map).cast<String, Object?>();
    final path = c['path'] as String;
    final file = File(path.startsWith('/') ? path : '$root/$path');
    final json = await file.readAsString();
    var verdict = 'accepted';
    try {
      decodeDeclaration(c['declaration'] as String, json);
    } on FormatException {
      verdict = 'rejected';
    }
    results.add({
      'caseId': c['id'],
      'declaration': c['declaration'],
      'verdict': verdict,
    });
  }
  final adapter = {
    'id': 'dart-ores-dnd',
    'language': 'dart',
    'runtime': 'dart@${Platform.version.split(' ').first}',
    'validator': 'ores_dnd@0.1.0',
    'toolchain': 'dart@${Platform.version.split(' ').first}',
    'status': 'passed',
    'results': results,
  };
  await File(
    args[1],
  ).writeAsString('${const JsonEncoder.withIndent('  ').convert(adapter)}\n');
  stderr.writeln('dart adapter: ${results.length} cases -> ${args[1]}');
}
