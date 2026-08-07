import 'dart:convert';

import 'package:http/http.dart' as http;
import 'package:http/testing.dart';
import 'package:keryx_api/keryx_api.dart';
import 'package:test/test.dart';

/// Seam 4 — OpenAPI Approvals ↔ Dart client (ticket #81).
///
/// Paths and shapes match frozen docs/api/openapi.yaml + live Worker routes.
Map<String, dynamic> _approvalJson({
  String id = 'a1',
  String runId = 'r1',
  String action = 'shell_exec',
  String summary = 'rm /tmp/x',
  String status = 'pending',
  String requestedBy = 'op',
  String? decidedBy,
}) {
  return {
    'id': id,
    'run_id': runId,
    'action': action,
    'summary': summary,
    'status': status,
    'requested_by': requestedBy,
    'decided_by': decidedBy,
  };
}

void main() {
  group('ApprovalRecord', () {
    test('parses ApprovalResponse fields', () {
      final a = ApprovalRecord.fromJson(_approvalJson(decidedBy: null));
      expect(a.id, 'a1');
      expect(a.runId, 'r1');
      expect(a.action, 'shell_exec');
      expect(a.summary, 'rm /tmp/x');
      expect(a.status, 'pending');
      expect(a.requestedBy, 'op');
      expect(a.decidedBy, isNull);
      expect(a.isPending, isTrue);
      expect(a.isResolved, isFalse);
    });

    test('resolved statuses map helpers', () {
      final approved = ApprovalRecord.fromJson(
        _approvalJson(status: 'approved', decidedBy: 'op'),
      );
      expect(approved.isPending, isFalse);
      expect(approved.isResolved, isTrue);
      expect(approved.decidedBy, 'op');

      final denied = ApprovalRecord.fromJson(_approvalJson(status: 'denied'));
      expect(denied.isResolved, isTrue);
    });
  });

  group('ApprovalsListResponse', () {
    test('requires approvals array', () {
      final list = ApprovalsListResponse.fromJson({
        'approvals': [_approvalJson()],
      });
      expect(list.approvals.single.id, 'a1');
      expect(
        () => ApprovalsListResponse.fromJson({'items': []}),
        throwsA(isA<FormatException>()),
      );
    });
  });

  group('KeryxApiClient Approvals', () {
    test('listApprovals GETs pending query with bearer', () async {
      final client = KeryxApiClient(
        config: const KeryxApiConfig(
          baseUrl: 'http://worker.test',
          operatorToken: 'tok',
        ),
        httpClient: MockClient((request) async {
          expect(request.method, 'GET');
          expect(request.url.path, '/v1/approvals');
          expect(request.url.queryParameters['pending'], 'true');
          expect(request.headers['authorization'], 'Bearer tok');
          return http.Response(
            jsonEncode({
              'approvals': [_approvalJson()],
            }),
            200,
          );
        }),
      );
      addTearDown(client.close);

      final list = await client.listApprovals();
      expect(list.single.id, 'a1');
      expect(list.single.isPending, isTrue);
    });

    test('listApprovals pending=false requests all statuses', () async {
      final client = KeryxApiClient(
        config: const KeryxApiConfig(
          baseUrl: 'http://worker.test',
          operatorToken: 'tok',
        ),
        httpClient: MockClient((request) async {
          expect(request.url.queryParameters['pending'], 'false');
          return http.Response(
            jsonEncode({
              'approvals': [
                _approvalJson(status: 'approved', decidedBy: 'op'),
              ],
            }),
            200,
          );
        }),
      );
      addTearDown(client.close);

      final list = await client.listApprovals(pending: false);
      expect(list.single.status, 'approved');
    });

    test('getApproval GETs /v1/approvals/{id} with bearer', () async {
      final client = KeryxApiClient(
        config: const KeryxApiConfig(
          baseUrl: 'http://worker.test',
          operatorToken: 'tok',
        ),
        httpClient: MockClient((request) async {
          expect(request.method, 'GET');
          expect(request.url.path, '/v1/approvals/a1');
          expect(request.headers['authorization'], 'Bearer tok');
          return http.Response(jsonEncode(_approvalJson()), 200);
        }),
      );
      addTearDown(client.close);

      final a = await client.getApproval('a1');
      expect(a.id, 'a1');
      expect(a.action, 'shell_exec');
    });

    test('approveApproval POSTs with Principal bearer', () async {
      final client = KeryxApiClient(
        config: const KeryxApiConfig(
          baseUrl: 'http://worker.test',
          operatorToken: 'tok',
        ),
        httpClient: MockClient((request) async {
          expect(request.method, 'POST');
          expect(request.url.path, '/v1/approvals/a1/approve');
          expect(request.headers['authorization'], 'Bearer tok');
          expect(request.headers['content-type'], contains('application/json'));
          return http.Response(
            jsonEncode(
              _approvalJson(status: 'approved', decidedBy: 'op'),
            ),
            200,
          );
        }),
      );
      addTearDown(client.close);

      final a = await client.approveApproval('a1');
      expect(a.status, 'approved');
      expect(a.decidedBy, 'op');
    });

    test('denyApproval POSTs with Principal bearer', () async {
      final client = KeryxApiClient(
        config: const KeryxApiConfig(
          baseUrl: 'http://worker.test',
          operatorToken: 'tok',
        ),
        httpClient: MockClient((request) async {
          expect(request.method, 'POST');
          expect(request.url.path, '/v1/approvals/a1/deny');
          expect(request.headers['authorization'], 'Bearer tok');
          return http.Response(
            jsonEncode(_approvalJson(status: 'denied', decidedBy: 'op')),
            200,
          );
        }),
      );
      addTearDown(client.close);

      final a = await client.denyApproval('a1');
      expect(a.status, 'denied');
    });

    test('approve without token fails closed before HTTP', () async {
      var calls = 0;
      final client = KeryxApiClient(
        config: const KeryxApiConfig(baseUrl: 'http://worker.test'),
        httpClient: MockClient((request) async {
          calls++;
          return http.Response('{}', 200);
        }),
      );
      addTearDown(client.close);

      expect(
        () => client.approveApproval('a1'),
        throwsA(isA<KeryxAuthException>()),
      );
      expect(calls, 0);
    });

    test('409 not pending becomes http error', () async {
      final client = KeryxApiClient(
        config: const KeryxApiConfig(
          baseUrl: 'http://worker.test',
          operatorToken: 'tok',
        ),
        httpClient: MockClient((request) async {
          return http.Response(
            jsonEncode({'error': 'approval is not pending'}),
            409,
          );
        }),
      );
      addTearDown(client.close);

      expect(
        () => client.approveApproval('a1'),
        throwsA(
          isA<KeryxHttpException>().having((e) => e.statusCode, 'code', 409),
        ),
      );
    });
  });

  group('InboxItem approval_id linkage', () {
    test('parses approval_pending for Needs you dual surface', () {
      final item = InboxItem.fromJson({
        'id': 'approval:a1',
        'kind': 'approval_pending',
        'session_id': 's1',
        'run_id': 'r1',
        'approval_id': 'a1',
        'title': 'Allow exec',
        'summary': 'shell_exec',
        'created_at': 42,
      });
      expect(item.kind, 'approval_pending');
      expect(item.approvalId, 'a1');
      expect(item.sessionId, 's1');
    });

    test('listInbox maps items with approval_id', () async {
      final client = KeryxApiClient(
        config: const KeryxApiConfig(
          baseUrl: 'http://worker.test',
          operatorToken: 'tok',
        ),
        httpClient: MockClient((request) async {
          expect(request.url.path, '/v1/inbox');
          expect(request.headers['authorization'], 'Bearer tok');
          return http.Response(
            jsonEncode({
              'items': [
                {
                  'id': 'approval:a1',
                  'kind': 'approval_pending',
                  'session_id': 's1',
                  'run_id': 'r1',
                  'approval_id': 'a1',
                  'title': 'Allow exec',
                  'summary': 'high blast',
                  'created_at': 1,
                },
              ],
            }),
            200,
          );
        }),
      );
      addTearDown(client.close);

      final items = await client.listInbox();
      expect(items.single.approvalId, 'a1');
      expect(items.single.kind, 'approval_pending');
    });
  });
}
