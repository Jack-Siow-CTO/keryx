import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../theme/keryx_theme.dart';
import '../auth/auth_controller.dart';
import '../inbox/inbox_screen.dart';
import '../memory/memory_screen.dart';
import '../schedules/schedules_screen.dart';
import '../sessions/session_detail.dart';
import '../sessions/sessions_list.dart';
import '../settings/settings_screen.dart';
import '../skills/skills_screen.dart';

/// Dual-rail home: Inbox + Sessions simultaneously on wide (ADR 0014, 0020).
///
/// Layout breakpoints:
/// - **wide** (≥1100): Inbox rail + Sessions rail + main pane (both rails visible)
/// - **medium** (700–1099): single list rail (toggle Inbox/Sessions) + main
/// - **narrow** (<700): bottom nav stack Inbox · Sessions · More
class DualRailShell extends ConsumerStatefulWidget {
  const DualRailShell({super.key});

  @override
  ConsumerState<DualRailShell> createState() => _DualRailShellState();
}

class _DualRailShellState extends ConsumerState<DualRailShell> {
  /// 0 = Inbox focus, 1 = Sessions focus (medium/narrow).
  int _index = 0;

  static const _wideBreakpoint = 1100.0;
  static const _mediumBreakpoint = 700.0;

  @override
  Widget build(BuildContext context) {
    final width = MediaQuery.sizeOf(context).width;

    if (width >= _wideBreakpoint) {
      return _wideLayout(context);
    }
    if (width >= _mediumBreakpoint) {
      return _mediumLayout(context);
    }
    return _narrowLayout(context);
  }

  /// True dual-rail: Inbox and Sessions both visible.
  Widget _wideLayout(BuildContext context) {
    return Scaffold(
      appBar: AppBar(
        title: const Text('Keryx Console'),
        actions: _consoleActions(context),
      ),
      body: Row(
        children: [
          SizedBox(
            width: 280,
            child: Material(
              color: Theme.of(context).colorScheme.surfaceContainerLow,
              child: const InboxScreen(),
            ),
          ),
          const VerticalDivider(width: 1),
          SizedBox(
            width: 300,
            child: Material(
              color: Theme.of(context).colorScheme.surfaceContainerLow,
              child: const SessionsList(),
            ),
          ),
          const VerticalDivider(width: 1),
          const Expanded(child: SessionDetailPane()),
        ],
      ),
    );
  }

  /// Medium: list + main (one attention surface at a time).
  Widget _mediumLayout(BuildContext context) {
    return Scaffold(
      appBar: AppBar(
        title: const Text('Keryx Console'),
        actions: _consoleActions(context),
      ),
      body: Row(
        children: [
          SizedBox(
            width: 300,
            child: Material(
              color: Theme.of(context).colorScheme.surfaceContainerLow,
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.stretch,
                children: [
                  _RailHeader(
                    title: 'Inbox',
                    selected: _index == 0,
                    onTap: () => setState(() => _index = 0),
                  ),
                  _RailHeader(
                    title: 'Sessions',
                    selected: _index == 1,
                    onTap: () => setState(() => _index = 1),
                  ),
                  const Divider(height: 1),
                  Expanded(
                    child: _index == 0
                        ? const InboxScreen()
                        : const SessionsList(),
                  ),
                ],
              ),
            ),
          ),
          const VerticalDivider(width: 1),
          Expanded(
            child: _index == 0
                ? const _MainPlaceholder(tab: 0)
                : const SessionDetailPane(),
          ),
        ],
      ),
    );
  }

  Widget _narrowLayout(BuildContext context) {
    return Scaffold(
      appBar: AppBar(
        title: Text(
          _index == 0
              ? 'Inbox'
              : _index == 1
                  ? 'Sessions'
                  : 'More',
        ),
      ),
      body: switch (_index) {
        0 => const InboxScreen(),
        1 => SessionsList(
            onOpenSession: (id) => _pushSessionDetail(context),
          ),
        _ => MorePanel(onOpenSettings: () => _openSettings(context)),
      },
      bottomNavigationBar: NavigationBar(
        selectedIndex: _index,
        onDestinationSelected: (i) => setState(() => _index = i),
        destinations: const [
          NavigationDestination(
            icon: Icon(Icons.inbox_outlined),
            selectedIcon: Icon(Icons.inbox),
            label: 'Inbox',
          ),
          NavigationDestination(
            icon: Icon(Icons.forum_outlined),
            selectedIcon: Icon(Icons.forum),
            label: 'Sessions',
          ),
          NavigationDestination(
            icon: Icon(Icons.more_horiz),
            selectedIcon: Icon(Icons.more_horiz),
            label: 'More',
          ),
        ],
      ),
    );
  }

  List<Widget> _consoleActions(BuildContext context) {
    return [
      PopupMenuButton<String>(
        tooltip: 'More',
        onSelected: (v) {
          final page = switch (v) {
            'memory' => const MemoryScreen(),
            'schedules' => const SchedulesScreen(),
            'skills' => const SkillsScreen(),
            'settings' => const SettingsScreen(),
            _ => null,
          };
          if (page != null) {
            Navigator.of(context).push(
              MaterialPageRoute<void>(builder: (_) => page),
            );
          }
        },
        itemBuilder: (context) => const [
          PopupMenuItem(value: 'memory', child: Text('Memory')),
          PopupMenuItem(value: 'schedules', child: Text('Schedules')),
          PopupMenuItem(value: 'skills', child: Text('Skills')),
          PopupMenuItem(value: 'settings', child: Text('Settings')),
        ],
      ),
    ];
  }

  void _openSettings(BuildContext context) {
    Navigator.of(context).push(
      MaterialPageRoute<void>(builder: (_) => const SettingsScreen()),
    );
  }

  /// Narrow stack: Session detail is full-screen with back to Sessions list.
  void _pushSessionDetail(BuildContext context) {
    Navigator.of(context).push(
      MaterialPageRoute<void>(
        builder: (_) => Scaffold(
          appBar: AppBar(
            title: const Text('Session'),
          ),
          body: const SessionDetailPane(),
        ),
      ),
    );
  }
}

