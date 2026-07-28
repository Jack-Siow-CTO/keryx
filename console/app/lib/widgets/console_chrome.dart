import 'package:flutter/material.dart';

import '../theme/keryx_theme.dart';

/// Shared empty-state for rails and main pane.
class ConsoleEmptyState extends StatelessWidget {
  const ConsoleEmptyState({
    super.key,
    required this.icon,
    required this.title,
    required this.body,
    this.action,
  });

  final IconData icon;
  final String title;
  final String body;
  final Widget? action;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Center(
      child: ConstrainedBox(
        constraints: const BoxConstraints(maxWidth: 320),
        child: Padding(
          padding: const EdgeInsets.all(28),
          child: Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              Container(
                width: 56,
                height: 56,
                decoration: BoxDecoration(
                  color: theme.colorScheme.surfaceContainerHigh,
                  borderRadius: BorderRadius.circular(16),
                ),
                child: Icon(
                  icon,
                  size: 28,
                  color: theme.colorScheme.onSurfaceVariant,
                ),
              ),
              const SizedBox(height: 16),
              Text(
                title,
                textAlign: TextAlign.center,
                style: theme.textTheme.titleMedium,
              ),
              const SizedBox(height: 8),
              Text(
                body,
                textAlign: TextAlign.center,
                style: theme.textTheme.bodySmall,
              ),
              if (action != null) ...[
                const SizedBox(height: 16),
                action!,
              ],
            ],
          ),
        ),
      ),
    );
  }
}

/// Compact attention count pill (needs-you fill only for nonzero).
class AttentionBadge extends StatelessWidget {
  const AttentionBadge({super.key, required this.count});

  final int count;

  @override
  Widget build(BuildContext context) {
    if (count <= 0) return const SizedBox.shrink();
    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 7, vertical: 2),
      decoration: BoxDecoration(
        color: KeryxTheme.needsYou,
        borderRadius: BorderRadius.circular(999),
      ),
      child: Text(
        count > 99 ? '99+' : '$count',
        style: const TextStyle(
          color: Color(0xFFFFF8F4),
          fontSize: 11,
          fontWeight: FontWeight.w700,
          height: 1.2,
        ),
      ),
    );
  }
}

/// Rail section header with optional trailing actions.
class RailSectionHeader extends StatelessWidget {
  const RailSectionHeader({
    super.key,
    required this.title,
    this.trailing = const [],
  });

  final String title;
  final List<Widget> trailing;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Padding(
      padding: const EdgeInsets.fromLTRB(14, 12, 6, 6),
      child: Row(
        children: [
          Expanded(
            child: Text(
              title.toUpperCase(),
              style: theme.textTheme.labelSmall?.copyWith(
                fontWeight: FontWeight.w700,
                letterSpacing: 0.8,
                color: theme.colorScheme.onSurfaceVariant,
              ),
            ),
          ),
          ...trailing,
        ],
      ),
    );
  }
}

/// Soft status chip for Active Run / connectivity.
class StatusPill extends StatelessWidget {
  const StatusPill({
    super.key,
    required this.label,
    this.icon,
    this.tone = StatusPillTone.neutral,
  });

  final String label;
  final IconData? icon;
  final StatusPillTone tone;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final (Color bg, Color fg) = switch (tone) {
      StatusPillTone.attention => (
          KeryxTheme.needsYou.withValues(alpha: 0.14),
          KeryxTheme.needsYou,
        ),
      StatusPillTone.active => (
          theme.colorScheme.primary.withValues(alpha: 0.12),
          theme.colorScheme.primary,
        ),
      StatusPillTone.ok => (
          const Color(0xFF1B7A4E).withValues(alpha: 0.12),
          const Color(0xFF1B7A4E),
        ),
      StatusPillTone.danger => (
          theme.colorScheme.error.withValues(alpha: 0.12),
          theme.colorScheme.error,
        ),
      StatusPillTone.neutral => (
          theme.colorScheme.surfaceContainerHigh,
          theme.colorScheme.onSurfaceVariant,
        ),
    };

    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 5),
      decoration: BoxDecoration(
        color: bg,
        borderRadius: BorderRadius.circular(999),
      ),
      child: Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          if (icon != null) ...[
            Icon(icon, size: 14, color: fg),
            const SizedBox(width: 5),
          ],
          Flexible(
            child: Text(
              label,
              overflow: TextOverflow.ellipsis,
              style: theme.textTheme.labelMedium?.copyWith(
                color: fg,
                fontWeight: FontWeight.w600,
              ),
            ),
          ),
        ],
      ),
    );
  }
}

enum StatusPillTone { neutral, active, attention, ok, danger }

/// Inline error / warning banner without left stripe accent.
class ConsoleBanner extends StatelessWidget {
  const ConsoleBanner({
    super.key,
    required this.message,
    this.tone = StatusPillTone.danger,
    this.icon,
  });

