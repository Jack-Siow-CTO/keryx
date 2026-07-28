import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:keryx_console/app.dart';
import 'package:keryx_console/core/credentials_store.dart';
import 'package:keryx_console/core/device_lock.dart';
import 'package:keryx_console/features/auth/auth_controller.dart';
import 'package:keryx_console/features/shell/dual_rail_shell.dart';

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();

  group('onboarding', () {
    testWidgets('shows connect form when unconfigured', (tester) async {
      final store = MemoryCredentialsStore();
      final lock = FakeDeviceLock();

      await tester.pumpWidget(
        ProviderScope(
          overrides: [
            credentialsStoreProvider.overrideWithValue(store),
            deviceLockProvider.overrideWithValue(lock),
          ],
          child: const KeryxConsoleApp(),
        ),
      );
      await tester.pumpAndSettle();

      expect(find.text('Keryx Console'), findsOneWidget);
      expect(find.text('Connect'), findsOneWidget);
      expect(find.text('Worker base URL'), findsOneWidget);
      expect(find.text('Operator token'), findsOneWidget);
    });
  });

  group('dual-rail', () {
    testWidgets('wide shows Inbox and Sessions rails simultaneously',
        (tester) async {
      final store = MemoryCredentialsStore();
      await store.save(
        baseUrl: 'http://127.0.0.1:8787',
        operatorToken: 'test-token',
        biometricLockEnabled: false,
      );

      tester.view.physicalSize = const Size(1400, 900);
      tester.view.devicePixelRatio = 1.0;
      addTearDown(tester.view.resetPhysicalSize);
      addTearDown(tester.view.resetDevicePixelRatio);

      await tester.pumpWidget(
        ProviderScope(
          overrides: [
            credentialsStoreProvider.overrideWithValue(store),
            deviceLockProvider.overrideWithValue(FakeDeviceLock()),
          ],
          child: const KeryxConsoleApp(),
        ),
      );
      await tester.pumpAndSettle();

      expect(find.byType(DualRailShell), findsOneWidget);
      // Sessions rail is live list (may show empty/loading/error without Worker).
      expect(find.text('Sessions'), findsWidgets);
      expect(find.text('Inbox'), findsWidgets);
      expect(find.byType(NavigationBar), findsNothing);
    });

    testWidgets('narrow layout uses bottom navigation', (tester) async {
      final store = MemoryCredentialsStore();
      await store.save(
        baseUrl: 'http://127.0.0.1:8787',
        operatorToken: 'test-token',
        biometricLockEnabled: false,
      );

      tester.view.physicalSize = const Size(390, 844);
      tester.view.devicePixelRatio = 1.0;
      addTearDown(tester.view.resetPhysicalSize);
      addTearDown(tester.view.resetDevicePixelRatio);

      await tester.pumpWidget(
        ProviderScope(
          overrides: [
            credentialsStoreProvider.overrideWithValue(store),
            deviceLockProvider.overrideWithValue(FakeDeviceLock()),
          ],
          child: const KeryxConsoleApp(),
        ),
      );
      await tester.pumpAndSettle();

      expect(find.byType(NavigationBar), findsOneWidget);
      expect(find.text('Inbox'), findsWidgets);
    });
  });

  group('biometric lock', () {
    testWidgets('locked state requires successful device lock', (tester) async {
      final store = MemoryCredentialsStore();
      await store.save(
        baseUrl: 'http://127.0.0.1:8787',
        operatorToken: 'test-token',
        biometricLockEnabled: true,
      );
      final lock = FakeDeviceLock(succeed: true);

      await tester.pumpWidget(
        ProviderScope(
          overrides: [
            credentialsStoreProvider.overrideWithValue(store),
            deviceLockProvider.overrideWithValue(lock),
          ],
          child: const KeryxConsoleApp(),
        ),
      );
      await tester.pumpAndSettle();

      if (find.text('Console locked').evaluate().isNotEmpty) {
        await tester.tap(find.text('Unlock'));
        await tester.pumpAndSettle();
      }

      expect(lock.authenticateCalls, greaterThan(0));
      expect(find.byType(DualRailShell), findsOneWidget);
    });

    testWidgets('unsupported device lock does not fail-open', (tester) async {
      final store = MemoryCredentialsStore();
      await store.save(
        baseUrl: 'http://127.0.0.1:8787',
        operatorToken: 'test-token',
        biometricLockEnabled: true,
      );
      final lock = FakeDeviceLock(supported: false, succeed: true);

      await tester.pumpWidget(
        ProviderScope(
          overrides: [
            credentialsStoreProvider.overrideWithValue(store),
            deviceLockProvider.overrideWithValue(lock),
          ],
          child: const KeryxConsoleApp(),
        ),
      );
      await tester.pumpAndSettle();

      // Stay locked — never silent-success when unsupported.
      expect(find.byType(DualRailShell), findsNothing);
      expect(find.textContaining('cannot authenticate'), findsOneWidget);
    });
  });

  group('logout', () {
    testWidgets('logout clears credentials and returns to onboarding',
        (tester) async {
      final store = MemoryCredentialsStore();
      await store.save(
        baseUrl: 'http://127.0.0.1:8787',
        operatorToken: 'test-token',
        biometricLockEnabled: false,
      );

      await tester.pumpWidget(
        ProviderScope(
          overrides: [
            credentialsStoreProvider.overrideWithValue(store),
            deviceLockProvider.overrideWithValue(FakeDeviceLock()),
          ],
          child: const KeryxConsoleApp(),
        ),
      );
      await tester.pumpAndSettle();
      expect(find.byType(DualRailShell), findsOneWidget);

      final container = ProviderScope.containerOf(
        tester.element(find.byType(DualRailShell)),
      );
      await container.read(authControllerProvider.notifier).logout();
      await tester.pumpAndSettle();

      expect(await store.load(), isNull);
      expect(find.text('Connect'), findsOneWidget);
    });
  });
}
