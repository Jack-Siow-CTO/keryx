import 'dart:async';
import 'dart:convert';

import 'package:http/http.dart' as http;

import 'errors.dart';
import 'models.dart';
import 'sse.dart';

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

  /// `GET /v1/sessions` — operator Session list projection.
  Future<SessionListResponse> listSessions() async {
    final response = await _send(
      method: 'GET',
      path: '/v1/sessions',
      authenticated: true,
    );
    return SessionListResponse.fromJson(_decodeObject(response.body));
  }

  /// `POST /v1/sessions` — create Session.
  Future<SessionSummary> createSession() async {
    final response = await _send(
      method: 'POST',
      path: '/v1/sessions',
      authenticated: true,
      body: <String, dynamic>{},
    );
    return SessionSummary.fromJson(_decodeObject(response.body));
  }

  /// `GET /v1/sessions/{id}`.
  Future<SessionSummary> getSession(String sessionId) async {
    final response = await _send(
      method: 'GET',
      path: '/v1/sessions/$sessionId',
      authenticated: true,
    );
    return SessionSummary.fromJson(_decodeObject(response.body));
  }

  /// `PATCH /v1/sessions/{id}` — rename title (durable on Worker).
  Future<SessionSummary> patchSessionTitle(
    String sessionId, {
    required String title,
  }) async {
    final response = await _send(
      method: 'PATCH',
      path: '/v1/sessions/$sessionId',
      authenticated: true,
      body: {'title': title},
    );
    return SessionSummary.fromJson(_decodeObject(response.body));
  }

  /// `GET /v1/sessions/{id}/transcript` — reverse-chronological page.
  Future<TranscriptPage> getTranscript(
    String sessionId, {
    int limit = 50,
    String? before,
  }) async {
    final qp = <String, String>{'limit': '$limit'};
    if (before != null) qp['before'] = before;
    final path = Uri(
      path: '/v1/sessions/$sessionId/transcript',
      queryParameters: qp,
    ).toString();
    final response = await _send(
      method: 'GET',
      path: path,
      authenticated: true,
    );
    return TranscriptPage.fromJson(_decodeObject(response.body));
  }


  /// `POST /v1/sessions/{id}/runs` — start root Run (control_plane origin).
  Future<RunRecord> startRun(
    String sessionId, {
    required String goal,
    String? provider,
    String? model,
  }) async {
    final body = <String, dynamic>{'goal': goal};
    if (provider != null) body['provider'] = provider;
    if (model != null) body['model'] = model;
    final response = await _send(
      method: 'POST',
      path: '/v1/sessions/$sessionId/runs',
      authenticated: true,
      body: body,
    );
    return RunRecord.fromJson(_decodeObject(response.body));
  }

  Future<RunRecord> getRun(String runId) async {
    final response = await _send(
      method: 'GET',
      path: '/v1/runs/$runId',
      authenticated: true,
    );
    return RunRecord.fromJson(_decodeObject(response.body));
  }

  Future<RunRecord> cancelRun(String runId) async {
    final response = await _send(
      method: 'POST',
      path: '/v1/runs/$runId/cancel',
      authenticated: true,
      body: <String, dynamic>{},
    );
    return RunRecord.fromJson(_decodeObject(response.body));
  }

  /// Cancel Active root then start a new Run with [note] as goal (composer law).
  Future<RunRecord> cancelAndRerun(
    String sessionId, {
    required String activeRunId,
    required String note,
    String? provider,
    String? model,
  }) async {
    await cancelRun(activeRunId);
    return startRun(sessionId, goal: note, provider: provider, model: model);
  }

  /// SSE stream of Run events until terminal (or cancel).
  Stream<RunEvent> streamRunEvents(String runId) async* {
    final uri = _config.resolve('/v1/runs/$runId/events');
    final token = _config.operatorToken;
    if (token == null || token.isEmpty) {
      throw const KeryxAuthException('missing operator token');
    }
    final request = http.Request('GET', uri)
      ..headers.addAll({
        'accept': 'text/event-stream',
        'authorization': 'Bearer $token',
      });
    final streamed = await _http.send(request);
    if (streamed.statusCode == 401 || streamed.statusCode == 403) {
      throw KeryxAuthException('unauthorized SSE', statusCode: streamed.statusCode);
    }
    if (streamed.statusCode < 200 || streamed.statusCode >= 300) {
      throw KeryxHttpException(
        'SSE HTTP ${streamed.statusCode}',
        statusCode: streamed.statusCode,
      );
    }
    final parser = SseParser();
    await for (final chunk in streamed.stream.transform(utf8.decoder)) {
      for (final frame in parser.push(chunk)) {
        if (frame.data.isEmpty) continue;
        final event = RunEvent.fromSse(frame);
        yield event;
        if (event.isTerminal) return;
      }
    }
  }

  Future<List<ApprovalRecord>> listApprovals({bool pending = true}) async {
    final response = await _send(
      method: 'GET',
      path: '/v1/approvals?pending=$pending',
      authenticated: true,
    );
    final obj = _decodeObject(response.body);
    final raw = obj['approvals'] ?? obj;
    final list = raw is List ? raw : (obj['items'] as List? ?? const []);
    return list
        .whereType<Map>()
        .map((e) => ApprovalRecord.fromJson(Map<String, dynamic>.from(e)))
        .toList();
  }

  Future<ApprovalRecord> approveApproval(String id) async {
    final response = await _send(
      method: 'POST',
      path: '/v1/approvals/$id/approve',
      authenticated: true,
      body: <String, dynamic>{},
    );
    return ApprovalRecord.fromJson(_decodeObject(response.body));
  }

  Future<ApprovalRecord> denyApproval(String id) async {
    final response = await _send(
      method: 'POST',
      path: '/v1/approvals/$id/deny',
      authenticated: true,
      body: <String, dynamic>{},
    );
    return ApprovalRecord.fromJson(_decodeObject(response.body));
  }

  Future<List<InboxItem>> listInbox({int limit = 50}) async {
    final response = await _send(
      method: 'GET',
      path: '/v1/inbox?limit=$limit',
      authenticated: true,
    );
    final obj = _decodeObject(response.body);
    final raw = obj['items'] as List? ?? const [];
    return raw
        .whereType<Map>()
        .map((e) => InboxItem.fromJson(Map<String, dynamic>.from(e)))
        .toList();
  }

  Future<List<MemoryEntry>> listMemory({String? query, int limit = 50}) async {
    final qp = <String, String>{'limit': '$limit'};
    if (query != null && query.isNotEmpty) qp['q'] = query;
    final path = Uri(path: '/v1/memory', queryParameters: qp).toString();
    final response = await _send(method: 'GET', path: path, authenticated: true);
    final obj = _decodeObject(response.body);
    final raw = obj['entries'] as List? ?? const [];
    return raw
        .whereType<Map>()
        .map((e) => MemoryEntry.fromJson(Map<String, dynamic>.from(e)))
        .toList();
  }

  Future<MemoryEntry> createMemory({required String content, String? label}) async {
    final body = <String, dynamic>{'content': content};
    if (label != null) body['label'] = label;
    final response = await _send(
      method: 'POST',
      path: '/v1/memory',
      authenticated: true,
      body: body,
    );
    return MemoryEntry.fromJson(_decodeObject(response.body));
  }

  Future<MemoryEntry> updateMemory(
    String id, {
    required String content,
    String? label,
  }) async {
    final body = <String, dynamic>{'content': content};
    if (label != null) body['label'] = label;
    final response = await _send(
      method: 'PUT',
      path: '/v1/memory/$id',
      authenticated: true,
      body: body,
    );
    return MemoryEntry.fromJson(_decodeObject(response.body));
  }

  Future<void> deleteMemory(String id) async {
    await _send(method: 'DELETE', path: '/v1/memory/$id', authenticated: true);
  }

  Future<List<ScheduleRecord>> listSchedules() async {
    final response = await _send(
      method: 'GET',
      path: '/v1/schedules',
      authenticated: true,
    );
    final obj = _decodeObject(response.body);
    final raw = obj['schedules'] as List? ?? const [];
    return raw
        .whereType<Map>()
        .map((e) => ScheduleRecord.fromJson(Map<String, dynamic>.from(e)))
        .toList();
  }

  Future<ScheduleRecord> createSchedule({
    required String goal,
    required int intervalSecs,
    required int nextFireAt,
  }) async {
    final response = await _send(
      method: 'POST',
      path: '/v1/schedules',
      authenticated: true,
      body: {
        'goal': goal,
        'interval_secs': intervalSecs,
        'next_fire_at': nextFireAt,
      },
    );
    return ScheduleRecord.fromJson(_decodeObject(response.body));
  }

  Future<ScheduleRecord> pauseSchedule(String id) async {
    final response = await _send(
      method: 'POST',
      path: '/v1/schedules/$id/pause',
      authenticated: true,
      body: <String, dynamic>{},
    );
    return ScheduleRecord.fromJson(_decodeObject(response.body));
  }

  Future<ScheduleRecord> resumeSchedule(String id) async {
    final response = await _send(
      method: 'POST',
      path: '/v1/schedules/$id/resume',
      authenticated: true,
      body: <String, dynamic>{},
    );
    return ScheduleRecord.fromJson(_decodeObject(response.body));
  }

  Future<ScheduleRecord> deleteSchedule(String id) async {
    final response = await _send(
      method: 'POST',
      path: '/v1/schedules/$id/delete',
      authenticated: true,
      body: <String, dynamic>{},
    );
    return ScheduleRecord.fromJson(_decodeObject(response.body));
  }

  Future<List<SkillSummary>> listSkills() async {
    final response = await _send(
      method: 'GET',
      path: '/v1/skills',
      authenticated: true,
    );
    final obj = _decodeObject(response.body);
    final raw = obj['skills'] as List? ?? const [];
    return raw
        .whereType<Map>()
        .map((e) => SkillSummary.fromJson(Map<String, dynamic>.from(e)))
        .toList();
  }

  Future<SkillDetail> getSkill(String name) async {
    final response = await _send(
      method: 'GET',
      path: '/v1/skills/$name',
      authenticated: true,
    );
    return SkillDetail.fromJson(_decodeObject(response.body));
  }

  Future<ArtifactMeta> getArtifactMeta(String id) async {
    final uri = _config.resolve('/v1/artifacts/$id');
    final token = _config.operatorToken;
    if (token == null || token.isEmpty) {
      throw const KeryxAuthException('missing operator token');
    }
    final response = await _http.get(
      uri,
      headers: {
        'authorization': 'Bearer $token',
        'accept': 'application/json',
      },
    ).timeout(_config.timeout);
    if (response.statusCode == 401 || response.statusCode == 403) {
      throw KeryxAuthException('unauthorized', statusCode: response.statusCode);
    }
    if (response.statusCode < 200 || response.statusCode >= 300) {
      throw KeryxHttpException(
        'artifact meta HTTP ${response.statusCode}',
        statusCode: response.statusCode,
      );
    }
    return ArtifactMeta.fromJson(_decodeObject(response.body));
  }

  Future<List<int>> getArtifactBytes(String id) async {
    final uri = _config.resolve('/v1/artifacts/$id');
    final token = _config.operatorToken;
    if (token == null || token.isEmpty) {
      throw const KeryxAuthException('missing operator token');
    }
    final response = await _http.get(
      uri,
      headers: {
        'authorization': 'Bearer $token',
        'accept': 'application/octet-stream',
      },
    ).timeout(_config.timeout);
    if (response.statusCode == 401 || response.statusCode == 403) {
      throw KeryxAuthException('unauthorized', statusCode: response.statusCode);
    }
    if (response.statusCode < 200 || response.statusCode >= 300) {
      throw KeryxHttpException(
        'artifact HTTP ${response.statusCode}',
        statusCode: response.statusCode,
      );
    }
    return response.bodyBytes;
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