  final String message;
  final StatusPillTone tone;
  final IconData? icon;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final bg = switch (tone) {
      StatusPillTone.danger => theme.colorScheme.errorContainer,
      StatusPillTone.attention => KeryxTheme.needsYou.withValues(alpha: 0.12),
      StatusPillTone.active => theme.colorScheme.primaryContainer,
      StatusPillTone.ok => const Color(0xFF1B7A4E).withValues(alpha: 0.12),
      StatusPillTone.neutral => theme.colorScheme.surfaceContainerHigh,
    };
    final fg = switch (tone) {
      StatusPillTone.danger => theme.colorScheme.onErrorContainer,
      StatusPillTone.ok => const Color(0xFF1B7A4E),
      StatusPillTone.attention => KeryxTheme.needsYou,
      _ => theme.colorScheme.onSurface,
    };
    final resolvedIcon = icon ??
        switch (tone) {
          StatusPillTone.danger => Icons.error_outline,
          StatusPillTone.attention => Icons.priority_high,
          StatusPillTone.ok => Icons.check_circle_outline,
          StatusPillTone.active => Icons.info_outline,
          StatusPillTone.neutral => Icons.info_outline,
        };

    return Material(
      color: bg,
      borderRadius: BorderRadius.circular(10),
      child: Padding(
        padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 10),
        child: Row(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Icon(resolvedIcon, size: 18, color: fg),
            const SizedBox(width: 10),
            Expanded(
              child: Text(
                message,
                style: theme.textTheme.bodySmall?.copyWith(color: fg),
              ),
            ),
          ],
        ),
      ),
    );
  }
}

/// Compact loading indicator for rails and panes.
class ConsoleLoader extends StatelessWidget {
  const ConsoleLoader({super.key, this.label});

  final String? label;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Center(
      child: Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          SizedBox(
            width: 22,
            height: 22,
            child: CircularProgressIndicator(
              strokeWidth: 2,
              color: theme.colorScheme.primary,
            ),
          ),
          if (label != null) ...[
            const SizedBox(height: 12),
            Text(label!, style: theme.textTheme.bodySmall),
          ],
        ],
      ),
    );
  }
}

/// Page scaffold for secondary full-screen tools (Memory, Skills, …).
class ConsolePageScaffold extends StatelessWidget {
  const ConsolePageScaffold({
    super.key,
    required this.title,
    required this.body,
    this.actions = const [],
    this.bottom,
  });

  final String title;
  final Widget body;
  final List<Widget> actions;
  final Widget? bottom;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Scaffold(
      backgroundColor: theme.colorScheme.surface,
      appBar: AppBar(
        title: Text(title),
        actions: actions,
      ),
      body: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Expanded(child: body),
          if (bottom != null)
            Material(
              color: theme.colorScheme.surfaceContainerLow,
              elevation: 0,
              child: DecoratedBox(
                decoration: BoxDecoration(
                  border: Border(
                    top: BorderSide(
                      color: theme.dividerTheme.color ?? theme.dividerColor,
                    ),
                  ),
                ),
                child: SafeArea(
                  top: false,
                  child: bottom!,
                ),
              ),
            ),
        ],
      ),
    );
  }
}

/// Row used in secondary lists: flat, no nested cards.
class ConsoleListRow extends StatelessWidget {
  const ConsoleListRow({
    super.key,
    required this.title,
    this.subtitle,
    this.leading,
    this.trailing,
    this.onTap,
    this.selected = false,
  });

  final String title;
  final String? subtitle;
  final Widget? leading;
  final Widget? trailing;
  final VoidCallback? onTap;
  final bool selected;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Padding(
      padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 2),
      child: Material(
        color: selected
            ? theme.colorScheme.primary.withValues(alpha: 0.10)
            : Colors.transparent,
        borderRadius: BorderRadius.circular(10),
        child: InkWell(
          borderRadius: BorderRadius.circular(10),
          onTap: onTap,
          child: Padding(
            padding: const EdgeInsets.fromLTRB(12, 12, 8, 12),
            child: Row(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                if (leading != null) ...[
                  leading!,
                  const SizedBox(width: 12),
                ],
                Expanded(
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Text(
                        title,
                        style: theme.textTheme.titleSmall?.copyWith(
                          fontWeight: FontWeight.w600,
                        ),
                      ),
                      if (subtitle != null) ...[
                        const SizedBox(height: 4),
                        Text(
                          subtitle!,
                          maxLines: 3,
                          overflow: TextOverflow.ellipsis,
                          style: theme.textTheme.bodySmall,
                        ),
                      ],
                    ],
                  ),
                ),
                if (trailing != null) ...[
                  const SizedBox(width: 8),
                  trailing!,
                ],
              ],
            ),
          ),
        ),
      ),
    );
  }
}

/// Section label for settings / forms.
class ConsoleSectionLabel extends StatelessWidget {
  const ConsoleSectionLabel(this.text, {super.key});

  final String text;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Padding(
      padding: const EdgeInsets.only(bottom: 10),
      child: Text(
        text,
        style: theme.textTheme.titleSmall?.copyWith(
          color: theme.colorScheme.onSurfaceVariant,
          letterSpacing: 0.2,
        ),
      ),
    );
  }
}
