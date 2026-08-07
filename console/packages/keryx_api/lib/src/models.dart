/// `GET /health` body.
final class HealthStatus {
  const HealthStatus({required this.status});

  final String status;

  bool get isOk => status == 'ok';

  factory HealthStatus.fromJson(Map<String, dynamic> json) {
    final status = json['status'];
    if (status is! String) {
      throw const FormatException('HealthStatus.status must be a string');
    }
    return HealthStatus(status: status);
  }

  Map<String, dynamic> toJson() => {'status': status};
}

/// One registered (or known) model provider entry — no secrets.
///
/// Matches Worker `ProviderInfo` (`crates/api/src/state.rs`).
final class ProviderInfo {
  const ProviderInfo({
    required this.name,
    required this.authKind,
    required this.displayName,
    required this.defaultModel,
    this.models = const [],
    required this.registered,
    required this.supportsModelOverride,
  });

  final String name;
  final String authKind;
  final String displayName;
  final String defaultModel;
  final List<String> models;
  final bool registered;
  final bool supportsModelOverride;

  factory ProviderInfo.fromJson(Map<String, dynamic> json) {
    String requireString(String key) {
      final v = json[key];
      if (v is! String) {
        throw FormatException('ProviderInfo.$key must be a string');
      }
      return v;
    }

    bool requireBool(String key) {
      final v = json[key];
      if (v is! bool) {
        throw FormatException('ProviderInfo.$key must be a bool');
      }
      return v;
    }

    final rawModels = json['models'];
    final models = <String>[];
    if (rawModels is List) {
      for (final m in rawModels) {
        if (m is String) models.add(m);
      }
    }

    return ProviderInfo(
      name: requireString('name'),
      authKind: requireString('auth_kind'),
      displayName: requireString('display_name'),
      defaultModel: requireString('default_model'),
      models: models,
      registered: requireBool('registered'),
      supportsModelOverride: requireBool('supports_model_override'),
    );
  }

  Map<String, dynamic> toJson() => {
        'name': name,
        'auth_kind': authKind,
        'display_name': displayName,
        'default_model': defaultModel,
        'models': models,
        'registered': registered,
        'supports_model_override': supportsModelOverride,
      };
}

/// `GET /v1/providers` body.
final class ProvidersResponse {
  const ProvidersResponse({this.defaultProvider, required this.providers});

  final String? defaultProvider;
  final List<ProviderInfo> providers;

  factory ProvidersResponse.fromJson(Map<String, dynamic> json) {
    final defaultProvider = json['default'] as String?;
    final raw = json['providers'];
    if (raw is! List) {
      throw const FormatException('ProvidersResponse.providers must be a list');
    }
    final providers = raw
        .whereType<Map>()
        .map((e) => ProviderInfo.fromJson(Map<String, dynamic>.from(e)))
        .toList();
    return ProvidersResponse(
      defaultProvider: defaultProvider,
      providers: providers,
    );
  }

  Map<String, dynamic> toJson() => {
        'default': defaultProvider,
        'providers': providers.map((p) => p.toJson()).toList(),
      };
}

/// Active root Run chip on Session list/detail (ADR 0027).
final class ActiveRootRunSummary {
  const ActiveRootRunSummary({
    required this.id,
    required this.goal,
    required this.status,
    required this.origin,
  });

  final String id;
  final String goal;
  final String status;
  final String origin;

  factory ActiveRootRunSummary.fromJson(Map<String, dynamic> json) {
    return ActiveRootRunSummary(
      id: json['id'] as String,
      goal: json['goal'] as String,
      status: json['status'] as String,
      origin: json['origin'] as String,
    );
  }

  Map<String, dynamic> toJson() => {
        'id': id,
        'goal': goal,
        'status': status,
        'origin': origin,
      };
}

/// Operator Session projection for list/detail.
final class SessionSummary {
  const SessionSummary({
    required this.id,
    required this.principalId,
    required this.title,
    required this.titleIsCustom,
    required this.createdAt,
    required this.updatedAt,
    this.lastMessagePreview,
    this.activeRootRun,
    required this.pendingApprovalCount,
  });

  final String id;
  final String principalId;
  final String title;
  final bool titleIsCustom;
  final int createdAt;
  final int updatedAt;
  final String? lastMessagePreview;
  final ActiveRootRunSummary? activeRootRun;
  final int pendingApprovalCount;

  factory SessionSummary.fromJson(Map<String, dynamic> json) {
    final active = json['active_root_run'];
    return SessionSummary(
      id: json['id'] as String,
      principalId: json['principal_id'] as String,
      title: json['title'] as String,
      titleIsCustom: json['title_is_custom'] as bool? ?? false,
      createdAt: (json['created_at'] as num).toInt(),
      updatedAt: (json['updated_at'] as num).toInt(),
      lastMessagePreview: json['last_message_preview'] as String?,
      activeRootRun: active is Map
          ? ActiveRootRunSummary.fromJson(Map<String, dynamic>.from(active))
          : null,
      pendingApprovalCount:
          (json['pending_approval_count'] as num?)?.toInt() ?? 0,
    );
  }

