import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:keryx_api/keryx_api.dart';

import '../../core/credentials_store.dart';
import '../../core/device_lock.dart';
import '../../core/session_cache.dart';

enum AuthStatus {
  /// Bootstrapping from secure storage.
  unknown,

  /// No base URL / token configured.
  unconfigured,

  /// Credentials present; biometric/device lock required.
  locked,

  /// Ready to use messaging shell.
  ready,
}

final class AuthState {
  const AuthState({
    required this.status,
    this.baseUrl,
    this.biometricLockEnabled = false,
    this.lastConnectivity,
    this.errorMessage,
  });

  final AuthStatus status;
  final String? baseUrl;
  final bool biometricLockEnabled;
  final ConnectivityResult? lastConnectivity;
  final String? errorMessage;

  /// Operator token is intentionally never exposed on AuthState for UI logging.
  AuthState copyWith({
    AuthStatus? status,
    String? baseUrl,
    bool? biometricLockEnabled,
    ConnectivityResult? lastConnectivity,
    String? errorMessage,
    bool clearError = false,
    bool clearConnectivity = false,
  }) {
    return AuthState(
      status: status ?? this.status,
      baseUrl: baseUrl ?? this.baseUrl,
      biometricLockEnabled:
          biometricLockEnabled ?? this.biometricLockEnabled,
      lastConnectivity: clearConnectivity
          ? null
          : (lastConnectivity ?? this.lastConnectivity),
      errorMessage: clearError ? null : (errorMessage ?? this.errorMessage),
    );
  }
}

final authControllerProvider =
    StateNotifierProvider<AuthController, AuthState>((ref) {
  return AuthController(ref)..bootstrap();
});

class AuthController extends StateNotifier<AuthState> {
  AuthController(this._ref)
      : super(const AuthState(status: AuthStatus.unknown));

  final Ref _ref;
  String? _operatorToken;
  KeryxApiClient? _client;

  CredentialsStore get _store => _ref.read(credentialsStoreProvider);
  DeviceLock get _lock => _ref.read(deviceLockProvider);
  SessionCache get _cache => _ref.read(sessionCacheProvider);

  KeryxApiClient? get client => _client;

  Future<void> bootstrap() async {
    try {
      final stored = await _store.load();
      if (stored == null) {
        state = const AuthState(status: AuthStatus.unconfigured);
        return;
      }
      _operatorToken = stored.operatorToken;
      _rebuildClient(stored.baseUrl, stored.operatorToken);
      if (stored.biometricLockEnabled) {
        state = AuthState(
          status: AuthStatus.locked,
          baseUrl: stored.baseUrl,
          biometricLockEnabled: true,
        );
      } else {
        state = AuthState(
          status: AuthStatus.ready,
          baseUrl: stored.baseUrl,
          biometricLockEnabled: false,
        );
      }
    } catch (e) {
      state = AuthState(
        status: AuthStatus.unconfigured,
        errorMessage: 'Failed to load credentials: $e',
      );
    }
  }

