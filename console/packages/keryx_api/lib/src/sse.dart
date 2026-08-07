import 'dart:convert';

/// One Server-Sent Event frame (ADR 0007 / Console live Run stream).
final class SseFrame {
  const SseFrame({
    this.event,
    this.id,
    required this.data,
  });

  final String? event;
  final String? id;
  final String data;
}

/// Parses SSE text chunks into [SseFrame]s (handles multi-line data).
final class SseParser {
  final StringBuffer _buf = StringBuffer();

  /// Feed raw bytes/string from the HTTP body; returns completed frames.
  List<SseFrame> push(String chunk) {
    _buf.write(chunk);
    final text = _buf.toString();
    final parts = text.split('\n\n');
    // Keep incomplete trailing fragment.
    if (!text.endsWith('\n\n')) {
      _buf
        ..clear()
        ..write(parts.isEmpty ? '' : parts.removeLast());
    } else {
      _buf.clear();
      // trailing empty from split
      if (parts.isNotEmpty && parts.last.isEmpty) {
        parts.removeLast();
      }
    }

    final frames = <SseFrame>[];
    for (final block in parts) {
      if (block.trim().isEmpty) continue;
      String? event;
      String? id;
      final dataLines = <String>[];
      for (final line in block.split('\n')) {
        if (line.startsWith(':')) continue; // comment
        if (line.startsWith('event:')) {
          event = line.substring(6).trim();
        } else if (line.startsWith('id:')) {
          id = line.substring(3).trim();
        } else if (line.startsWith('data:')) {
          dataLines.add(line.substring(5).trimLeft());
        }
      }
      if (dataLines.isEmpty && event == null) continue;
      frames.add(SseFrame(
        event: event,
        id: id,
        data: dataLines.join('\n'),
      ));
    }
    return frames;
  }
}

/// Wire shape of a control-plane Run event SSE data payload.
final class RunEvent {
  const RunEvent({
    required this.runId,
    required this.seq,
    required this.type,
    required this.raw,
    this.payload = const {},
  });

  final String runId;
  final int seq;
  /// Event type from `kind.type` or SSE `event:` name.
  final String type;
  final Map<String, dynamic> raw;
  final Map<String, dynamic> payload;

  bool get isTerminal =>
      type == 'run_completed' ||
      type == 'run.completed' ||
      type == 'run_failed' ||
      type == 'run.failed' ||
      type == 'run_cancelled' ||
      type == 'run.cancelled' ||
      type.endsWith('completed') && type.contains('run') ||
      type == 'RunCompleted' ||
      _terminalFromKind;

  bool get _terminalFromKind {
    final k = raw['kind'];
    if (k is Map && k['type'] is String) {
      final t = k['type'] as String;
      return t == 'run_completed' ||
          t == 'run_failed' ||
          t == 'run_cancelled';
    }
    return false;
  }

  String? get deltaText {
    final k = raw['kind'];
    if (k is Map && k['type'] == 'model_delta') {
      final p = k['payload'];
      if (p is Map && p['text'] is String) return p['text'] as String;
    }
    return null;
  }

  String? get toolName {
    final p = _kindPayload;
    if (p == null) return null;
    final t = type.toLowerCase().replaceAll('.', '_');
    if (t == 'tool_started' || t == 'tool_finished') {
      final name = p['name'];
      if (name is String && name.isNotEmpty) return name;
    }
    return null;
  }

  bool get isToolStarted {
    final t = type.toLowerCase().replaceAll('.', '_');
    return t == 'tool_started';
  }

  bool get isToolFinished {
    final t = type.toLowerCase().replaceAll('.', '_');
    return t == 'tool_finished';
  }

  bool get isChildRunStarted {
    final t = type.toLowerCase().replaceAll('.', '_');
    return t == 'child_run_started';
  }

  bool get isChildRunFinished {
    final t = type.toLowerCase().replaceAll('.', '_');
    return t == 'child_run_finished';
  }

  bool get isApprovalWaiting {
    final t = type.toLowerCase().replaceAll('.', '_');
    return t == 'approval_waiting';
  }

  bool get isApprovalResolved {
    final t = type.toLowerCase().replaceAll('.', '_');
    return t == 'approval_resolved';
  }

  String? get childRunId {
    final p = _kindPayload;
    if (p == null) return null;
    final id = p['child_run_id'];
    return id is String && id.isNotEmpty ? id : null;
  }

  String? get childRunGoal {
    final p = _kindPayload;
    if (p == null) return null;
    final g = p['goal'];
    return g is String ? g : null;
  }

  String? get childRunTerminalStatus {
    final p = _kindPayload;
    if (p == null) return null;
    final s = p['status'];
    return s is String ? s : null;
  }

  String? get approvalAction {
    final p = _kindPayload;
    if (p == null) return null;
    final a = p['action'];
    return a is String ? a : null;
  }

  String? get approvalSummary {
    final p = _kindPayload;
    if (p == null) return null;
    final s = p['summary'];
    return s is String ? s : null;
  }

  String? get approvalDecision {
    final p = _kindPayload;
    if (p == null) return null;
    final d = p['decision'];
    return d is String ? d : null;
  }

  String? get budgetMessage {
    final p = _kindPayload;
    if (p == null) return null;
    final m = p['message'];
    return m is String ? m : null;
  }

  String? get failureReason {
    final p = _kindPayload;
    if (p == null) return null;
    final r = p['reason'];
    return r is String ? r : null;
  }

  Map<String, dynamic>? get _kindPayload {
    final k = raw['kind'];
    if (k is! Map) return null;
    final p = k['payload'];
    if (p is Map) return Map<String, dynamic>.from(p);
    // Some payloads are inlined beside type under kind.
    return Map<String, dynamic>.from(k);
  }

  factory RunEvent.fromSse(SseFrame frame) {
    final decoded = jsonDecode(frame.data);
    if (decoded is! Map) {
      throw FormatException('RunEvent data must be object: ${frame.data}');
    }
    final map = Map<String, dynamic>.from(decoded);
    final kind = map['kind'];
    String type = frame.event ?? 'message';
    Map<String, dynamic> payload = {};
    if (kind is Map) {
      final km = Map<String, dynamic>.from(kind);
      if (km['type'] is String) type = km['type'] as String;
      if (km['payload'] is Map) {
        payload = Map<String, dynamic>.from(km['payload'] as Map);
      }
    }
    return RunEvent(
      runId: map['run_id'] as String? ?? '',
      seq: (map['seq'] as num?)?.toInt() ?? 0,
      type: type,
      raw: map,
      payload: payload,
    );
  }
}