  Map<String, dynamic> toJson() => {
        'id': id,
        'principal_id': principalId,
        'title': title,
        'title_is_custom': titleIsCustom,
        'created_at': createdAt,
        'updated_at': updatedAt,
        'last_message_preview': lastMessagePreview,
        'active_root_run': activeRootRun?.toJson(),
        'pending_approval_count': pendingApprovalCount,
      };
}

final class SessionListResponse {
  const SessionListResponse({required this.sessions});

  final List<SessionSummary> sessions;

  factory SessionListResponse.fromJson(Map<String, dynamic> json) {
    final raw = json['sessions'];
    if (raw is! List) {
      throw const FormatException('SessionListResponse.sessions must be a list');
    }
    return SessionListResponse(
      sessions: raw
          .whereType<Map>()
          .map((e) => SessionSummary.fromJson(Map<String, dynamic>.from(e)))
          .toList(),
    );
  }
}

/// Compact tool row in Transcript (ADR 0025).
final class ToolCompact {
  const ToolCompact({
    required this.name,
    required this.status,
    required this.summary,
    this.artifactRefs = const [],
  });

  final String name;
  final String status;
  final String summary;
  final List<String> artifactRefs;

  factory ToolCompact.fromJson(Map<String, dynamic> json) {
    final refs = json['artifact_refs'];
    return ToolCompact(
      name: json['name'] as String,
      status: json['status'] as String,
      summary: json['summary'] as String? ?? '',
      artifactRefs: refs is List
          ? refs.whereType<String>().toList()
          : const [],
    );
  }
}

/// Structured Transcript message (Console conversation SoR from Worker).
final class TranscriptMessage {
  const TranscriptMessage({
    required this.id,
    this.runId,
    required this.createdAt,
    required this.role,
    required this.content,
    this.tool,
  });

  final String id;
  final String? runId;
  final int createdAt;
  final String role;
  final String content;
  final ToolCompact? tool;

  bool get isTool => role == 'tool' || tool != null;

  factory TranscriptMessage.fromJson(Map<String, dynamic> json) {
    final tool = json['tool'];
    return TranscriptMessage(
      id: json['id'] as String,
      runId: json['run_id'] as String?,
      createdAt: (json['created_at'] as num).toInt(),
      role: json['role'] as String,
      content: json['content'] as String? ?? '',
      tool: tool is Map
          ? ToolCompact.fromJson(Map<String, dynamic>.from(tool))
          : null,
    );
  }
}

final class TranscriptPage {
  const TranscriptPage({
    required this.sessionId,
    required this.messages,
    this.nextBefore,
  });

  final String sessionId;
  /// Newest-first page.
  final List<TranscriptMessage> messages;
  final String? nextBefore;

  factory TranscriptPage.fromJson(Map<String, dynamic> json) {
    final raw = json['messages'];
    if (raw is! List) {
      throw const FormatException('TranscriptPage.messages must be a list');
    }
    return TranscriptPage(
      sessionId: json['session_id'] as String,
      messages: raw
          .whereType<Map>()
          .map((e) => TranscriptMessage.fromJson(Map<String, dynamic>.from(e)))
          .toList(),
      nextBefore: json['next_before'] as String?,
    );
  }
}

/// Control-plane Run record.
final class RunRecord {
  const RunRecord({
    required this.id,
    required this.sessionId,
    required this.principalId,
    required this.goal,
    required this.status,
    required this.origin,
    this.parentRunId,
    this.result,
  });

  final String id;
  final String sessionId;
  final String principalId;
  final String goal;
  final String status;
  final String origin;
  final String? parentRunId;
  final String? result;

  bool get isActive => status == 'active';
  bool get isTerminal =>
      status == 'completed' ||
      status == 'failed' ||
      status == 'cancelled' ||
      status == 'interrupted';

  factory RunRecord.fromJson(Map<String, dynamic> json) => RunRecord(
        id: json['id'] as String,
        sessionId: json['session_id'] as String,
        principalId: json['principal_id'] as String,
        goal: json['goal'] as String,
        status: json['status'] as String,
        origin: json['origin'] as String? ?? 'control_plane',
        parentRunId: json['parent_run_id'] as String?,
        result: json['result'] as String?,
      );
}

/// Control-plane Approval (OpenAPI `ApprovalResponse`).
final class ApprovalRecord {
  const ApprovalRecord({
    required this.id,
    required this.runId,
    required this.action,
    required this.summary,
    required this.status,
    required this.requestedBy,
    this.decidedBy,
  });

  final String id;
  final String runId;
  final String action;
  final String summary;
  /// One of: pending, approved, denied.
  final String status;
  final String requestedBy;
  final String? decidedBy;

  bool get isPending => status == 'pending';
  bool get isResolved => status == 'approved' || status == 'denied';

