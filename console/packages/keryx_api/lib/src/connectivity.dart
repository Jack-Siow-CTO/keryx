import 'client.dart';
import 'errors.dart';

/// Outcome of Console connectivity / health check (spec US 14).
enum ConnectivityKind {
  /// Worker `/health` and authenticated probe both succeeded.
  ok,

  /// Could not reach Worker (network / DNS / TLS / timeout).
  unreachable,

  /// Worker is up but operator token is missing/invalid (HTTP 401).
  authFailure,

  /// Worker responded with an unexpected non-auth error.
  unexpected,
}

/// Structured result for Settings health UI.
final class ConnectivityResult {
  const ConnectivityResult({
    required this.kind,
    required this.message,
    this.healthOk,
  });

  final ConnectivityKind kind;
  final String message;

  /// Whether unauthenticated `/health` returned ok, when known.
  final bool? healthOk;

  bool get isOk => kind == ConnectivityKind.ok;

  factory ConnectivityResult.ok() => const ConnectivityResult(
        kind: ConnectivityKind.ok,
        message: 'Connected to Worker',
        healthOk: true,
      );

  factory ConnectivityResult.unreachable(Object error) => ConnectivityResult(
        kind: ConnectivityKind.unreachable,
        message: 'Cannot reach Worker: $error',
        healthOk: false,
      );

  factory ConnectivityResult.authFailure(String detail) => ConnectivityResult(
        kind: ConnectivityKind.authFailure,
        message: 'Authentication failed: $detail',
        healthOk: true,
      );

  factory ConnectivityResult.unexpected(String detail, {bool? healthOk}) =>
      ConnectivityResult(
        kind: ConnectivityKind.unexpected,
        message: detail,
        healthOk: healthOk,
      );
}

/// Distinguishes unreachable Worker vs invalid token (ticket #38).
///
/// 1. `GET /health` (no auth) — if this fails, [ConnectivityKind.unreachable].
/// 2. `GET /v1/providers` (bearer) — 401 → [ConnectivityKind.authFailure].
Future<ConnectivityResult> checkConnectivity(KeryxApiClient client) async {
  try {
    final health = await client.getHealth();
    if (!health.isOk) {
      return ConnectivityResult.unexpected(
        'Unexpected health status: ${health.status}',
        healthOk: false,
      );
    }
  } on KeryxUnreachableException catch (e) {
    return ConnectivityResult.unreachable(e.message);
  } on KeryxApiException catch (e) {
    return ConnectivityResult.unreachable(e.message);
  }

  try {
    await client.listProviders();
    return ConnectivityResult.ok();
  } on KeryxAuthException catch (e) {
    return ConnectivityResult.authFailure(e.message);
  } on KeryxUnreachableException catch (e) {
    // Health worked but authenticated call failed transport-wise.
    return ConnectivityResult.unreachable(e.message);
  } on KeryxHttpException catch (e) {
    return ConnectivityResult.unexpected(
      'Worker error ${e.statusCode}: ${e.message}',
      healthOk: true,
    );
  } on KeryxApiException catch (e) {
    return ConnectivityResult.unexpected(e.message, healthOk: true);
  }
}
