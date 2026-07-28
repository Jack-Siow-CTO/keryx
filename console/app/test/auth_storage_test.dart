import 'package:flutter_test/flutter_test.dart';
import 'package:keryx_console/core/credentials_store.dart';

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();

  group('MemoryCredentialsStore', () {
    test('token is never written to prefs map', () async {
      final store = MemoryCredentialsStore();
      await store.save(
        baseUrl: 'http://127.0.0.1:8787',
        operatorToken: 'super-secret-operator-token',
        biometricLockEnabled: true,
      );

      expect(store.prefs.containsKey('keryx.operator_token'), isFalse);
      expect(
        store.prefs.values,
        isNot(contains('super-secret-operator-token')),
      );
      expect(store.secure['keryx.operator_token'], 'super-secret-operator-token');
      expect(store.prefs['keryx.base_url'], 'http://127.0.0.1:8787');
      expect(store.prefs['keryx.biometric_lock'], isTrue);

      final loaded = await store.load();
      expect(loaded?.operatorToken, 'super-secret-operator-token');
    });

    test('clearAll removes secret and non-secrets', () async {
      final store = MemoryCredentialsStore();
      await store.save(
        baseUrl: 'http://example',
        operatorToken: 'tok',
        biometricLockEnabled: false,
      );
      await store.clearAll();
      expect(await store.load(), isNull);
      expect(store.secure, isEmpty);
      expect(store.prefs, isEmpty);
    });
  });
}