  factory ApprovalRecord.fromJson(Map<String, dynamic> json) => ApprovalRecord(
        id: json['id'] as String,
        runId: json['run_id'] as String,
        action: json['action'] as String,
        summary: json['summary'] as String,
        status: json['status'] as String,
        requestedBy: json['requested_by'] as String,
        decidedBy: json['decided_by'] as String?,
      );

  Map<String, dynamic> toJson() => {
        'id': id,
        'run_id': runId,
        'action': action,
        'summary': summary,
        'status': status,
        'requested_by': requestedBy,
        'decided_by': decidedBy,
      };
}

/// `GET /v1/approvals` body (OpenAPI `ApprovalsListResponse`).
final class ApprovalsListResponse {
  const ApprovalsListResponse({required this.approvals});

  final List<ApprovalRecord> approvals;

  factory ApprovalsListResponse.fromJson(Map<String, dynamic> json) {
    final raw = json['approvals'];
    if (raw is! List) {
      throw const FormatException(
        'ApprovalsListResponse.approvals must be a list',
      );
    }
    return ApprovalsListResponse(
      approvals: raw
          .whereType<Map>()
          .map((e) => ApprovalRecord.fromJson(Map<String, dynamic>.from(e)))
          .toList(),
    );
  }
}

final class InboxItem {
  const InboxItem({
    required this.id,
    required this.kind,
    this.sessionId,
    this.runId,
    this.approvalId,
    required this.title,
    required this.summary,
    required this.createdAt,
  });

  final String id;
  final String kind;
  final String? sessionId;
  final String? runId;
  final String? approvalId;
  final String title;
  final String summary;
  final int createdAt;

  factory InboxItem.fromJson(Map<String, dynamic> json) => InboxItem(
        id: json['id'] as String,
        kind: json['kind'] as String,
        sessionId: json['session_id'] as String?,
        runId: json['run_id'] as String?,
        approvalId: json['approval_id'] as String?,
        title: json['title'] as String,
        summary: json['summary'] as String? ?? '',
        createdAt: (json['created_at'] as num?)?.toInt() ?? 0,
      );
}

final class MemoryEntry {
  const MemoryEntry({
    required this.id,
    required this.content,
    this.label,
    this.sourceRunId,
    this.sourcePrincipalId,
  });

  final String id;
  final String content;
  final String? label;
  final String? sourceRunId;
  final String? sourcePrincipalId;

  factory MemoryEntry.fromJson(Map<String, dynamic> json) => MemoryEntry(
        id: json['id'] as String,
        content: json['content'] as String,
        label: json['label'] as String?,
        sourceRunId: json['source_run_id'] as String?,
        sourcePrincipalId: json['source_principal_id'] as String?,
      );
}

final class ScheduleRecord {
  const ScheduleRecord({
    required this.id,
    required this.principalId,
    this.sessionId,
    required this.goal,
    required this.intervalSecs,
    required this.status,
    required this.nextFireAt,
  });

  final String id;
  final String principalId;
  final String? sessionId;
  final String goal;
  final int intervalSecs;
  final String status;
  final int nextFireAt;

  factory ScheduleRecord.fromJson(Map<String, dynamic> json) => ScheduleRecord(
        id: json['id'] as String,
        principalId: json['principal_id'] as String,
        sessionId: json['session_id'] as String?,
        goal: json['goal'] as String,
        intervalSecs: (json['interval_secs'] as num).toInt(),
        status: json['status'] as String,
        nextFireAt: (json['next_fire_at'] as num).toInt(),
      );
}

final class SkillSummary {
  const SkillSummary({required this.name});
  final String name;
  factory SkillSummary.fromJson(Map<String, dynamic> json) =>
      SkillSummary(name: json['name'] as String);
}

final class SkillDetail {
  const SkillDetail({required this.name, required this.content});
  final String name;
  final String content;
  factory SkillDetail.fromJson(Map<String, dynamic> json) => SkillDetail(
        name: json['name'] as String,
        content: json['content'] as String,
      );
}

final class ArtifactMeta {
  const ArtifactMeta({
    required this.id,
    required this.kind,
    required this.mediaType,
    required this.byteLen,
    required this.createdAt,
    this.runId,
    this.sessionId,
    required this.summary,
  });

  final String id;
  final String kind;
  final String mediaType;
  final int byteLen;
  final int createdAt;
  final String? runId;
  final String? sessionId;
  final String summary;

  factory ArtifactMeta.fromJson(Map<String, dynamic> json) => ArtifactMeta(
        id: json['id'] as String,
        kind: json['kind'] as String,
        mediaType: json['media_type'] as String,
        byteLen: (json['byte_len'] as num).toInt(),
        createdAt: (json['created_at'] as num).toInt(),
        runId: json['run_id'] as String?,
        sessionId: json['session_id'] as String?,
        summary: json['summary'] as String? ?? '',
      );
}
