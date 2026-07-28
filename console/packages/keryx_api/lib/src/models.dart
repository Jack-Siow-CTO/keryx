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
