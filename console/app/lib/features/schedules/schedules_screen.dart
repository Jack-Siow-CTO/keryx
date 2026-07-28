import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:keryx_api/keryx_api.dart';

import '../../widgets/console_chrome.dart';
import '../auth/auth_controller.dart';

final schedulesProvider =
    FutureProvider.autoDispose<List<ScheduleRecord>>((ref) async {
  final client = ref.watch(authControllerProvider.notifier).client;
  if (client == null) return const [];
  return client.listSchedules();
});

class SchedulesScreen extends ConsumerStatefulWidget {
  const SchedulesScreen({super.key});

  @override
  ConsumerState<SchedulesScreen> createState() => _SchedulesScreenState();
}

class _SchedulesScreenState extends ConsumerState<SchedulesScreen> {
  final _goal = TextEditingController();
  final _interval = TextEditingController(text: '3600');

  @override
  void dispose() {
    _goal.dispose();
    _interval.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final async = ref.watch(schedulesProvider);

    return ConsolePageScaffold(
      title: 'Schedules',
      actions: [
        IconButton(
          tooltip: 'Refresh',
          icon: const Icon(Icons.refresh, size: 20),
          onPressed: () => ref.invalidate(schedulesProvider),
        ),
      ],
      body: async.when(
        loading: () => const ConsoleLoader(label: 'Loading Schedules…'),
        error: (e, _) => Padding(
          padding: const EdgeInsets.all(16),
          child: ConsoleBanner(message: '$e'),
        ),
        data: (items) {
          if (items.isEmpty) {
            return const ConsoleEmptyState(
              icon: Icons.schedule_outlined,
              title: 'No Schedules',
              body:
                  'Create a timed goal. When it fires, the Worker starts a Run on the default Session path.',
            );
          }
          return ListView.builder(
            padding: const EdgeInsets.only(bottom: 12, top: 4),
            itemCount: items.length,
            itemBuilder: (context, i) {
              final s = items[i];
              final active = s.status == 'active';
              return ConsoleListRow(
                title: s.goal,
                subtitle:
                    'Every ${s.intervalSecs}s · next ${s.nextFireAt}',
                leading: Icon(
                  active ? Icons.play_circle_outline : Icons.pause_circle_outline,
                  color: active
                      ? Theme.of(context).colorScheme.primary
                      : Theme.of(context).colorScheme.onSurfaceVariant,
                ),
                trailing: Row(
                  mainAxisSize: MainAxisSize.min,
                  children: [
                    StatusPill(
                      label: s.status,
                      tone: active
                          ? StatusPillTone.active
                          : StatusPillTone.neutral,
                    ),
                    if (active)
                      IconButton(
                        tooltip: 'Pause',
                        icon: const Icon(Icons.pause, size: 18),
                        visualDensity: VisualDensity.compact,
                        onPressed: () async {
                          await ref
                              .read(authControllerProvider.notifier)
                              .client
                              ?.pauseSchedule(s.id);
                          ref.invalidate(schedulesProvider);
                        },
                      )
                    else
                      IconButton(
                        tooltip: 'Resume',
                        icon: const Icon(Icons.play_arrow, size: 18),
                        visualDensity: VisualDensity.compact,
                        onPressed: () async {
                          await ref
                              .read(authControllerProvider.notifier)
                              .client
                              ?.resumeSchedule(s.id);
                          ref.invalidate(schedulesProvider);
                        },
                      ),
                    IconButton(
                      tooltip: 'Delete',
                      icon: const Icon(Icons.delete_outline, size: 18),
                      visualDensity: VisualDensity.compact,
                      onPressed: () async {
                        await ref
                            .read(authControllerProvider.notifier)
                            .client
                            ?.deleteSchedule(s.id);
                        ref.invalidate(schedulesProvider);
                      },
                    ),
                  ],
                ),
              );
            },
          );
        },
      ),
      bottom: Padding(
        padding: const EdgeInsets.fromLTRB(16, 12, 16, 14),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          mainAxisSize: MainAxisSize.min,
          children: [
            TextField(
              controller: _goal,
              decoration: const InputDecoration(
                labelText: 'Goal',
                hintText: 'What should the Run do?',
              ),
            ),
            const SizedBox(height: 10),
            TextField(
              controller: _interval,
              decoration: const InputDecoration(
                labelText: 'Interval (seconds)',
              ),
              keyboardType: TextInputType.number,
            ),
            const SizedBox(height: 12),
            Align(
              alignment: Alignment.centerRight,
              child: FilledButton.icon(
                onPressed: () async {
                  final client =
                      ref.read(authControllerProvider.notifier).client;
                  if (client == null) return;
                  final secs = int.tryParse(_interval.text) ?? 3600;
                  final now = DateTime.now().millisecondsSinceEpoch ~/ 1000;
                  await client.createSchedule(
                    goal: _goal.text.trim(),
                    intervalSecs: secs,
                    nextFireAt: now + secs,
                  );
                  _goal.clear();
                  ref.invalidate(schedulesProvider);
                },
                icon: const Icon(Icons.add, size: 18),
                label: const Text('Create Schedule'),
              ),
            ),
          ],
        ),
      ),
    );
  }
}
