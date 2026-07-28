import 'package:flutter/material.dart';

/// Keryx Console visual system (ADR 0020 + product register).
///
/// Restrained: cool-slate neutrals + one needs-you accent. System light/dark.
abstract final class KeryxTheme {
  /// Attention only: Approvals, Inbox badges, Active-needs-you moments.
  static const Color needsYou = Color(0xFFD85A1A);

  static const Color _seedLight = Color(0xFF3D6B8C);
  static const Color _seedDark = Color(0xFF7AA3C0);

  static ThemeData light() {
    final scheme = ColorScheme.fromSeed(
      seedColor: _seedLight,
      brightness: Brightness.light,
      dynamicSchemeVariant: DynamicSchemeVariant.tonalSpot,
    ).copyWith(
      tertiary: needsYou,
      surface: const Color(0xFFF6F7F8),
      surfaceContainerLowest: const Color(0xFFFCFCFD),
      surfaceContainerLow: const Color(0xFFEEF1F3),
      surfaceContainer: const Color(0xFFE8ECF0),
      surfaceContainerHigh: const Color(0xFFE1E6EB),
      surfaceContainerHighest: const Color(0xFFD9DFE5),
      outline: const Color(0xFFB8C0C8),
      outlineVariant: const Color(0xFFD0D6DC),
    );
    return _base(scheme, Brightness.light);
  }

  static ThemeData dark() {
    final scheme = ColorScheme.fromSeed(
      seedColor: _seedDark,
      brightness: Brightness.dark,
      dynamicSchemeVariant: DynamicSchemeVariant.tonalSpot,
    ).copyWith(
      tertiary: needsYou,
      surface: const Color(0xFF161A1E),
      surfaceContainerLowest: const Color(0xFF111417),
      surfaceContainerLow: const Color(0xFF1C2126),
      surfaceContainer: const Color(0xFF22282E),
      surfaceContainerHigh: const Color(0xFF2A3138),
      surfaceContainerHighest: const Color(0xFF343C44),
      outline: const Color(0xFF5C6670),
      outlineVariant: const Color(0xFF3A424A),
    );
    return _base(scheme, Brightness.dark);
  }

