import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:local_auth/local_auth.dart';

/// Optional biometric / device-credential gate (ADR 0021).
///
/// When the operator enables the lock, [authenticate] must never silently
/// succeed on unsupported platforms (fail-open is a security bug).
abstract class DeviceLock {
  Future<bool> get isSupported;

  /// Prompt the user. Returns true only when the OS actually authenticated.
  /// Returns false when unsupported, cancelled, or failed.
  Future<bool> authenticate({String reason = 'Unlock Keryx Console'});
}

final class LocalAuthDeviceLock implements DeviceLock {
  LocalAuthDeviceLock({LocalAuthentication? auth})
      : _auth = auth ?? LocalAuthentication();

  final LocalAuthentication _auth;

  @override
  Future<bool> get isSupported async {
    try {
      return await _auth.isDeviceSupported();
    } catch (_) {
      return false;
    }
  }

  @override
  Future<bool> authenticate({
    String reason = 'Unlock Keryx Console',
  }) async {
    try {
      final supported = await isSupported;
      // Fail closed when lock is requested but device cannot challenge.
      if (!supported) return false;
      return await _auth.authenticate(
        localizedReason: reason,
        options: const AuthenticationOptions(
          biometricOnly: false,
          stickyAuth: true,
        ),
      );
    } catch (_) {
      return false;
    }
  }
}

/// Test double: configurable support and outcome (never silent-success unless set).
final class FakeDeviceLock implements DeviceLock {
  FakeDeviceLock({this.supported = true, this.succeed = true});

  bool supported;
  bool succeed;
  int authenticateCalls = 0;

  @override
  Future<bool> get isSupported async => supported;

  @override
  Future<bool> authenticate({
    String reason = 'Unlock Keryx Console',
  }) async {
    authenticateCalls++;
    if (!supported) return false;
    return succeed;
  }
}

final deviceLockProvider = Provider<DeviceLock>((ref) {
  throw UnimplementedError(
    'deviceLockProvider must be overridden in main() or tests',
  );
});
