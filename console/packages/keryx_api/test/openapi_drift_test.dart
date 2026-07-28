import 'dart:io';

import 'package:test/test.dart';
import 'package:yaml/yaml.dart';

/// Seam 4 — OpenAPI ↔ client surface drift guard (ticket #38 skeleton).
///
/// Resolves the monorepo OpenAPI document and asserts Must paths for this
/// ticket remain declared. Expand as later tickets add resources.
void main() {
  late YamlMap openapi;

  setUpAll(() {
    final candidates = [
      // package cwd: console/packages/keryx_api
      Directory.current.uri.resolve('../../../docs/api/openapi.yaml'),
      // monorepo root cwd
      Directory.current.uri.resolve('docs/api/openapi.yaml'),
    ];
    File? file;
    for (final uri in candidates) {
      final f = File.fromUri(uri);
      if (f.existsSync()) {
        file = f;
        break;
      }
    }
    expect(file, isNotNull, reason: 'docs/api/openapi.yaml not found');
    final doc = loadYaml(file!.readAsStringSync());
    expect(doc, isA<YamlMap>());
    openapi = doc as YamlMap;
  });

  test('documents /health, /v1/providers, and Session surfaces', () {
    final paths = openapi['paths'] as YamlMap;
    expect(paths.containsKey('/health'), isTrue);
    expect(paths.containsKey('/v1/providers'), isTrue);
    expect(paths.containsKey('/v1/sessions'), isTrue);
    expect(paths.containsKey('/v1/sessions/{session_id}'), isTrue);
  });

  test('health is unauthenticated getHealth', () {
    final health = (openapi['paths'] as YamlMap)['/health'] as YamlMap;
    final get = health['get'] as YamlMap;
    expect(get['operationId'], 'getHealth');
  });

  test('providers requires bearerAuth', () {
    final providers =
        (openapi['paths'] as YamlMap)['/v1/providers'] as YamlMap;
    final get = providers['get'] as YamlMap;
    expect(get['operationId'], 'listProviders');
    final security = get['security'] as YamlList;
    expect(security, isNotEmpty);
  });

  test('HealthResponse schema requires status', () {
    final schemas =
        (openapi['components'] as YamlMap)['schemas'] as YamlMap;
    final health = schemas['HealthResponse'] as YamlMap;
    final required = (health['required'] as YamlList).map((e) => '$e').toList();
    expect(required, contains('status'));
  });
}
