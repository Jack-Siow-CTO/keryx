import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'features/auth/auth_controller.dart';
import 'features/auth/lock_gate.dart';
import 'features/auth/onboarding_screen.dart';
import 'features/shell/dual_rail_shell.dart';
import 'theme/keryx_theme.dart';

/// Root Console widget — auth → optional lock → dual-rail shell.
class KeryxConsoleApp extends ConsumerWidget {
  const KeryxConsoleApp({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final auth = ref.watch(authControllerProvider);

    return MaterialApp(
      title: 'Keryx Console',
      debugShowCheckedModeBanner: false,
      theme: KeryxTheme.light(),
      darkTheme: KeryxTheme.dark(),
      themeMode: ThemeMode.system,
      home: switch (auth.status) {
        AuthStatus.unknown => const _BootSplash(),
        AuthStatus.unconfigured => const OnboardingScreen(),
        AuthStatus.locked => const LockGate(),
        AuthStatus.ready => const DualRailShell(),
      },
    );
  }
}

class _BootSplash extends StatelessWidget {
  const _BootSplash();

  @override
  Widget build(BuildContext context) {
    return const Scaffold(
      body: Center(child: CircularProgressIndicator()),
    );
  }
}
