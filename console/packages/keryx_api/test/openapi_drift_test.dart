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
    expect(paths.containsKey('/v1/sessions/{session_id}/transcript'), isTrue);
    expect(paths.containsKey('/v1/inbox'), isTrue);
    expect(paths.containsKey('/v1/memory'), isTrue);
    expect(paths.containsKey('/v1/skills'), isTrue);
    expect(paths.containsKey('/v1/skills/{name}'), isTrue);
    expect(paths.containsKey('/v1/artifacts/{artifact_id}'), isTrue);
    expect(paths.containsKey('/v1/sessions/{session_id}/runs'), isTrue);
    expect(paths.containsKey('/v1/runs/{run_id}/events'), isTrue);
  });

  test('documents Approvals list, get, approve, deny (live Worker paths)', () {
    final paths = openapi['paths'] as YamlMap;
    expect(paths.containsKey('/v1/approvals'), isTrue);
    expect(paths.containsKey('/v1/approvals/{approval_id}'), isTrue);
    expect(paths.containsKey('/v1/approvals/{approval_id}/approve'), isTrue);
    expect(paths.containsKey('/v1/approvals/{approval_id}/deny'), isTrue);

    final list = paths['/v1/approvals'] as YamlMap;
    final listGet = list['get'] as YamlMap;
    expect(listGet['operationId'], 'listApprovals');
    final security = listGet['security'] as YamlList;
    expect(security, isNotEmpty);

    final params = listGet['parameters'] as YamlList;
    final pending = params.cast<YamlMap>().firstWhere(
      (p) => p['name'] == 'pending',
      orElse: () => throw TestFailure('listApprovals missing pending query'),
    );
    expect(pending['in'], 'query');
    final pendingSchema = pending['schema'] as YamlMap;
    expect(pendingSchema['type'], 'boolean');
    expect(pendingSchema['default'], isTrue);

    final getOne = (paths['/v1/approvals/{approval_id}'] as YamlMap)['get']
        as YamlMap;
    expect(getOne['operationId'], 'getApproval');

    final approve =
        (paths['/v1/approvals/{approval_id}/approve'] as YamlMap)['post']
            as YamlMap;
    expect(approve['operationId'], 'approveApproval');

    final deny =
        (paths['/v1/approvals/{approval_id}/deny'] as YamlMap)['post']
            as YamlMap;
    expect(deny['operationId'], 'denyApproval');
  });

  test('ApprovalResponse schema matches live ApprovalResponse fields', () {
    final schemas =
        (openapi['components'] as YamlMap)['schemas'] as YamlMap;
    expect(schemas.containsKey('ApprovalResponse'), isTrue);
    expect(schemas.containsKey('ApprovalsListResponse'), isTrue);

    final approval = schemas['ApprovalResponse'] as YamlMap;
    final required =
        (approval['required'] as YamlList).map((e) => '$e').toList();
    for (final field in [
      'id',
      'run_id',
      'action',
      'summary',
      'status',
      'requested_by',
    ]) {
      expect(required, contains(field), reason: 'required: $field');
    }
    final props = approval['properties'] as YamlMap;
    for (final field in [
      'id',
      'run_id',
      'action',
      'summary',
      'status',
      'requested_by',
      'decided_by',
    ]) {
      expect(props.containsKey(field), isTrue, reason: 'property: $field');
    }
    final status = props['status'] as YamlMap;
    final statusEnum = (status['enum'] as YamlList).map((e) => '$e').toList();
    expect(statusEnum, containsAll(['pending', 'approved', 'denied']));

    final list = schemas['ApprovalsListResponse'] as YamlMap;
    final listRequired =
        (list['required'] as YamlList).map((e) => '$e').toList();
    expect(listRequired, contains('approvals'));
    final listProps = list['properties'] as YamlMap;
    final approvals = listProps['approvals'] as YamlMap;
    expect(approvals['type'], 'array');
    final items = approvals['items'] as YamlMap;
    expect(items['\$ref'], '#/components/schemas/ApprovalResponse');
  });

  test('InboxItem schema links pending Approvals for Needs you', () {
    final schemas =
        (openapi['components'] as YamlMap)['schemas'] as YamlMap;
    expect(schemas.containsKey('InboxItem'), isTrue);
    expect(schemas.containsKey('InboxResponse'), isTrue);

    final item = schemas['InboxItem'] as YamlMap;
    final required = (item['required'] as YamlList).map((e) => '$e').toList();
    for (final field in ['id', 'kind', 'title', 'summary', 'created_at']) {
      expect(required, contains(field), reason: 'required: $field');
    }
    final props = item['properties'] as YamlMap;
    expect(props.containsKey('approval_id'), isTrue);
    expect(props.containsKey('session_id'), isTrue);
    expect(props.containsKey('run_id'), isTrue);
    final kind = props['kind'] as YamlMap;
    final kindEnum = (kind['enum'] as YamlList).map((e) => '$e').toList();
    expect(kindEnum, contains('approval_pending'));

    final inbox = (openapi['paths'] as YamlMap)['/v1/inbox'] as YamlMap;
    final get = inbox['get'] as YamlMap;
    final responses = get['responses'] as YamlMap;
    final ok = responses['200'] as YamlMap;
    final content = ok['content'] as YamlMap;
    final json = content['application/json'] as YamlMap;
    final schema = json['schema'] as YamlMap;
    expect(schema['\$ref'], '#/components/schemas/InboxResponse');
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