  /// Save credentials after a successful connectivity probe when [probe] is true.
  ///
  /// Invalid tokens fail closed: no secret persist, no shell entry, error UI only.
  Future<ConnectivityResult?> saveConnection({
    required String baseUrl,
    required String operatorToken,
    bool biometricLockEnabled = false,
    bool probe = true,
  }) async {
    final trimmedUrl = baseUrl.trim();
    final trimmedToken = operatorToken.trim();
    if (trimmedUrl.isEmpty || trimmedToken.isEmpty) {
      state = state.copyWith(
        errorMessage: 'Base URL and operator token are required',
      );
      return null;
    }

    if (biometricLockEnabled) {
      final supported = await _lock.isSupported;
      if (!supported) {
        state = state.copyWith(
          errorMessage:
              'Device unlock is not available on this device; disable the lock or use a device with biometrics/credentials',
        );
        return null;
      }
    }

    // Probe with ephemeral client before writing secrets.
    if (probe) {
      final probeClient = KeryxApiClient(
        config: KeryxApiConfig(
          baseUrl: trimmedUrl,
          operatorToken: trimmedToken,
        ),
      );
      try {
        final result = await checkConnectivity(probeClient);
        if (!result.isOk) {
          state = state.copyWith(
            lastConnectivity: result,
            errorMessage: result.message,
          );
          return result;
        }
        state = state.copyWith(
          lastConnectivity: result,
          clearError: true,
        );
      } catch (e) {
        state = state.copyWith(
          errorMessage: 'Connectivity check failed: $e',
        );
        return null;
      } finally {
        probeClient.close();
      }
    }

    try {
      await _store.save(
        baseUrl: trimmedUrl,
        operatorToken: trimmedToken,
        biometricLockEnabled: biometricLockEnabled,
      );
    } catch (e) {
      state = state.copyWith(
        errorMessage: 'Could not save credentials: $e',
      );
      return null;
    }
    _operatorToken = trimmedToken;
    _rebuildClient(trimmedUrl, trimmedToken);

    if (biometricLockEnabled) {
      state = AuthState(
        status: AuthStatus.locked,
        baseUrl: trimmedUrl,
        biometricLockEnabled: true,
        lastConnectivity: state.lastConnectivity,
      );
    } else {
      state = AuthState(
        status: AuthStatus.ready,
        baseUrl: trimmedUrl,
        biometricLockEnabled: false,
        lastConnectivity: state.lastConnectivity,
      );
    }
    return state.lastConnectivity;
  }

  /// Update base URL while keeping the existing token (Settings URL-only save).
  Future<ConnectivityResult?> updateBaseUrl(String baseUrl) async {
    final token = _operatorToken;
    if (token == null || token.isEmpty) {
      state = state.copyWith(errorMessage: 'No operator token stored');
      return null;
    }
    return saveConnection(
      baseUrl: baseUrl,
      operatorToken: token,
      biometricLockEnabled: state.biometricLockEnabled,
      probe: true,
    );
  }

  Future<bool> unlock() async {
    if (!state.biometricLockEnabled) {
      state = AuthState(
        status: AuthStatus.ready,
        baseUrl: state.baseUrl,
        biometricLockEnabled: false,
      );
      return true;
    }
    final supported = await _lock.isSupported;
    if (!supported) {
      state = state.copyWith(
        errorMessage:
            'Device unlock is required but this device cannot authenticate. Log out or enable biometrics/device credentials.',
      );
      return false;
    }
    final ok = await _lock.authenticate();
    if (!ok) {
      state = state.copyWith(errorMessage: 'Unlock failed');
      return false;
    }
    state = AuthState(
      status: AuthStatus.ready,
      baseUrl: state.baseUrl,
      biometricLockEnabled: state.biometricLockEnabled,
    );
    return true;
  }

  Future<void> setBiometricLockEnabled(bool enabled) async {
    if (enabled) {
      final supported = await _lock.isSupported;
      if (!supported) {
        state = state.copyWith(
          errorMessage:
              'Device unlock is not available on this device',
        );
        return;
      }
    }
    await _store.setBiometricLockEnabled(enabled);
    state = state.copyWith(
      biometricLockEnabled: enabled,
      clearError: true,
    );
  }

  Future<ConnectivityResult> checkHealth() async {
    final client = _client;
    if (client == null) {
      final result = ConnectivityResult.unexpected(
        'Not configured',
        healthOk: false,
      );
      state = state.copyWith(lastConnectivity: result);
      return result;
    }
    final result = await checkConnectivity(client);
    state = state.copyWith(
      lastConnectivity: result,
      clearError: true,
      errorMessage: result.isOk ? null : result.message,
    );
    return result;
  }

  /// Logout: delete secret + caches. No residual Principal access.
  Future<void> logout() async {
    _client?.close();
    _client = null;
    _operatorToken = null;
    _cache.clear();
    await _store.clearAll();
    state = const AuthState(status: AuthStatus.unconfigured);
  }

  void _rebuildClient(String baseUrl, String token) {
    _client?.close();
    _client = KeryxApiClient(
      config: KeryxApiConfig(
        baseUrl: baseUrl,
        operatorToken: token,
      ),
    );
  }

  @override
  void dispose() {
    _client?.close();
    super.dispose();
  }
}
