import 'dart:convert';
import 'dart:io';

const contracts = '../../contracts';

typedef CorpusCase = ({String declaration, String expectation, String file, String json});

List<CorpusCase> corpus() {
  final out = <CorpusCase>[];
  final root = Directory('$contracts/instances');
  final decls = root.listSync().whereType<Directory>().toList()..sort((a, b) => a.path.compareTo(b.path));
  for (final decl in decls) {
    final name = decl.uri.pathSegments.where((s) => s.isNotEmpty).last;
    for (final (lane, expectation) in const [('valid', 'accepted'), ('invalid', 'rejected')]) {
      final dir = Directory('${decl.path}/$lane');
      if (!dir.existsSync()) continue;
      final files = dir.listSync().whereType<File>().where((f) => f.path.endsWith('.json')).toList()..sort((a, b) => a.path.compareTo(b.path));
      for (final file in files) {
        out.add((declaration: name, expectation: expectation, file: file.uri.pathSegments.last, json: file.readAsStringSync()));
      }
    }
  }
  return out;
}

Map<String, Object?> readJson(String rel) => (jsonDecode(File('$contracts/$rel').readAsStringSync()) as Map).cast<String, Object?>();