  static ThemeData _base(ColorScheme scheme, Brightness brightness) {
    final textTheme = Typography.material2021(platform: TargetPlatform.macOS)
        .black
        .apply(
          bodyColor: scheme.onSurface,
          displayColor: scheme.onSurface,
          fontFamily: '.SF Pro Text',
          fontFamilyFallback: const [
            'Segoe UI',
            'Roboto',
            'Helvetica Neue',
            'Arial',
            'sans-serif',
          ],
        );

    return ThemeData(
      useMaterial3: true,
      brightness: brightness,
      colorScheme: scheme,
      scaffoldBackgroundColor: scheme.surface,
      visualDensity: VisualDensity.comfortable,
      splashFactory: InkSparkle.splashFactory,
      textTheme: textTheme.copyWith(
        bodyLarge: textTheme.bodyLarge?.copyWith(fontSize: 15, height: 1.45),
        bodyMedium: textTheme.bodyMedium?.copyWith(fontSize: 14, height: 1.4),
        bodySmall: textTheme.bodySmall?.copyWith(
          fontSize: 12.5,
          height: 1.35,
          color: scheme.onSurfaceVariant,
        ),
        titleLarge: textTheme.titleLarge?.copyWith(
          fontSize: 20,
          fontWeight: FontWeight.w600,
          letterSpacing: -0.3,
        ),
        titleMedium: textTheme.titleMedium?.copyWith(
          fontSize: 16,
          fontWeight: FontWeight.w600,
          letterSpacing: -0.2,
        ),
        titleSmall: textTheme.titleSmall?.copyWith(
          fontSize: 13,
          fontWeight: FontWeight.w600,
          letterSpacing: 0.1,
        ),
        labelLarge: textTheme.labelLarge?.copyWith(
          fontSize: 13,
          fontWeight: FontWeight.w600,
        ),
        labelMedium: textTheme.labelMedium?.copyWith(
          fontSize: 12,
          fontWeight: FontWeight.w500,
        ),
        labelSmall: textTheme.labelSmall?.copyWith(
          fontSize: 11,
          fontWeight: FontWeight.w500,
          letterSpacing: 0.2,
        ),
      ),
      appBarTheme: AppBarTheme(
        elevation: 0,
        scrolledUnderElevation: 0.5,
        centerTitle: false,
        backgroundColor: scheme.surface,
        foregroundColor: scheme.onSurface,
        titleTextStyle: textTheme.titleMedium?.copyWith(
          fontWeight: FontWeight.w600,
          fontSize: 16,
          color: scheme.onSurface,
        ),
      ),
      navigationBarTheme: NavigationBarThemeData(
        height: 64,
        backgroundColor: scheme.surfaceContainerLow,
        indicatorColor: scheme.primary.withValues(alpha: 0.14),
        labelTextStyle: WidgetStateProperty.resolveWith((states) {
          final selected = states.contains(WidgetState.selected);
          return TextStyle(
            fontSize: 12,
            fontWeight: selected ? FontWeight.w600 : FontWeight.w500,
          );
        }),
      ),
      dividerTheme: DividerThemeData(
        color: scheme.outlineVariant.withValues(alpha: 0.85),
        thickness: 1,
        space: 1,
      ),
      chipTheme: ChipThemeData(
        side: BorderSide(color: scheme.outlineVariant),
        shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(8)),
        labelStyle: textTheme.labelMedium,
        padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 0),
      ),
      cardTheme: CardThemeData(
        elevation: 0,
        color: scheme.surfaceContainerLow,
        shape: RoundedRectangleBorder(
          borderRadius: BorderRadius.circular(10),
          side: BorderSide(color: scheme.outlineVariant.withValues(alpha: 0.7)),
        ),
        margin: EdgeInsets.zero,
      ),
      filledButtonTheme: FilledButtonThemeData(
        style: FilledButton.styleFrom(
          elevation: 0,
          padding: const EdgeInsets.symmetric(horizontal: 18, vertical: 12),
          shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(10)),
        ),
      ),
      outlinedButtonTheme: OutlinedButtonThemeData(
        style: OutlinedButton.styleFrom(
          padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 12),
          shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(10)),
        ),
      ),
      inputDecorationTheme: InputDecorationTheme(
        filled: true,
        fillColor: scheme.surfaceContainerLowest,
        border: OutlineInputBorder(
          borderRadius: BorderRadius.circular(10),
          borderSide: BorderSide(color: scheme.outlineVariant),
        ),
        enabledBorder: OutlineInputBorder(
          borderRadius: BorderRadius.circular(10),
          borderSide: BorderSide(color: scheme.outlineVariant),
        ),
        focusedBorder: OutlineInputBorder(
          borderRadius: BorderRadius.circular(10),
          borderSide: BorderSide(color: scheme.primary, width: 1.5),
        ),
        contentPadding:
            const EdgeInsets.symmetric(horizontal: 14, vertical: 12),
        isDense: true,
      ),
      listTileTheme: ListTileThemeData(
        dense: true,
        contentPadding: const EdgeInsets.symmetric(horizontal: 12, vertical: 2),
        shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(8)),
        selectedTileColor: scheme.primary.withValues(alpha: 0.10),
        selectedColor: scheme.onSurface,
        iconColor: scheme.onSurfaceVariant,
      ),
      snackBarTheme: SnackBarThemeData(
        behavior: SnackBarBehavior.floating,
        shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(10)),
      ),
      tooltipTheme: TooltipThemeData(
        waitDuration: const Duration(milliseconds: 400),
        decoration: BoxDecoration(
          color: scheme.inverseSurface,
          borderRadius: BorderRadius.circular(6),
        ),
      ),
    );
  }
}
