import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:keryx_api/keryx_api.dart';

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
    return Scaffold(
      appBar: AppBar(title: const Text('Skills')),
      body: async.when(
        loading: () => const Center(child: CircularProgressIndicator()),
        error: (e, _) => Center(child: Text('$e')),
        data: (skills) {
          if (skills.isEmpty) {
            return const Center(
              child: Text('No Skill packages on Worker skills root.'),
            );
          }
          return ListView.builder(
            itemCount: skills.length,
            itemBuilder: (context, i) {
              final s = skills[i];
              return ListTile(
                title: Text(s.name),
                trailing: const Icon(Icons.chevron_right),
                onTap: () async {
                  final client =
                      ref.read(authControllerProvider.notifier).client;
                  if (client == null) return;
                  final detail = await client.getSkill(s.name);
                  if (!context.mounted) return;
                  await Navigator.of(context).push(
                    MaterialPageRoute<void>(
                      builder: (_) => Scaffold(
                        appBar: AppBar(title: Text(detail.name)),
                        body: SingleChildScrollView(
                          padding: const EdgeInsets.all(16),
                          child: SelectableText(detail.content),
                        ),
                      ),
                    ),
                  );
                },
              );
            },
          );
        },
      ),
    );
  }
}
