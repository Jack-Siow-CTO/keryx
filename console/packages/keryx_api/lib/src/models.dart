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
