import 'package:flutter/material.dart';

/// Keryx operator visual system (ADR 0020): neutral chrome, system light/dark,
/// single "needs you" accent — not a Slack skin.
abstract final class KeryxTheme {
  /// Attention / needs-you accent (Approvals, Inbox badges).
  static const Color needsYou = Color(0xFFE85D04);

  static ThemeData light() {
    final base = ColorScheme.fromSeed(
      seedColor: const Color(0xFF2F6FED),
      brightness: Brightness.light,
    );
    return ThemeData(
      useMaterial3: true,
      colorScheme: base.copyWith(tertiary: needsYou),
      visualDensity: VisualDensity.comfortable,
      inputDecorationTheme: const InputDecorationTheme(
        border: OutlineInputBorder(),
      ),
    );
  }

  static ThemeData dark() {
    final base = ColorScheme.fromSeed(
      seedColor: const Color(0xFF6B9BFF),
      brightness: Brightness.dark,
    );
    return ThemeData(
      useMaterial3: true,
      colorScheme: base.copyWith(tertiary: needsYou),
      visualDensity: VisualDensity.comfortable,
      inputDecorationTheme: const InputDecorationTheme(
        border: OutlineInputBorder(),
      ),
    );
  }
}