class _RailHeader extends StatelessWidget {
  const _RailHeader({
    required this.title,
    required this.selected,
    required this.onTap,
  });

  final String title;
  final bool selected;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return InkWell(
      onTap: onTap,
      child: Container(
        padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 14),
        color: selected
            ? theme.colorScheme.surfaceContainerHighest
            : Colors.transparent,
        child: Text(
          title,
          style: theme.textTheme.titleSmall?.copyWith(
            fontWeight: selected ? FontWeight.w700 : FontWeight.w500,
            color: selected
                ? theme.colorScheme.onSurface
                : theme.colorScheme.onSurfaceVariant,
          ),
        ),
      ),
    );
  }
}

class InboxPlaceholder extends StatelessWidget {
  const InboxPlaceholder({super.key});

  @override
  Widget build(BuildContext context) {
    return const _EmptyRail(
      icon: Icons.inbox_outlined,
      title: 'Inbox',
      body:
          'Cross-Session needs-you items (Approvals, failed Runs) will appear here. '
          'Nothing needs you yet.',
    );
  }
}

class _EmptyRail extends StatelessWidget {
  const _EmptyRail({
    required this.icon,
    required this.title,
    required this.body,
  });

  final IconData icon;
  final String title;
  final String body;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Center(
      child: Padding(
        padding: const EdgeInsets.all(24),
        child: Column(
          mainAxisAlignment: MainAxisAlignment.center,
          children: [
            Icon(icon, size: 40, color: theme.colorScheme.outline),
            const SizedBox(height: 12),
            Text(title, style: theme.textTheme.titleMedium),
            const SizedBox(height: 8),
            Text(
              body,
              textAlign: TextAlign.center,
              style: theme.textTheme.bodyMedium?.copyWith(
                color: theme.colorScheme.onSurfaceVariant,
              ),
            ),
          ],
        ),
      ),
    );
  }
}

class _MainPlaceholder extends StatelessWidget {
  const _MainPlaceholder({required this.tab});

  final int tab;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Center(
      child: Column(
        mainAxisAlignment: MainAxisAlignment.center,
        children: [
          Icon(
            Icons.auto_awesome_mosaic_outlined,
            size: 48,
            color: theme.colorScheme.outline,
          ),
          const SizedBox(height: 12),
          Text(
            tab == 0 ? 'Select an Inbox item' : 'Select a Session',
            style: theme.textTheme.titleMedium,
          ),
          const SizedBox(height: 8),
          Text(
            'Conversation and activity open here.',
            style: theme.textTheme.bodyMedium?.copyWith(
              color: theme.colorScheme.onSurfaceVariant,
            ),
          ),
          const SizedBox(height: 16),
          Container(
            width: 12,
            height: 12,
            decoration: const BoxDecoration(
              color: KeryxTheme.needsYou,
              shape: BoxShape.circle,
            ),
          ),
          const SizedBox(height: 4),
          Text(
            'Needs-you accent',
            style: theme.textTheme.labelSmall?.copyWith(
              color: KeryxTheme.needsYou,
            ),
          ),
        ],
      ),
    );
  }
}

class MorePanel extends ConsumerWidget {
  const MorePanel({super.key, required this.onOpenSettings});

  final VoidCallback onOpenSettings;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final baseUrl = ref.watch(authControllerProvider).baseUrl;
    return ListView(
      children: [
        ListTile(
          leading: const Icon(Icons.settings_outlined),
          title: const Text('Settings'),
          subtitle: Text(baseUrl ?? ''),
          onTap: onOpenSettings,
        ),
        ListTile(
          leading: const Icon(Icons.memory_outlined),
          title: const Text('Memory'),
          onTap: () => Navigator.of(context).push(
            MaterialPageRoute<void>(builder: (_) => const MemoryScreen()),
          ),
        ),
        ListTile(
          leading: const Icon(Icons.schedule_outlined),
          title: const Text('Schedules'),
          onTap: () => Navigator.of(context).push(
            MaterialPageRoute<void>(builder: (_) => const SchedulesScreen()),
          ),
        ),
        ListTile(
          leading: const Icon(Icons.extension_outlined),
          title: const Text('Skills'),
          onTap: () => Navigator.of(context).push(
            MaterialPageRoute<void>(builder: (_) => const SkillsScreen()),
          ),
        ),
      ],
    );
  }
}
