import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:keryx_api/keryx_api.dart';

import '../../widgets/console_chrome.dart';
import '../auth/auth_controller.dart';

final skillsProvider =
    FutureProvider.autoDispose<List<SkillSummary>>((ref) async {
  final client = ref.watch(authControllerProvider.notifier).client;
  if (client == null) return const [];
  return client.listSkills();
});

/// Read-mostly Skills hub: packages on the Worker skills root (ADR 0030 / #82).
///
/// After learning-loop Approve writes a package, refresh shows it here with an
/// Available indicator so the operator can verify list without a CMS.
class SkillsScreen extends ConsumerWidget {
  const SkillsScreen({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final async = ref.watch(skillsProvider);
    final theme = Theme.of(context);

    return Scaffold(
      appBar: AppBar(
        title: const Text('Skills'),
        actions: [
          IconButton(
            tooltip: 'Refresh',
            icon: const Icon(Icons.refresh, size: 20),
            onPressed: () => ref.invalidate(skillsProvider),
          ),
        ],
      ),
      body: async.when(
        loading: () => const ConsoleLoader(label: 'Loading Skills…'),
        error: (e, _) => Padding(
          padding: const EdgeInsets.all(16),
          child: ConsoleBanner(message: '$e'),
        ),
        data: (skills) {
          if (skills.isEmpty) {
            return const ConsoleEmptyState(
              icon: Icons.extension_outlined,
              title: 'No Skill packages',
              body:
                  'Approve a skill proposal or install packages under the Worker skills root. Runs load them via skill_load.',
            );
          }
          return Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              Padding(
                padding: const EdgeInsets.fromLTRB(16, 12, 16, 4),
                child: Text(
                  'Packages on the Worker · available for skill_load. Read-only — authoring stays agent/CLI/filesystem.',
                  style: theme.textTheme.bodySmall?.copyWith(
                    color: theme.colorScheme.onSurfaceVariant,
                  ),
                ),
              ),
              Expanded(
                child: ListView.builder(
                  padding: const EdgeInsets.symmetric(vertical: 8),
                  itemCount: skills.length,
                  itemBuilder: (context, i) {
                    final s = skills[i];
                    return ConsoleListRow(
                      leading: Icon(
                        Icons.extension_outlined,
                        color: theme.colorScheme.onSurfaceVariant,
                      ),
                      title: s.name,
                      subtitle: 'Package ready for skill_load',
                      trailing: Row(
                        mainAxisSize: MainAxisSize.min,
                        children: [
                          const StatusPill(
                            icon: Icons.check_circle_outline,
                            label: 'Available',
                            tone: StatusPillTone.ok,
                          ),
                          const SizedBox(width: 4),
                          Icon(
                            Icons.chevron_right,
                            color: theme.colorScheme.onSurfaceVariant,
                          ),
                        ],
                      ),
                      onTap: () async {
                        final client =
                            ref.read(authControllerProvider.notifier).client;
                        if (client == null) return;
                        try {
                          final detail = await client.getSkill(s.name);
                          if (!context.mounted) return;
                          await Navigator.of(context).push(
                            MaterialPageRoute<void>(
                              builder: (_) =>
                                  SkillDetailPage(detail: detail),
                            ),
                          );
                        } catch (e) {
                          if (!context.mounted) return;
                          ScaffoldMessenger.of(context).showSnackBar(
                            SnackBar(content: Text('$e')),
                          );
                        }
                      },
                    );
                  },
                ),
              ),
            ],
          );
        },
      ),
    );
  }
}

/// Read-only package content (no CMS / IDE).
class SkillDetailPage extends StatelessWidget {
  const SkillDetailPage({super.key, required this.detail});

  final SkillDetail detail;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Scaffold(
      appBar: AppBar(title: Text(detail.name)),
      body: ColoredBox(
        color: theme.colorScheme.surfaceContainerLowest,
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            Padding(
              padding: const EdgeInsets.fromLTRB(16, 12, 16, 0),
              child: Wrap(
                spacing: 8,
                runSpacing: 6,
                children: [
                  StatusPill(
                    icon: Icons.extension_outlined,
                    label: detail.name,
                    tone: StatusPillTone.neutral,
                  ),
                  const StatusPill(
                    icon: Icons.check_circle_outline,
                    label: 'Available for load',
                    tone: StatusPillTone.ok,
                  ),
                ],
              ),
            ),
            Expanded(
              child: SingleChildScrollView(
                padding: const EdgeInsets.fromLTRB(20, 16, 20, 32),
                child: SelectableText(
                  detail.content,
                  style: theme.textTheme.bodyMedium?.copyWith(
                    fontFamily: 'monospace',
                    fontSize: 13,
                    height: 1.45,
                  ),
                ),
              ),
            ),
          ],
        ),
      ),
    );
  }
}
