import 'dart:async';
import 'dart:convert';

import 'package:http/http.dart' as http;

import 'errors.dart';
import 'models.dart';

/// Configuration for talking to one Worker control plane.
///
/// [operatorToken] may be null for unauthenticated probes only (`/health`).
/// Types allow future per-device tokens without rewriting the shell (ADR 0021).
final class KeryxApiConfig {
  const KeryxApiConfig({
    required this.baseUrl,
    this.operatorToken,
    this.timeout = const Duration(seconds: 15),
  });

  /// Worker base URL, e.g. `https://keryx.tailnet.ts.net` or `http://127.0.0.1:8787`.
  final String baseUrl;

  /// Bearer operator token. Never log or persist this outside secure storage.
  final String? operatorToken;

  final Duration timeout;

  Uri resolve(String path) {
    final root = baseUrl.endsWith('/')
        ? baseUrl.substring(0, baseUrl.length - 1)
        : baseUrl;
    final normalized = path.startsWith('/') ? path : '/$path';
    return Uri.parse('$root$normalized');
  }

  KeryxApiConfig copyWith({
    String? baseUrl,
    String? operatorToken,
    bool clearToken = false,
    Duration? timeout,
  }) {
    return KeryxApiConfig(
      baseUrl: baseUrl ?? this.baseUrl,
      operatorToken: clearToken ? null : (operatorToken ?? this.operatorToken),
      timeout: timeout ?? this.timeout,
    );
  }
}

/// Thin HTTP client for the Keryx control plane. No Flutter, no agent loop.
class KeryxApiClient {
  KeryxApiClient({
    required KeryxApiConfig config,
    http.Client? httpClient,
  })  : _config = config,
        _http = httpClient ?? http.Client(),
        _ownsClient = httpClient == null;

  KeryxApiConfig _config;
  final http.Client _http;
  final bool _ownsClient;

  KeryxApiConfig get config => _config;

  void updateConfig(KeryxApiConfig config) {
    _config = config;
  }

  void close() {
    if (_ownsClient) {
      _http.close();
    }
  }

  /// `GET /health` — unauthenticated liveness.
  Future<HealthStatus> getHealth() async {
    final response = await _send(
      method: 'GET',
      path: '/health',
      authenticated: false,
    );
    return HealthStatus.fromJson(_decodeObject(response.body));
  }

  /// `GET /v1/providers` — authenticated provider catalog (auth probe).
  Future<ProvidersResponse> listProviders() async {
    final response = await _send(
      method: 'GET',
      path: '/v1/providers',
      authenticated: true,
    );
    return ProvidersResponse.fromJson(_decodeObject(response.body));
  }

  Future<http.Response> _send({
    required String method,
    required String path,
    required bool authenticated,
    Object? body,
  }) async {
    final uri = _config.resolve(path);
    final headers = <String, String>{
      'accept': 'application/json',
    };
    if (body != null) {
      headers['content-type'] = 'application/json';
    }
    if (authenticated) {
      final token = _config.operatorToken;
      if (token == null || token.isEmpty) {
        throw const KeryxAuthException('missing operator token');
      }
      headers['authorization'] = 'Bearer $token';
    }

    try {
      final request = http.Request(method, uri)
        ..headers.addAll(headers);
      if (body != null) {
        request.body = body is String ? body : jsonEncode(body);
      }
      final streamed = await _http.send(request).timeout(_config.timeout);
      final response = await http.Response.fromStream(streamed);
      return _mapStatus(response);
    } on TimeoutException catch (e) {
      throw KeryxUnreachableException(
        'request timed out after ${_config.timeout.inSeconds}s',
        cause: e,
      );
    } on KeryxApiException {
      rethrow;
    } on http.ClientException catch (e) {
      throw KeryxUnreachableException(e.message, cause: e);
    } catch (e) {
      if (e is KeryxApiException) rethrow;
      throw KeryxUnreachableException(e.toString(), cause: e);
    }
  }

  http.Response _mapStatus(http.Response response) {
    final code = response.statusCode;
    if (code >= 200 && code < 300) {
      return response;
    }
    final errorMessage = _tryExtractError(response.body) ??
        'HTTP $code';
    if (code == 401 || code == 403) {
      throw KeryxAuthException(errorMessage, statusCode: code);
    }
    throw KeryxHttpException(
      errorMessage,
      statusCode: code,
      body: response.body,
    );
  }

  Map<String, dynamic> _decodeObject(String body) {
    try {
      final decoded = jsonDecode(body);
      if (decoded is Map<String, dynamic>) return decoded;
      if (decoded is Map) return Map<String, dynamic>.from(decoded);
      throw const FormatException('expected JSON object');
    } catch (e) {
      throw KeryxClientException('failed to decode JSON response', cause: e);
    }
  }

  String? _tryExtractError(String body) {
    try {
      final decoded = jsonDecode(body);
      if (decoded is Map && decoded['error'] is String) {
        return decoded['error'] as String;
      }
    } catch (_) {
      // ignore
    }
    return null;
  }
}
