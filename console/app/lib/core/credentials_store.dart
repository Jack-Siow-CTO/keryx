import 'package:flutter/foundation.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_secure_storage/flutter_secure_storage.dart';
import 'package:shared_preferences/shared_preferences.dart';

/// Non-secret connection prefs + secure operator token storage (ADR 0021).
///
/// Token is **never** written to SharedPreferences / plaintext prefs.
abstract class CredentialsStore {
  Future<StoredConnection?> load();

  /// Persist base URL (prefs) + operator token (secure storage).
  Future<void> save({
    required String baseUrl,
    required String operatorToken,
    required bool biometricLockEnabled,
  });

  Future<void> setBiometricLockEnabled(bool enabled);

  /// Delete token from secure storage and clear local caches/prefs.
  Future<void> clearAll();
}

final class StoredConnection {
  const StoredConnection({
    required this.baseUrl,
    required this.operatorToken,
    required this.biometricLockEnabled,
  });

  final String baseUrl;
  final String operatorToken;
  final bool biometricLockEnabled;
}

const _kBaseUrl = 'keryx.base_url';
const _kBiometric = 'keryx.biometric_lock';
const _kTokenKey = 'keryx.operator_token';

/// Production store: SharedPreferences for non-secrets, Keychain/Keystore for token.
final class SecureCredentialsStore implements CredentialsStore {
  SecureCredentialsStore({
    FlutterSecureStorage? secureStorage,
    SharedPreferences? prefs,
  })  : _secure = secureStorage ??
            const FlutterSecureStorage(
              aOptions: AndroidOptions(encryptedSharedPreferences: true),
              // Avoid Data Protection Keychain path that needs extra macOS
              // entitlements on some debug-signed builds (-34018).
              mOptions: MacOsOptions(
                accessibility: KeychainAccessibility.first_unlock,
                useDataProtectionKeyChain: false,
              ),
              iOptions: IOSOptions(
                accessibility: KeychainAccessibility.first_unlock,
              ),
            ),
        _prefsOverride = prefs;

  final FlutterSecureStorage _secure;
  final SharedPreferences? _prefsOverride;

  Future<SharedPreferences> get _prefs async =>
      _prefsOverride ?? await SharedPreferences.getInstance();

  @override
  Future<StoredConnection?> load() async {
    final prefs = await _prefs;
    final baseUrl = prefs.getString(_kBaseUrl);
    final biometric = prefs.getBool(_kBiometric) ?? false;
    String? token;
    try {
      token = await _secure.read(key: _kTokenKey);
    } on PlatformException catch (e) {
      debugPrint('secure storage read failed: ${e.code} ${e.message}');
      rethrow;
    }
    if (baseUrl == null ||
        baseUrl.isEmpty ||
        token == null ||
        token.isEmpty) {
      return null;
    }
    return StoredConnection(
      baseUrl: baseUrl,
      operatorToken: token,
      biometricLockEnabled: biometric,
    );
  }

  @override
  Future<void> save({
    required String baseUrl,
    required String operatorToken,
    required bool biometricLockEnabled,
  }) async {
    final prefs = await _prefs;
    await prefs.setString(_kBaseUrl, baseUrl.trim());
    await prefs.setBool(_kBiometric, biometricLockEnabled);
    // Token only in secure storage — never prefs.
    try {
      await _secure.write(key: _kTokenKey, value: operatorToken);
    } on PlatformException catch (e) {
      // Surface as a clear operator-facing failure (Connect UI shows errorMessage).
      throw StateError(
        'Could not store operator token in Keychain '
        '(${e.code}: ${e.message}). '
        'On macOS ensure the app is signed and keychain-access-groups is entitled.',
      );
    }
  }

  @override
  Future<void> setBiometricLockEnabled(bool enabled) async {
    final prefs = await _prefs;
    await prefs.setBool(_kBiometric, enabled);
  }

  @override
  Future<void> clearAll() async {
    final prefs = await _prefs;
    await prefs.remove(_kBaseUrl);
    await prefs.remove(_kBiometric);
    // Clear any accidental legacy plaintext token key if present.
    await prefs.remove(_kTokenKey);
    try {
      await _secure.delete(key: _kTokenKey);
    } on PlatformException catch (e) {
      debugPrint('secure storage delete failed: ${e.code} ${e.message}');
    }
  }
}

/// In-memory store for widget/unit tests (token still not in a prefs map).
final class MemoryCredentialsStore implements CredentialsStore {
  StoredConnection? _connection;

  /// Exposes non-secret prefs surface for tests asserting token isolation.
  final Map<String, Object?> prefs = {};

  /// Secure map — only place the token may live in tests.
  final Map<String, String> secure = {};

  @override
  Future<StoredConnection?> load() async => _connection;

  @override
  Future<void> save({
    required String baseUrl,
    required String operatorToken,
    required bool biometricLockEnabled,
  }) async {
    prefs[_kBaseUrl] = baseUrl.trim();
    prefs[_kBiometric] = biometricLockEnabled;
    // Explicitly refuse to put token in prefs.
    prefs.remove(_kTokenKey);
    secure[_kTokenKey] = operatorToken;
    _connection = StoredConnection(
      baseUrl: baseUrl.trim(),
      operatorToken: operatorToken,
      biometricLockEnabled: biometricLockEnabled,
    );
  }

  @override
  Future<void> setBiometricLockEnabled(bool enabled) async {
    prefs[_kBiometric] = enabled;
    final c = _connection;
    if (c != null) {
      _connection = StoredConnection(
        baseUrl: c.baseUrl,
        operatorToken: c.operatorToken,
        biometricLockEnabled: enabled,
      );
    }
  }

  @override
  Future<void> clearAll() async {
    prefs.clear();
    secure.clear();
    _connection = null;
  }
}

final credentialsStoreProvider = Provider<CredentialsStore>((ref) {
  throw UnimplementedError(
    'credentialsStoreProvider must be overridden in main() or tests',
  );
});
