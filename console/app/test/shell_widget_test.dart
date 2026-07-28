import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:keryx_console/app.dart';
import 'package:keryx_console/core/credentials_store.dart';
import 'package:keryx_console/core/device_lock.dart';
import 'package:keryx_console/features/auth/auth_controller.dart';
import 'package:keryx_console/features/shell/messaging_shell.dart';
import 'package:keryx_console/features/shell/profile_hub.dart';

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

      expect(find.text('Keryx'), findsOneWidget);
      expect(find.text('Connect'), findsOneWidget);
      expect(find.text('Worker base URL'), findsOneWidget);
      expect(find.text('Operator token'), findsOneWidget);
    });
  });

  group('messaging shell', () {
    testWidgets('wide shows chat list and thread regions simultaneously',
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

      expect(find.byType(MessagingShell), findsOneWidget);
      // Chat list home — not dual-rail INBOX + SESSIONS peer rails.
      expect(find.text('Chats'), findsOneWidget);
      expect(find.text('Needs you'), findsOneWidget);
      expect(find.text('SESSIONS'), findsOneWidget);
      expect(find.text('Select a chat'), findsOneWidget);
      // Dual-rail permanent Inbox column must not return.
      expect(find.text('INBOX'), findsNothing);
      expect(find.byType(NavigationBar), findsNothing);
    });

    testWidgets('narrow is list-first without dual permanent rails',
        (tester) async {
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

      expect(find.byType(MessagingShell), findsOneWidget);
      expect(find.text('Chats'), findsWidgets);
      expect(find.text('Needs you'), findsOneWidget);
      // No dual-rail bottom nav (Inbox | Sessions | More).
      expect(find.byType(NavigationBar), findsNothing);
      expect(find.text('Inbox'), findsNothing);
    });

    testWidgets('profile hub opens Memory / Settings destinations',
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

      await tester.tap(find.byTooltip('Profile and tools'));
      await tester.pumpAndSettle();

      expect(find.byType(ProfileHubPage), findsOneWidget);
      expect(find.text('Memory'), findsOneWidget);
      expect(find.text('Skills'), findsOneWidget);
      expect(find.text('Schedules'), findsOneWidget);
      expect(find.text('Settings'), findsOneWidget);
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
      expect(find.byType(MessagingShell), findsOneWidget);
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

      expect(find.byType(MessagingShell), findsNothing);
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
      expect(find.byType(MessagingShell), findsOneWidget);

      final container = ProviderScope.containerOf(
        tester.element(find.byType(MessagingShell)),
      );
      await container.read(authControllerProvider.notifier).logout();
      await tester.pumpAndSettle();

      expect(await store.load(), isNull);
      expect(find.text('Connect'), findsOneWidget);
    });
  });
}
