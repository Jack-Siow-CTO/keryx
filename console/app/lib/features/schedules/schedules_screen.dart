import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:keryx_api/keryx_api.dart';

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
    return Scaffold(
      appBar: AppBar(title: const Text('Schedules')),
      body: Column(
        children: [
          Expanded(
            child: async.when(
              loading: () => const Center(child: CircularProgressIndicator()),
              error: (e, _) => Center(child: Text('$e')),
              data: (items) => ListView.builder(
                itemCount: items.length,
                itemBuilder: (context, i) {
                  final s = items[i];
                  return ListTile(
                    title: Text(s.goal),
                    subtitle: Text(
                      '${s.status} · every ${s.intervalSecs}s · next ${s.nextFireAt}',
                    ),
                    trailing: Wrap(
                      children: [
                        if (s.status == 'active')
                          IconButton(
                            icon: const Icon(Icons.pause),
                            onPressed: () async {
                              await ref
                                  .read(authControllerProvider.notifier)
                                  .client
                                  ?.pauseSchedule(s.id);
                              ref.invalidate(schedulesProvider);
                            },
                          ),
                        if (s.status == 'paused')
                          IconButton(
                            icon: const Icon(Icons.play_arrow),
                            onPressed: () async {
                              await ref
                                  .read(authControllerProvider.notifier)
                                  .client
                                  ?.resumeSchedule(s.id);
                              ref.invalidate(schedulesProvider);
                            },
                          ),
                        IconButton(
                          icon: const Icon(Icons.delete_outline),
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
              ),
            ),
          ),
          const Divider(height: 1),
          Padding(
            padding: const EdgeInsets.all(12),
            child: Column(
              children: [
                TextField(
                  controller: _goal,
                  decoration: const InputDecoration(labelText: 'Goal'),
                ),
                TextField(
                  controller: _interval,
                  decoration: const InputDecoration(
                    labelText: 'Interval seconds',
                  ),
                  keyboardType: TextInputType.number,
                ),
                const SizedBox(height: 8),
                FilledButton(
                  onPressed: () async {
                    final client =
                        ref.read(authControllerProvider.notifier).client;
                    if (client == null) return;
                    final secs = int.tryParse(_interval.text) ?? 3600;
                    final now =
                        DateTime.now().millisecondsSinceEpoch ~/ 1000;
                    await client.createSchedule(
                      goal: _goal.text.trim(),
                      intervalSecs: secs,
                      nextFireAt: now + secs,
                    );
                    _goal.clear();
                    ref.invalidate(schedulesProvider);
                  },
                  child: const Text('Create Schedule'),
                ),
              ],
            ),
          ),
        ],
      ),
    );
  }
}
