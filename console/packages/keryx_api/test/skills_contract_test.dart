import 'dart:convert';

import 'package:http/http.dart' as http;
import 'package:http/testing.dart';
import 'package:keryx_api/keryx_api.dart';
import 'package:test/test.dart';

/// Seam 4 — Skills list/get after learning-loop write (ticket #82).
///
/// Paths and shapes match GET /v1/skills and GET /v1/skills/{name}
/// (Worker control plane + OpenAPI). Covers the Console half of
/// draft → Approve → list → load without a package CMS.

Map<String, dynamic> _skillSummaryJson({String name = 'daily-note'}) => {
      'name': name,
    };

Map<String, dynamic> _skillDetailJson({
  String name = 'daily-note',
  String content = '# daily-note\n\nCapture a short daily note.\n',
}) =>
    {
      'name': name,
      'content': content,
    };

void main() {
  group('SkillSummary / SkillDetail', () {
    test('parses list item name', () {
      final s = SkillSummary.fromJson(_skillSummaryJson());
      expect(s.name, 'daily-note');
    });

    test('parses package content for read-only view', () {
      final d = SkillDetail.fromJson(_skillDetailJson());
      expect(d.name, 'daily-note');
      expect(d.content, contains('Capture a short daily note'));
    });
  });

  group('KeryxApiClient Skills', () {
    test('listSkills GETs /v1/skills with bearer and maps packages', () async {
      final client = KeryxApiClient(
        config: const KeryxApiConfig(
          baseUrl: 'http://worker.test',
          operatorToken: 'tok',
        ),
        httpClient: MockClient((request) async {
          expect(request.method, 'GET');
          expect(request.url.path, '/v1/skills');
          expect(request.headers['authorization'], 'Bearer tok');
          return http.Response(
            jsonEncode({
              'skills': [
                _skillSummaryJson(name: 'daily-note'),
                _skillSummaryJson(name: 'triage'),
              ],
            }),
            200,
          );
        }),
      );
      addTearDown(client.close);

      final list = await client.listSkills();
      expect(list, hasLength(2));
      expect(list.map((s) => s.name).toList(), ['daily-note', 'triage']);
    });

    test('listSkills empty root is empty list (pre-Approve)', () async {
      final client = KeryxApiClient(
        config: const KeryxApiConfig(
          baseUrl: 'http://worker.test',
          operatorToken: 'tok',
        ),
        httpClient: MockClient((request) async {
          return http.Response(jsonEncode({'skills': <Object>[]}), 200);
        }),
      );
      addTearDown(client.close);

      final list = await client.listSkills();
      expect(list, isEmpty);
    });

    test('getSkill GETs /v1/skills/{name} after package write', () async {
      final client = KeryxApiClient(
        config: const KeryxApiConfig(
          baseUrl: 'http://worker.test',
          operatorToken: 'tok',
        ),
        httpClient: MockClient((request) async {
          expect(request.method, 'GET');
          expect(request.url.path, '/v1/skills/daily-note');
          expect(request.headers['authorization'], 'Bearer tok');
          return http.Response(
            jsonEncode(_skillDetailJson()),
            200,
          );
        }),
      );
      addTearDown(client.close);

      final detail = await client.getSkill('daily-note');
      expect(detail.name, 'daily-note');
      expect(detail.content, startsWith('# daily-note'));
    });

    test('list then get is the post-Approve list/load verification path',
        () async {
      var step = 0;
      final client = KeryxApiClient(
        config: const KeryxApiConfig(
          baseUrl: 'http://worker.test',
          operatorToken: 'tok',
        ),
        httpClient: MockClient((request) async {
          step++;
          if (step == 1) {
            expect(request.url.path, '/v1/skills');
            return http.Response(
              jsonEncode({
                'skills': [_skillSummaryJson()],
              }),
              200,
            );
          }
          expect(request.url.path, '/v1/skills/daily-note');
          return http.Response(jsonEncode(_skillDetailJson()), 200);
        }),
      );
      addTearDown(client.close);

      final list = await client.listSkills();
      expect(list.single.name, 'daily-note');
      final detail = await client.getSkill(list.single.name);
      expect(detail.content, contains('daily note'));
    });
  });
}
