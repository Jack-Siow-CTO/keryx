import 'dart:async';
import 'dart:convert';

import 'package:http/http.dart' as http;
import 'package:http/testing.dart';
import 'package:keryx_api/keryx_api.dart';
import 'package:test/test.dart';

void main() {
  group('HealthStatus', () {
    test('parses ok body', () {
      final h = HealthStatus.fromJson({'status': 'ok'});
      expect(h.isOk, isTrue);
    });
  });

  group('KeryxApiClient.getHealth', () {
    test('maps 200 to HealthStatus', () async {
      final client = KeryxApiClient(
        config: const KeryxApiConfig(baseUrl: 'http://worker.test'),
        httpClient: MockClient((request) async {
          expect(request.url.path, '/health');
          expect(request.headers.containsKey('authorization'), isFalse);
          return http.Response(jsonEncode({'status': 'ok'}), 200);
        }),
      );
      addTearDown(client.close);

      final health = await client.getHealth();
      expect(health.status, 'ok');
    });

    test('transport failure becomes unreachable', () async {
      final client = KeryxApiClient(
        config: const KeryxApiConfig(baseUrl: 'http://worker.test'),
        httpClient: MockClient((request) async {
          throw http.ClientException('connection refused');
        }),
      );
      addTearDown(client.close);

      expect(
        () => client.getHealth(),
        throwsA(isA<KeryxUnreachableException>()),
      );
    });
  });

  group('KeryxApiClient.listProviders', () {
    test('sends bearer token and parses providers', () async {
      final client = KeryxApiClient(
        config: const KeryxApiConfig(
          baseUrl: 'http://worker.test',
          operatorToken: 'secret-token',
        ),
        httpClient: MockClient((request) async {
          expect(request.url.path, '/v1/providers');
          expect(request.headers['authorization'], 'Bearer secret-token');
          return http.Response(
            jsonEncode({
              'default': 'openai',
              'providers': [
                {
                  'name': 'openai',
                  'auth_kind': 'api_key',
                  'display_name': 'OpenAI',
                  'default_model': 'gpt-5',
                  'models': ['gpt-5'],
                  'registered': true,
                  'supports_model_override': true,
                },
              ],
            }),
            200,
          );
        }),
      );
      addTearDown(client.close);

      final res = await client.listProviders();
      expect(res.defaultProvider, 'openai');
      expect(res.providers.single.name, 'openai');
    });

    test('401 is auth failure (fail closed)', () async {
      final client = KeryxApiClient(
        config: const KeryxApiConfig(
          baseUrl: 'http://worker.test',
          operatorToken: 'bad',
        ),
        httpClient: MockClient((request) async {
          return http.Response(
            jsonEncode({'error': 'invalid operator token'}),
            401,
          );
        }),
      );
      addTearDown(client.close);

      expect(
        () => client.listProviders(),
        throwsA(
          isA<KeryxAuthException>().having(
            (e) => e.message,
            'message',
            contains('invalid operator token'),
          ),
        ),
      );
    });

    test('missing token fails closed without HTTP call side effects', () async {
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
        () => client.listProviders(),
        throwsA(isA<KeryxAuthException>()),
      );
      expect(calls, 0);
    });
  });

  group('checkConnectivity', () {
    test('ok when health and providers succeed', () async {
      final client = KeryxApiClient(
        config: const KeryxApiConfig(
          baseUrl: 'http://worker.test',
          operatorToken: 'tok',
        ),
        httpClient: MockClient((request) async {
          if (request.url.path == '/health') {
            return http.Response(jsonEncode({'status': 'ok'}), 200);
          }
          if (request.url.path == '/v1/providers') {
            return http.Response(
              jsonEncode({'default': null, 'providers': []}),
              200,
            );
          }
          return http.Response('not found', 404);
        }),
      );
      addTearDown(client.close);

      final result = await checkConnectivity(client);
      expect(result.kind, ConnectivityKind.ok);
      expect(result.healthOk, isTrue);
    });

    test('unreachable when health cannot connect', () async {
      final client = KeryxApiClient(
        config: const KeryxApiConfig(
          baseUrl: 'http://worker.test',
          operatorToken: 'tok',
        ),
        httpClient: MockClient((request) async {
          throw http.ClientException('network down');
        }),
      );
      addTearDown(client.close);

      final result = await checkConnectivity(client);
      expect(result.kind, ConnectivityKind.unreachable);
      expect(result.healthOk, isFalse);
    });

    test('authFailure when health ok but token invalid', () async {
      final client = KeryxApiClient(
        config: const KeryxApiConfig(
          baseUrl: 'http://worker.test',
          operatorToken: 'bad',
        ),
        httpClient: MockClient((request) async {
          if (request.url.path == '/health') {
            return http.Response(jsonEncode({'status': 'ok'}), 200);
          }
          return http.Response(
            jsonEncode({'error': 'invalid operator token'}),
            401,
          );
        }),
      );
      addTearDown(client.close);

      final result = await checkConnectivity(client);
      expect(result.kind, ConnectivityKind.authFailure);
      expect(result.healthOk, isTrue);
    });
  });
}
