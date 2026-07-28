import 'dart:convert';

import 'package:http/http.dart' as http;
import 'package:http/testing.dart';
import 'package:keryx_api/keryx_api.dart';
import 'package:test/test.dart';

void main() {
  group('SessionSummary', () {
    test('parses list projection fields', () {
      final s = SessionSummary.fromJson({
        'id': 'sess-1',
        'principal_id': 'op',
        'title': 'CI war room',
        'title_is_custom': true,
        'created_at': 1,
        'updated_at': 2,
        'last_message_preview': 'hello',
        'active_root_run': {
          'id': 'run-1',
          'goal': 'fix',
          'status': 'active',
          'origin': 'control_plane',
        },
        'pending_approval_count': 2,
      });
      expect(s.title, 'CI war room');
      expect(s.activeRootRun?.id, 'run-1');
      expect(s.pendingApprovalCount, 2);
    });
  });

  group('KeryxApiClient sessions', () {
    test('listSessions maps response', () async {
      final client = KeryxApiClient(
        config: const KeryxApiConfig(
          baseUrl: 'http://worker.test',
          operatorToken: 'tok',
        ),
        httpClient: MockClient((request) async {
          expect(request.url.path, '/v1/sessions');
          expect(request.method, 'GET');
          return http.Response(
            jsonEncode({
              'sessions': [
                {
                  'id': 's1',
                  'principal_id': 'op',
                  'title': 'New Session',
                  'title_is_custom': false,
                  'created_at': 10,
                  'updated_at': 10,
                  'last_message_preview': null,
                  'active_root_run': null,
                  'pending_approval_count': 0,
                },
              ],
            }),
            200,
          );
        }),
      );
      addTearDown(client.close);
      final list = await client.listSessions();
      expect(list.sessions.single.id, 's1');
    });

    test('createSession posts and parses', () async {
      final client = KeryxApiClient(
        config: const KeryxApiConfig(
          baseUrl: 'http://worker.test',
          operatorToken: 'tok',
        ),
        httpClient: MockClient((request) async {
          expect(request.method, 'POST');
          expect(request.url.path, '/v1/sessions');
          return http.Response(
            jsonEncode({
              'id': 'new',
              'principal_id': 'op',
              'title': 'New Session',
              'title_is_custom': false,
              'created_at': 1,
              'updated_at': 1,
              'pending_approval_count': 0,
            }),
            201,
          );
        }),
      );
      addTearDown(client.close);
      final s = await client.createSession();
      expect(s.id, 'new');
    });

    test('patchSessionTitle sends title body', () async {
      final client = KeryxApiClient(
        config: const KeryxApiConfig(
          baseUrl: 'http://worker.test',
          operatorToken: 'tok',
        ),
        httpClient: MockClient((request) async {
          expect(request.method, 'PATCH');
          expect(request.url.path, '/v1/sessions/s1');
          final body = jsonDecode(request.body) as Map;
          expect(body['title'], 'Renamed');
          return http.Response(
            jsonEncode({
              'id': 's1',
              'principal_id': 'op',
              'title': 'Renamed',
              'title_is_custom': true,
              'created_at': 1,
              'updated_at': 2,
              'pending_approval_count': 0,
            }),
            200,
          );
        }),
      );
      addTearDown(client.close);
      final s = await client.patchSessionTitle('s1', title: 'Renamed');
      expect(s.titleIsCustom, isTrue);
    });
  });
}
