import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:keryx_api/keryx_api.dart';

import '../../widgets/console_chrome.dart';
import '../auth/auth_controller.dart';
import '../memory/memory_screen.dart';
import '../schedules/schedules_screen.dart';
import '../settings/settings_screen.dart';
import '../skills/skills_screen.dart';

/// Global operator surfaces: Memory, Skills, Schedules, Settings (ADR 0031).
/// Not a peer rail of the chat list — opened from profile/avatar.
class ProfileHubPage extends ConsumerWidget {
  const ProfileHubPage({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final auth = ref.watch(authControllerProvider);
    final theme = Theme.of(context);
    final conn = auth.lastConnectivity;

    return ConsolePageScaffold(
      title: 'Profile',
      body: ListView(
        padding: const EdgeInsets.symmetric(vertical: 8, horizontal: 4),
        children: [
          Padding(
            padding: const EdgeInsets.fromLTRB(16, 8, 16, 16),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(
                  'Keryx',
                  style: theme.textTheme.titleMedium,
                ),
                const SizedBox(height: 4),
                Text(
                  auth.baseUrl ?? 'Not connected',
                  style: theme.textTheme.bodySmall,
                  maxLines: 2,
                  overflow: TextOverflow.ellipsis,
                ),
                if (conn != null) ...[
                  const SizedBox(height: 10),
                  StatusPill(
                    label: conn.isOk ? 'Worker connected' : _connLabel(conn),
                    icon: conn.isOk
                        ? Icons.cloud_done_outlined
                        : Icons.cloud_off_outlined,
                    tone: conn.isOk ? StatusPillTone.ok : StatusPillTone.danger,
                  ),
                ],
              ],
            ),
          ),
          const Divider(height: 1),
          ConsoleListRow(
            leading: Icon(
              Icons.memory_outlined,
              color: theme.colorScheme.onSurfaceVariant,
            ),
            title: 'Memory',
            subtitle: 'Durable notes on the Worker',
            onTap: () => _open(context, const MemoryScreen()),
          ),
          ConsoleListRow(
            leading: Icon(
              Icons.extension_outlined,
              color: theme.colorScheme.onSurfaceVariant,
            ),
            title: 'Skills',
            subtitle: 'Packages available to Runs',
            onTap: () => _open(context, const SkillsScreen()),
          ),
          ConsoleListRow(
            leading: Icon(
              Icons.schedule_outlined,
              color: theme.colorScheme.onSurfaceVariant,
            ),
            title: 'Schedules',
            subtitle: 'Timed goals that start Runs',
            onTap: () => _open(context, const SchedulesScreen()),
          ),
          ConsoleListRow(
            leading: Icon(
              Icons.settings_outlined,
              color: theme.colorScheme.onSurfaceVariant,
            ),
            title: 'Settings',
            subtitle: 'Worker URL, token, device lock, model defaults',
            onTap: () => _open(context, const SettingsScreen()),
          ),
        ],
      ),
    );
  }

  void _open(BuildContext context, Widget page) {
    Navigator.of(context).push(
      MaterialPageRoute<void>(builder: (_) => page),
    );
  }

  String _connLabel(ConnectivityResult conn) {
    return switch (conn.kind) {
      ConnectivityKind.unreachable => 'Worker unreachable',
      ConnectivityKind.authFailure => 'Auth failed',
      ConnectivityKind.unexpected => 'Unexpected response',
      ConnectivityKind.ok => 'Worker connected',
    };
  }
}
