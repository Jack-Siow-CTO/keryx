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
                  'Install packages under the Worker skills root. Runs can load them via tools.',
            );
          }
          return ListView.builder(
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
                subtitle: 'Open to read package content on the Worker',
                trailing: Icon(
                  Icons.chevron_right,
                  color: theme.colorScheme.onSurfaceVariant,
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
                        builder: (_) => _SkillDetailPage(detail: detail),
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
          );
        },
      ),
    );
  }
}

class _SkillDetailPage extends StatelessWidget {
  const _SkillDetailPage({required this.detail});

  final SkillDetail detail;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Scaffold(
      appBar: AppBar(title: Text(detail.name)),
      body: ColoredBox(
        color: theme.colorScheme.surfaceContainerLowest,
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
    );
  }
}
